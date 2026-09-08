//! Reading records for the verbs: the two queries behind `search` and `read`.
//!
//! Both are built from a [`CatalogEntry`] rather than written per type, which is
//! what makes adding a type a catalog entry instead of a new query. Field names
//! are interpolated from `&'static str` constants in the catalog; only the user's
//! query text and identity are ever bound as parameters.

use serde::Serialize;
use surrealdb::types::SurrealValue;

use super::catalog::{CatalogEntry, ChildCollection};
use crate::db::{Database, DbError};

/// How much of a matched body a search result may carry.
///
/// The contract says `search` returns summaries, not bodies — both because a
/// wide retrieval slice is a privacy cost and because every gathered snippet is
/// re-sent on each subsequent turn of the loop.
const SNIPPET_CHARS: usize = 240;

/// Markers `search::highlight` wraps around matched terms.
///
/// ⚠️ **Internal only — they never reach the model.** `search::highlight` marks up
/// the *whole field* rather than returning a window, so the markers are used to
/// locate the match and are stripped by [`keyword_in_context`] before the snippet
/// is returned. A marked snippet handed over as-is gets quoted back verbatim: the
/// first live run answered "Landlord posted the «rent» «notice» today", markers
/// and all.
///
/// Guillemets rather than angle brackets because a journal entry can contain
/// HTML, and `<b>` would be indistinguishable from the entry's own text.
const HL_OPEN: &str = "\u{ab}";
const HL_CLOSE: &str = "\u{bb}";

/// One search result. Deliberately not the record — that is what `read` is for.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    /// Which catalogued type this came from.
    pub record_type: String,
    /// Identity to hand back to `read`.
    pub id: String,
    /// What a person would call it: a date, a title, a routine's name.
    pub handle: String,
    /// BM25 relevance **within this type only**. See [`search`].
    pub score: f64,
    /// Matched text with the match marked, when the type has a body distinct
    /// from its handle. `None` for a type whose only text *is* its handle.
    pub snippet: Option<String>,
}

/// Results for one record type, kept separate rather than merged.
#[derive(Debug, Clone, Serialize)]
pub struct TypeResults {
    pub record_type: String,
    pub hits: Vec<SearchHit>,
    /// How many matched before the limit was applied, so the model can tell
    /// "that is all of them" from "there are more".
    pub total_matches: usize,
}

/// A record in full, plus whatever belongs to it.
#[derive(Debug, Clone, Serialize)]
pub struct FullRecord {
    pub record_type: String,
    pub id: String,
    pub handle: String,
    /// Every column of the row, as stored.
    pub fields: serde_json::Value,
    /// Declared child collections, keyed by [`ChildCollection::name`].
    pub children: serde_json::Map<String, serde_json::Value>,
}

/// `SurrealValue`, not serde's `Deserialize`: SurrealDB v3 decodes rows through
/// its own trait, and a plain serde derive fails the `take` bound.
#[derive(Debug, SurrealValue)]
struct RawHit {
    id: Option<String>,
    handle: Option<serde_json::Value>,
    score: Option<f64>,
    snippet: Option<String>,
    body: Option<String>,
}

/// Full-text search one catalogued type.
///
/// ⚠️ **Scores are comparable within a type and not across types.** BM25 is
/// relative to its own corpus — its document count and average length — so a note
/// scoring 1.7 and a journal entry scoring 1.7 have not been measured against
/// each other. That is why results come back grouped by type rather than merged
/// into one ranked list, and why nothing here sorts across the groups.
///
/// ⚠️ **Never filter on `score > 0`.** Classic BM25's IDF term is
/// `log((N - n + 0.5)/(n + 0.5))`, which is *exactly zero* when a term appears in
/// one of two documents. On a fresh install with a handful of records a perfectly
/// good match scores 0.0, and a threshold would silently drop it.
pub async fn search(
    db: &Database,
    entry: &CatalogEntry,
    query: &str,
    limit: u32,
) -> Result<TypeResults, DbError> {
    // The match clauses are numbered so `search::score(n)` can refer back to
    // them; the numbering is local to this statement. Each text field needs its
    // own FULLTEXT index, which is why they are separate clauses rather than one.
    let where_clause = entry
        .text_fields
        .iter()
        .enumerate()
        .map(|(i, f)| format!("{f} @{i}@ $q"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let score_expr = (0..entry.text_fields.len())
        .map(|i| format!("search::score({i})"))
        .collect::<Vec<_>>()
        .join(" + ");

    // The body is the last text field — for a note that is `raw_text`, for a
    // routine it is the name, which is also the handle. `snippet_source` is what
    // gets highlighted; `body` is the unhighlighted fallback for a hit that
    // matched a different field.
    let body_idx = entry.text_fields.len() - 1;
    let body_field = entry.text_fields[body_idx];
    let has_distinct_body = body_field != entry.handle;

    let sql = format!(
        "SELECT meta::id(id) AS id,
                {handle} AS handle,
                {score_expr} AS score,
                search::highlight('{HL_OPEN}', '{HL_CLOSE}', {body_idx}) AS snippet,
                {body_field} AS body
         FROM {table}
         WHERE {where_clause}
         ORDER BY score DESC
         LIMIT {limit}",
        handle = entry.handle,
        table = entry.table,
    );

    let mut resp = db.query(&sql).bind(("q", query.to_string())).await?;
    let raw: Vec<RawHit> = resp.take(0)?;

    // A second query only to learn whether the limit hid anything. Cheaper than
    // fetching everything and counting in Rust, and the difference between "that
    // is all of them" and "there are more" changes what the model does next.
    let count_sql = format!(
        "SELECT count() AS n FROM {table} WHERE {where_clause} GROUP ALL",
        table = entry.table,
    );
    let mut count_resp = db.query(&count_sql).bind(("q", query.to_string())).await?;
    let counts: Vec<serde_json::Value> = count_resp.take(0)?;
    let total_matches = counts
        .first()
        .and_then(|v| v["n"].as_u64())
        .unwrap_or(raw.len() as u64) as usize;

    let hits = raw
        .into_iter()
        .map(|r| SearchHit {
            record_type: entry.name.to_string(),
            id: r.id.unwrap_or_default(),
            handle: r
                .handle
                .map(|h| match h {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                })
                .unwrap_or_default(),
            score: r.score.unwrap_or(0.0),
            snippet: if has_distinct_body {
                // Prefer a window around the match; fall back to the opening of
                // the body when the match was in another field, so a title hit
                // still shows what the record is about.
                match r.snippet.filter(|s| s.contains(HL_OPEN)) {
                    Some(marked) => Some(keyword_in_context(&marked, SNIPPET_CHARS)),
                    None => Some(truncate(&r.body.unwrap_or_default(), SNIPPET_CHARS)),
                }
            } else {
                None
            },
        })
        .collect();

    Ok(TypeResults {
        record_type: entry.name.to_string(),
        hits,
        total_matches,
    })
}

/// Fetch one record in full, with its declared children.
pub async fn read(
    db: &Database,
    entry: &CatalogEntry,
    id: &str,
) -> Result<Option<FullRecord>, DbError> {
    let sql = format!(
        "SELECT *, meta::id(id) AS id FROM type::record('{table}', $id)",
        table = entry.table,
    );
    let mut resp = db.query(&sql).bind(("id", id.to_string())).await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let Some(fields) = rows.into_iter().next() else {
        return Ok(None);
    };

    let handle = fields
        .get(entry.handle)
        .map(|h| match h {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| id.to_string());

    let mut children = serde_json::Map::new();
    for child in entry.children {
        children.insert(
            child.name.to_string(),
            serde_json::Value::Array(fetch_children(db, child, id).await?),
        );
    }

    Ok(Some(FullRecord {
        record_type: entry.name.to_string(),
        id: id.to_string(),
        handle,
        fields,
        children,
    }))
}

/// One child collection, newest first and capped by its declaration.
///
/// The cap lives in the catalog rather than here because it is a property of the
/// data — a routine has a handful of items and an unbounded completion history —
/// and an uncapped history would quietly become the largest thing in a prompt.
async fn fetch_children(
    db: &Database,
    child: &ChildCollection,
    parent_id: &str,
) -> Result<Vec<serde_json::Value>, DbError> {
    let sql = format!(
        "SELECT *, meta::id(id) AS id FROM {table}
         WHERE {fk} = $parent
         ORDER BY id DESC
         LIMIT {limit}",
        table = child.table,
        fk = child.foreign_key,
        limit = child.limit,
    );
    let mut resp = db.query(&sql).bind(("parent", parent_id.to_string())).await?;
    Ok(resp.take(0)?)
}

/// A window of text around the first match, with the markers removed.
///
/// Two jobs at once, and both are needed. `search::highlight` marks up the whole
/// field, so a long journal entry cut from the start can easily not contain the
/// match at all — the window is what makes the snippet actually show why the
/// record matched. And the markers themselves must not survive, because a model
/// handed them quotes them straight back into its answer.
fn keyword_in_context(marked: &str, budget: usize) -> String {
    let chars: Vec<char> = marked.chars().collect();
    let open: Vec<char> = HL_OPEN.chars().collect();
    let at = chars
        .windows(open.len())
        .position(|w| w == open.as_slice())
        .unwrap_or(0);

    // Roughly a third of the budget before the match, so there is enough lead-in
    // to read it as a sentence rather than starting mid-word.
    let lead = budget / 3;
    let start = at.saturating_sub(lead);
    let end = (start + budget).min(chars.len());

    let window: String = chars[start..end]
        .iter()
        .collect::<String>()
        .replace(HL_OPEN, "")
        .replace(HL_CLOSE, "");
    let window = window.trim();

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(window);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Cut to a character budget on a char boundary, marking that it was cut.
fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
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
    use crate::events::{NotesProjection, Projection, RoutinesProjection};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        NotesProjection.init_schema(&db).await.unwrap();
        RoutinesProjection.init_schema(&db).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn entry(name: &str) -> &'static CatalogEntry {
        ALL_ENTRIES.iter().find(|e| e.name == name).unwrap()
    }

    /// Enough rows that BM25's IDF is not degenerate — see the warning on
    /// [`search`]. With two documents a real match scores exactly 0.0.
    async fn seed_notes(db: &Database) {
        for i in 0..9 {
            db.query("CREATE type::record('generic_notes', $id) SET title = $t, raw_text = $b, tags = [], created_at = time::now(), updated_at = time::now()")
                .bind(("id", format!("01JKFILLER{i:022}")))
                .bind(("t", format!("filler {i}")))
                .bind(("b", format!("cycling and soup, entry {i}")))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn search_finds_a_note_by_body_and_marks_the_match() {
        let db = test_db().await;
        seed_notes(&db).await;
        db.query("CREATE type::record('generic_notes', $id) SET title = 'Rent notice', raw_text = 'The landlord posted a notice about the rent increase.', tags = [], created_at = time::now(), updated_at = time::now()")
            .bind(("id", "01JKNOTE00000000000000000A"))
            .await
            .unwrap();

        let out = search(&db, entry("note"), "rent", 20).await.unwrap();
        assert_eq!(out.hits.len(), 1, "{out:?}");
        assert_eq!(out.total_matches, 1);
        assert_eq!(out.hits[0].handle, "Rent notice");
        assert_eq!(out.hits[0].id, "01JKNOTE00000000000000000A");

        let snippet = out.hits[0].snippet.as_deref().unwrap();
        assert!(snippet.contains("rent"), "snippet missed the match: {snippet}");
        // ⚠️ Regression: the first live run answered "Landlord posted the «rent»
        // «notice» today" — the model quoted the markers straight back. They are
        // internal and must never reach it.
        assert!(
            !snippet.contains(HL_OPEN) && !snippet.contains(HL_CLOSE),
            "highlight markers leaked into the snippet: {snippet}"
        );
    }

    /// The window is what makes a snippet useful on a long record: cutting from
    /// the start would show an opening that never mentions the match.
    #[tokio::test]
    async fn a_match_late_in_a_long_body_still_appears_in_the_snippet() {
        let db = test_db().await;
        seed_notes(&db).await;
        let filler = "padding sentence about nothing in particular. ".repeat(40);
        db.query("CREATE type::record('generic_notes', $id) SET title = 'Long', raw_text = $b, tags = [], created_at = time::now(), updated_at = time::now()")
            .bind(("id", "01JKNOTE00000000000000000C"))
            .bind(("b", format!("{filler} and finally the sourdough starter doubled overnight.")))
            .await
            .unwrap();

        let out = search(&db, entry("note"), "sourdough", 20).await.unwrap();
        let snippet = out.hits[0].snippet.as_deref().unwrap();
        assert!(
            snippet.contains("sourdough"),
            "the window must centre on the match, got: {snippet}"
        );
        assert!(snippet.starts_with('…'), "a mid-body window says so: {snippet}");
        assert!(
            snippet.chars().count() <= SNIPPET_CHARS + 2,
            "budget exceeded: {} chars",
            snippet.chars().count()
        );
    }

    /// A title hit has no highlight in the body, and must still come back with
    /// something that says what the note is about.
    #[tokio::test]
    async fn a_title_match_falls_back_to_the_plain_body() {
        let db = test_db().await;
        seed_notes(&db).await;
        db.query("CREATE type::record('generic_notes', $id) SET title = 'Dentist', raw_text = 'body mentions nothing relevant', tags = [], created_at = time::now(), updated_at = time::now()")
            .bind(("id", "01JKNOTE00000000000000000B"))
            .await
            .unwrap();

        let out = search(&db, entry("note"), "dentist", 20).await.unwrap();
        assert_eq!(out.hits.len(), 1, "{out:?}");
        assert_eq!(
            out.hits[0].snippet.as_deref(),
            Some("body mentions nothing relevant")
        );
    }

    #[tokio::test]
    async fn total_matches_reports_what_the_limit_hid() {
        let db = test_db().await;
        seed_notes(&db).await;
        let out = search(&db, entry("note"), "cycling", 3).await.unwrap();
        assert_eq!(out.hits.len(), 3);
        assert_eq!(out.total_matches, 9, "the limit must not hide the count");
    }

    /// The zero-score case, pinned deliberately: it is the shape that would make
    /// a `score > 0` filter silently drop real matches.
    #[tokio::test]
    async fn a_legitimate_match_can_score_zero_in_a_tiny_corpus() {
        let db = test_db().await;
        db.query("CREATE type::record('generic_notes', $id) SET title = 'a', raw_text = 'unique term here', tags = [], created_at = time::now(), updated_at = time::now()")
            .bind(("id", "01JKTINY000000000000000001"))
            .await
            .unwrap();
        db.query("CREATE type::record('generic_notes', $id) SET title = 'b', raw_text = 'nothing alike', tags = [], created_at = time::now(), updated_at = time::now()")
            .bind(("id", "01JKTINY000000000000000002"))
            .await
            .unwrap();

        let out = search(&db, entry("note"), "unique", 20).await.unwrap();
        assert_eq!(out.hits.len(), 1, "a zero-scoring match is still a match");
        assert_eq!(out.hits[0].score, 0.0, "if this changes, drop the warning");
    }

    #[tokio::test]
    async fn read_returns_a_routine_with_its_items() {
        let db = test_db().await;
        db.query("CREATE type::record('routine_groups', $id) SET name = 'Morning', frequency = 'daily', order_num = 0, removed = false, created_at = time::now(), updated_at = time::now()")
            .bind(("id", "01JKGROUP0000000000000001"))
            .await
            .unwrap();
        for (i, name) in ["stretch", "coffee"].iter().enumerate() {
            db.query("CREATE type::record('routine_items', $id) SET group_id = $g, name = $n, estimated_duration_min = 5, order_num = $o, removed = false")
                .bind(("id", format!("01JKITEM{i:018}")))
                .bind(("g", "01JKGROUP0000000000000001"))
                .bind(("n", name.to_string()))
                .bind(("o", i as i64))
                .await
                .unwrap();
        }

        let got = read(&db, entry("routine"), "01JKGROUP0000000000000001")
            .await
            .unwrap()
            .expect("routine should exist");
        assert_eq!(got.handle, "Morning");
        let items = got.children["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "{items:?}");
        assert!(got.children.contains_key("completions"));
    }

    #[tokio::test]
    async fn read_of_a_missing_record_is_none_not_an_error() {
        let db = test_db().await;
        assert!(
            read(&db, entry("note"), "01JKNOPE0000000000000000")
                .await
                .unwrap()
                .is_none()
        );
    }

    /// A routine's only text field *is* its handle, so a snippet would repeat it.
    #[tokio::test]
    async fn a_type_whose_text_is_its_handle_has_no_snippet() {
        let db = test_db().await;
        for i in 0..9 {
            db.query("CREATE type::record('routine_groups', $id) SET name = $n, frequency = 'daily', order_num = 0, removed = false, created_at = time::now(), updated_at = time::now()")
                .bind(("id", format!("01JKFILLG{i:017}")))
                .bind(("n", format!("filler routine {i}")))
                .await
                .unwrap();
        }
        db.query("CREATE type::record('routine_groups', $id) SET name = 'Evening winddown', frequency = 'daily', order_num = 0, removed = false, created_at = time::now(), updated_at = time::now()")
            .bind(("id", "01JKGROUP0000000000000009"))
            .await
            .unwrap();

        let out = search(&db, entry("routine"), "winddown", 20).await.unwrap();
        assert_eq!(out.hits.len(), 1, "{out:?}");
        assert_eq!(out.hits[0].handle, "Evening winddown");
        assert!(out.hits[0].snippet.is_none());
    }

    #[test]
    fn keyword_in_context_centres_on_the_match_and_strips_markers() {
        let marked = format!("{}one two {HL_OPEN}three{HL_CLOSE} four five", "lead ".repeat(40));
        let out = keyword_in_context(&marked, 60);
        assert!(out.contains("three"), "{out}");
        assert!(!out.contains(HL_OPEN) && !out.contains(HL_CLOSE), "{out}");
        assert!(out.starts_with('…'), "{out}");
        assert!(out.chars().count() <= 62, "{} chars", out.chars().count());
    }

    /// A match at the very start needs no leading ellipsis.
    #[test]
    fn keyword_in_context_at_the_start_has_no_leading_ellipsis() {
        let out = keyword_in_context(&format!("{HL_OPEN}rent{HL_CLOSE} notice posted"), 60);
        assert_eq!(out, "rent notice posted");
    }

    #[test]
    fn truncate_marks_that_it_cut_and_respects_char_boundaries() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 5), "abcde…");
        // Multi-byte: naive byte slicing would panic here.
        assert_eq!(truncate("ααααα", 3), "ααα…");
    }
}
