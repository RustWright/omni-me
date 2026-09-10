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

/// The optional third pass: a cross-encoder reordering what the first two found.
///
/// Gate-free for the same reason as [`SemanticSearch`], and returns positions rather
/// than records so it never learns what a `RecordKey` is.
#[async_trait::async_trait]
pub trait Rerank: Send + Sync {
    /// Score each document against `query`, returning `(index, score)` best first.
    ///
    /// ⚠️ **An empty return means "no opinion", never "no results".** Callers keep
    /// the fused order when this is empty — a reranker that failed to load, timed
    /// out, or was handed nothing must degrade the *ordering*, never the answer.
    async fn rank(&self, query: &str, documents: &[String]) -> Vec<(usize, f32)>;
}

/// The optional halves of retrieval, as one argument.
///
/// A struct rather than two parameters threaded through `dispatch`: both are
/// `Option` because both are host capabilities rather than request options, and a
/// default-constructed value is exactly the keyword-only behaviour that shipped
/// before this phase. Adding a fourth pass later widens this and nothing else.
#[derive(Default, Clone, Copy)]
pub struct Retrievers<'a> {
    pub semantic: Option<&'a dyn SemanticSearch>,
    pub reranker: Option<&'a dyn Rerank>,
}

/// How many fused candidates a reranker gets to look at.
///
/// Wider than the final list on purpose: a cross-encoder that only sees the top few
/// can reorder them but can never rescue a right answer RRF ranked eleventh, which is
/// most of what a reranker is for. Bounded absolutely rather than by a multiple alone
/// because every candidate costs one inference on a two-core host — the pool is a
/// latency budget as much as a quality knob. `MODEL_BENCH.md` § Retrieval holds the
/// per-model cost this was set against.
const RERANK_POOL_FACTOR: usize = 3;
const RERANK_POOL_MAX: usize = 24;

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
    query: &str,
    keyword: Vec<TypeResults>,
    semantic: Vec<SemanticHit>,
    limit: usize,
    reranker: Option<&dyn Rerank>,
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

    let fused = fusion::rank(&rankings);
    let ordered = match reranker {
        Some(r) => rerank_pool(r, query, &fused, &keyword, &semantic, limit).await,
        None => fusion::cut(&fused, limit),
    };
    let hits = describe(db, targets, &ordered, &keyword, &semantic).await;

    json!({
        "results": hits,
        "keyword_matches": tallies,
    })
}

/// Reorder the head of the fused list with a cross-encoder, then cut to `limit`.
///
/// Everything the reranker never saw stays behind the part it did, in its fused
/// order. That matters when the pool is narrower than the candidate list: without
/// it, a tail candidate would vanish rather than simply rank below the judged ones.
async fn rerank_pool(
    reranker: &dyn Rerank,
    query: &str,
    fused: &[(fusion::RecordKey, f64)],
    keyword: &[TypeResults],
    semantic: &[SemanticHit],
    limit: usize,
) -> Vec<RecordKey> {
    let pool_size = (limit * RERANK_POOL_FACTOR).min(RERANK_POOL_MAX).min(fused.len());
    let pool = &fused[..pool_size];
    let documents: Vec<String> = pool
        .iter()
        .map(|(key, _)| candidate_text(key, keyword, semantic))
        .collect();

    let scored = reranker.rank(query, &documents).await;
    if scored.is_empty() {
        return fusion::cut(fused, limit);
    }

    // Rebuilt rather than sorted in place: the reranker may return fewer entries
    // than it was given, and a candidate it declined to score must keep a position
    // rather than disappear.
    //
    // ⚠️ The scores in this vector are on **two different scales** — cross-encoder
    // logits for what was judged, RRF weights for what was not. That is safe only
    // because `fusion::cut` never compares them; it takes the order as given. Do
    // not add a sort here.
    let mut reordered: Vec<(RecordKey, f64)> = Vec::with_capacity(fused.len());
    let mut placed = vec![false; pool.len()];
    for (index, score) in scored {
        let Some((key, _)) = pool.get(index) else {
            continue;
        };
        // A repeated index would otherwise emit the same record twice. Nothing in
        // the trait forbids one, and a duplicated result row is the kind of defect
        // that reads as a data problem rather than a ranking one.
        if placed[index] {
            continue;
        }
        placed[index] = true;
        reordered.push((key.clone(), f64::from(score)));
    }
    for (i, (key, rrf)) in pool.iter().enumerate() {
        if !placed[i] {
            reordered.push((key.clone(), *rrf));
        }
    }
    reordered.extend(fused[pool_size..].iter().cloned());

    fusion::cut(&reordered, limit)
}

/// The best text we hold for a candidate, for the reranker to judge.
///
/// Preference order is a quality ordering, not a convenience one: the semantic chunk
/// is a whole passage the model already found relevant, the keyword snippet is a
/// window around a term, and the handle is a title. A cross-encoder reads whatever it
/// is given as the document, so feeding it a title where a passage exists would throw
/// away most of what it is for.
fn candidate_text(key: &RecordKey, keyword: &[TypeResults], semantic: &[SemanticHit]) -> String {
    if let Some(hit) = semantic.iter().find(|h| h.key == *key) {
        return hit.text.clone();
    }
    match keyword_hit(keyword, key) {
        Some(hit) => hit
            .snippet
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| hit.handle.clone()),
        None => String::new(),
    }
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
            "a query",
            vec![group("note", &["a", "b"], 2)],
            vec![],
            10,
            None,
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
            "a query",
            vec![group("note", &["a"], 14)],
            vec![],
            10,
            None,
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
            "a query",
            vec![group("note", &["first", "second", "third"], 3)],
            vec![],
            10,
            None,
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
            "a query",
            vec![],
            vec![SemanticHit {
                key: RecordKey {
                    record_type: "note".to_string(),
                    id: "01JKRET0000000000000000001".to_string(),
                },
                text: "agreed to 4% rather than the proposed 8%".to_string(),
            }],
            10,
            None,
        )
        .await;

        let first = &out["results"][0];
        assert_eq!(first["handle"], "Lease renewal", "{out}");
        assert!(
            first["snippet"].as_str().unwrap().contains("agreed to 4%"),
            "{out}"
        );
    }

    #[tokio::test]
    async fn nothing_at_all_says_so_rather_than_returning_an_empty_array() {
        let db = test_db().await;
        let out = merge(&db, &[entry("note")], "a query", vec![], vec![], 10, None).await;

        assert!(out["results"].as_array().unwrap().is_empty());
        assert!(out["note"].is_string(), "the model needs to be told: {out}");
    }

    /// A stub cross-encoder, so the wiring is testable without ONNX Runtime.
    ///
    /// Scores by how many of `wants`'s words the document contains — enough to
    /// express "this one is better" without pretending to be a model.
    struct StubRerank {
        wants: &'static str,
    }

    #[async_trait::async_trait]
    impl Rerank for StubRerank {
        async fn rank(&self, _query: &str, documents: &[String]) -> Vec<(usize, f32)> {
            let mut scored: Vec<(usize, f32)> = documents
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let hits = self.wants.split_whitespace().filter(|w| d.contains(w)).count();
                    (i, hits as f32)
                })
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            scored
        }
    }

    /// A reranker that returns nothing at all must cost the answer nothing.
    struct SilentRerank;

    #[async_trait::async_trait]
    impl Rerank for SilentRerank {
        async fn rank(&self, _query: &str, _documents: &[String]) -> Vec<(usize, f32)> {
            Vec::new()
        }
    }

    #[tokio::test]
    async fn a_reranker_can_promote_a_candidate_the_fusion_ranked_last() {
        let db = test_db().await;
        let out = merge(
            &db,
            &[entry("note")],
            "sourdough starter",
            vec![group("note", &["a", "b", "c"], 3)],
            vec![SemanticHit {
                key: RecordKey {
                    record_type: "note".to_string(),
                    id: "c".to_string(),
                },
                text: "feeding the sourdough starter twice a day".to_string(),
            }],
            3,
            Some(&StubRerank {
                wants: "sourdough starter",
            }),
        )
        .await;

        let ids: Vec<&str> = out["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.first(), Some(&"c"), "reranker was ignored: {out}");
    }

    /// The degrade-not-fail contract, in the direction that would be silent:
    /// an empty score list must leave the fused order intact, not empty the results.
    #[tokio::test]
    async fn a_reranker_with_no_opinion_leaves_the_fused_order_alone() {
        let db = test_db().await;
        let out = merge(
            &db,
            &[entry("note")],
            "a query",
            vec![group("note", &["first", "second", "third"], 3)],
            vec![],
            10,
            Some(&SilentRerank),
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

    /// A candidate outside the rerank pool must rank below the judged ones, not
    /// fall out of the results entirely.
    #[tokio::test]
    async fn candidates_beyond_the_pool_survive_reranking() {
        let db = test_db().await;
        let ids: Vec<String> = (0..40).map(|i| format!("n{i:02}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();

        let out = merge(
            &db,
            &[entry("note")],
            "a query",
            vec![group("note", &refs, refs.len())],
            vec![],
            30,
            Some(&SilentRerank),
        )
        .await;

        assert_eq!(out["results"].as_array().unwrap().len(), 30, "{out}");
    }
}
