//! Assembling one answer out of two retrievers.
//!
//! Gate-free by design: the vector side arrives as plain data, so this module — and
//! its tests — compile on a build with no ONNX Runtime. Only the *caller* is gated.
//! Rationale in `docs/src/assistant.md` § retrieval.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::catalog::CatalogEntry;
use super::fusion::{self, RecordKey};
use super::store::{self, TypeResults};
use crate::db::Database;

/// Longest passage returned per hit.
///
/// The contract says `search` returns summaries, not bodies: a wide slice is both a
/// privacy cost and a recurring one, since everything gathered is re-sent on every
/// later turn of the loop.
const SNIPPET_CHARS: usize = 240;

/// A hit from the vector side, reduced to what merging needs.
#[derive(Debug, Clone)]
pub struct SemanticHit {
    pub key: RecordKey,
    /// The matched chunk. Better than a keyword window — it is the passage the
    /// model actually judged relevant, rather than the neighbourhood of a word.
    pub text: String,
}

/// The semantic half of retrieval, as an interface rather than a type.
///
/// A trait rather than a concrete `Embedder` so that **this signature carries no
/// `#[cfg]`**: `dispatch` and `Session` take `Option<&dyn SemanticSearch>` and
/// compile identically whether or not the host built with `embeddings`. `None` is
/// keyword-only, which is exactly the behaviour that shipped before this phase.
/// Same shape as [`crate::llm::LlmClient`], for the same reason.
#[async_trait::async_trait]
pub trait SemanticSearch: Send + Sync {
    /// Nearest records to `query`, best first, at most one entry per record.
    ///
    /// Returns a plain `Vec` rather than a `Result`: retrieval degrading to
    /// keyword-only is a worse answer, not a failed one, and a model that receives
    /// an error here spends a turn recovering from something it cannot fix.
    /// Implementations log their own failures and return empty.
    async fn search(&self, query: &str, limit: usize) -> Vec<SemanticHit>;
}

/// Merge keyword and semantic results into the shape the model sees.
///
/// The output is **one ranked list across every type**, which keyword search alone
/// could never justify: BM25 scores are relative to their own corpus, so a note at
/// 1.7 and a journal entry at 1.7 were graded on different curves. Cosine distance
/// is corpus-independent, and fusing by *rank* rather than score keeps the merged
/// order honest for both.
///
/// `keyword_matches` counts BM25 matches only, and is named for it. There is no
/// meaningful count on the semantic side — every record has *some* similarity to
/// every query — so a combined total would be a number with no referent.
pub async fn merge(
    db: &Database,
    targets: &[&'static CatalogEntry],
    keyword: Vec<TypeResults>,
    semantic: Vec<SemanticHit>,
    limit: usize,
) -> Value {
    let mut tallies: BTreeMap<String, usize> = BTreeMap::new();
    for group in &keyword {
        tallies.insert(group.record_type.clone(), group.total_matches);
    }

    let keyword_ranking: Vec<RecordKey> = keyword
        .iter()
        .flat_map(|g| {
            g.hits.iter().map(|h| RecordKey {
                record_type: g.record_type.clone(),
                id: h.id.clone(),
            })
        })
        .collect();
    let semantic_ranking: Vec<RecordKey> = semantic.iter().map(|h| h.key.clone()).collect();

    // An empty ranking is dropped rather than passed as an empty list: RRF over
    // one retriever must reproduce that retriever's order exactly, which is what
    // keeps a no-embeddings build behaving as it did before.
    let rankings: Vec<Vec<RecordKey>> = [keyword_ranking, semantic_ranking]
        .into_iter()
        .filter(|r| !r.is_empty())
        .collect();

    if rankings.is_empty() {
        return json!({
            "results": [],
            "note": "nothing matched. Try different words, or say that it is not there.",
        });
    }

    let ordered = fusion::fuse(&rankings, limit);
    let hits = describe(db, targets, &ordered, &keyword, &semantic).await;

    json!({
        "results": hits,
        "keyword_matches": tallies,
    })
}

/// Turn fused identities back into something a person could recognise.
async fn describe(
    db: &Database,
    targets: &[&'static CatalogEntry],
    ordered: &[RecordKey],
    keyword: &[TypeResults],
    semantic: &[SemanticHit],
) -> Vec<Value> {
    // Handles for anything the keyword side never saw, fetched per type in one
    // query each rather than per hit.
    let mut fetched: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    for entry in targets {
        let missing: Vec<String> = ordered
            .iter()
            .filter(|k| k.record_type == entry.name)
            .filter(|k| keyword_hit(keyword, k).is_none())
            .map(|k| k.id.clone())
            .collect();
        if missing.is_empty() {
            continue;
        }
        match store::handles_for(db, entry, &missing).await {
            Ok(map) => {
                for (id, handle) in map {
                    fetched.insert((entry.name.to_string(), id), handle);
                }
            }
            // Non-fatal: a hit with no handle is still a hit, and refusing the
            // whole answer over a naming lookup would be worse than an unnamed row.
            Err(e) => {
                tracing::warn!(record_type = entry.name, error = %e, "could not fetch handles")
            }
        }
    }

    ordered
        .iter()
        .map(|key| {
            let from_keyword = keyword_hit(keyword, key);
            let handle = from_keyword
                .map(|h| h.handle.clone())
                .or_else(|| {
                    fetched
                        .get(&(key.record_type.clone(), key.id.clone()))
                        .cloned()
                })
                .unwrap_or_default();

            // Prefer the semantic passage: it is the text that actually matched
            // the meaning of the question, where the keyword window is only the
            // neighbourhood of a term.
            let snippet = semantic
                .iter()
                .find(|h| h.key == *key)
                .map(|h| truncate(&h.text, SNIPPET_CHARS))
                .or_else(|| from_keyword.and_then(|h| h.snippet.clone()));

            json!({
                "record_type": key.record_type,
                "id": key.id,
                "handle": handle,
                "snippet": snippet,
            })
        })
        .collect()
}

fn keyword_hit<'a>(
    keyword: &'a [TypeResults],
    key: &RecordKey,
) -> Option<&'a super::store::SearchHit> {
    keyword
        .iter()
        .find(|g| g.record_type == key.record_type)
        .and_then(|g| g.hits.iter().find(|h| h.id == key.id))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::catalog::ALL_ENTRIES;
    use crate::assistant::store::SearchHit;
    use crate::events::{NotesProjection, Projection, RoutinesProjection};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::connect(dir.path().join("r.db").to_str().unwrap())
            .await
            .unwrap();
        NotesProjection.init_schema(&db).await.unwrap();
        RoutinesProjection.init_schema(&db).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn entry(name: &str) -> &'static CatalogEntry {
        ALL_ENTRIES.iter().find(|e| e.name == name).unwrap()
    }

    fn group(record_type: &str, ids: &[&str], total: usize) -> TypeResults {
        TypeResults {
            record_type: record_type.to_string(),
            hits: ids
                .iter()
                .map(|id| SearchHit {
                    record_type: record_type.to_string(),
                    id: (*id).to_string(),
                    handle: format!("handle {id}"),
                    score: 1.0,
                    snippet: Some(format!("keyword snippet {id}")),
                })
                .collect(),
            total_matches: total,
        }
    }

    #[tokio::test]
    async fn results_are_one_list_not_per_type_groups() {
        let db = test_db().await;
        let out = merge(
            &db,
            &[entry("note")],
            vec![group("note", &["a", "b"], 2)],
            vec![],
            10,
        )
        .await;

        let results = out["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["record_type"], "note");
        assert!(results[0]["handle"].is_string());
        // The old shape nested hits under a per-type group; nothing should.
        assert!(results[0]["hits"].is_null(), "still grouped: {out}");
    }

    #[tokio::test]
    async fn keyword_match_counts_are_reported_per_type() {
        let db = test_db().await;
        let out = merge(
            &db,
            &[entry("note")],
            vec![group("note", &["a"], 14)],
            vec![],
            10,
        )
        .await;

        assert_eq!(out["keyword_matches"]["note"], 14, "{out}");
    }

    /// Without embeddings there is one ranking, and it must pass through intact.
    #[tokio::test]
    async fn a_single_retriever_preserves_its_own_order() {
        let db = test_db().await;
        let out = merge(
            &db,
            &[entry("note")],
            vec![group("note", &["first", "second", "third"], 3)],
            vec![],
            10,
        )
        .await;

        let ids: Vec<&str> = out["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["first", "second", "third"], "{out}");
    }

    /// A vector-only hit has no handle of its own; it must be looked up, and its
    /// matched passage preferred over a keyword window.
    #[tokio::test]
    async fn a_semantic_only_hit_gets_its_handle_and_passage() {
        let db = test_db().await;
        db.query(
            "CREATE type::record('generic_notes', $id) SET title = 'Lease renewal',
             raw_text = 'body', tags = [], created_at = time::now(), updated_at = time::now()",
        )
        .bind(("id", "01JKRET0000000000000000001"))
        .await
        .unwrap();

        let out = merge(
            &db,
            &[entry("note")],
            vec![],
            vec![SemanticHit {
                key: RecordKey {
                    record_type: "note".to_string(),
                    id: "01JKRET0000000000000000001".to_string(),
                },
                text: "agreed to 4% rather than the proposed 8%".to_string(),
            }],
            10,
        )
        .await;

        let first = &out["results"][0];
        assert_eq!(first["handle"], "Lease renewal", "{out}");
        assert!(
            first["snippet"]
                .as_str()
                .unwrap()
                .contains("agreed to 4%"),
            "{out}"
        );
    }

    #[tokio::test]
    async fn nothing_at_all_says_so_rather_than_returning_an_empty_array() {
        let db = test_db().await;
        let out = merge(&db, &[entry("note")], vec![], vec![], 10).await;

        assert!(out["results"].as_array().unwrap().is_empty());
        assert!(out["note"].is_string(), "the model needs to be told: {out}");
    }
}
