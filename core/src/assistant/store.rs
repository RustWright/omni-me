//! Reading records for the verbs: the two queries behind `search` and `read`.
//!
//! Both are built from a [`CatalogEntry`] rather than written per type, which is
//! what makes adding a type a catalog entry instead of a new query. Field names
//! are interpolated from `&'static str` constants in the catalog; only the user's
//! query text and identity are ever bound as parameters.

use serde::Serialize;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use super::catalog::{CatalogEntry, ChildCollection, DerivedView, FilterKind};
use crate::db::{Database, DbError};
use crate::routines::CompletionRecord;

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
    pub children: serde_json::Map<String, Value>,
    /// A computed answer the raw rows technically contain but do not make
    /// usable. See [`DerivedView`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived: Option<Value>,
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

/// A listed row. Distinct from [`RawHit`] because a listing has no relevance —
/// nothing was matched — and selecting `0 AS score` to reuse the search row
/// instead fails to decode, which surfaces as "could not be listed".
#[derive(Debug, SurrealValue)]
struct ListRow {
    id: Option<String>,
    handle: Option<Value>,
    body: Option<String>,
}

/// One narrowing clause, already checked against the catalog.
///
/// Built by [`plan_filters`] rather than constructed at the call site, so a key
/// the type does not declare cannot reach the SQL.
#[derive(Debug, Clone)]
pub struct Narrowing {
    column: &'static str,
    op: NarrowOp,
    value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NarrowOp {
    Gte,
    Lte,
    Eq,
    Contains,
}

/// Turn the model's `filters` object into clauses, or say what went wrong.
///
/// Errors name the valid keys for *this* type. That matters more than it looks:
/// the model can only learn the keys from `describe_type`, and if it guessed
/// instead, a bare "unknown filter" costs it a whole turn to recover from while
/// a listed alternative costs none.
pub fn plan_filters(
    entry: &CatalogEntry,
    filters: &Value,
) -> Result<Vec<Narrowing>, String> {
    let Some(object) = filters.as_object() else {
        return Err("`filters` must be an object".to_string());
    };
    let mut out = Vec::new();
    for (key, value) in object {
        let Some(field) = entry.filters.iter().find(|f| f.key == key) else {
            return Err(format!(
                "`{}` has no filter called `{key}`. It has: {}",
                entry.name,
                entry
                    .filters
                    .iter()
                    .map(|f| f.key)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        };
        match field.kind {
            FilterKind::Range => {
                // `{from, to}`, either half optional — "everything since March"
                // is a real request and needs no upper bound.
                let bounds = value.as_object().ok_or_else(|| {
                    format!("`{key}` is a range; pass {{\"from\": …, \"to\": …}}")
                })?;
                if let Some(from) = bounds.get("from").filter(|v| !v.is_null()) {
                    out.push(Narrowing {
                        column: field.key,
                        op: NarrowOp::Gte,
                        value: from.clone(),
                    });
                }
                if let Some(to) = bounds.get("to").filter(|v| !v.is_null()) {
                    out.push(Narrowing {
                        column: field.key,
                        op: NarrowOp::Lte,
                        value: to.clone(),
                    });
                }
                if bounds.is_empty() {
                    return Err(format!("`{key}` needs a `from`, a `to`, or both"));
                }
            }
            FilterKind::Exact | FilterKind::Flag => out.push(Narrowing {
                column: field.key,
                op: NarrowOp::Eq,
                value: value.clone(),
            }),
            FilterKind::Tag => out.push(Narrowing {
                column: field.key,
                op: NarrowOp::Contains,
                value: value.clone(),
            }),
        }
    }
    Ok(out)
}

/// Enumerate records of one kind, newest or user-ordered first.
///
/// The gap `search` structurally cannot fill: it matches text, and "what routines
/// do I have" has no text to match — the word "routine" appears nowhere in a
/// routine called "Morning". Observed live burning a whole turn budget before
/// this existed.
pub async fn list(
    db: &Database,
    entry: &CatalogEntry,
    narrowings: &[Narrowing],
    limit: u32,
) -> Result<TypeResults, DbError> {
    // Column names come from the catalog (compile-time constants); only values
    // are bound, so the model cannot reach the SQL through a filter key.
    let mut wheres = Vec::new();
    let mut binds: Vec<(String, Value)> = Vec::new();
    for (i, n) in narrowings.iter().enumerate() {
        let param = format!("f{i}");
        wheres.push(match n.op {
            NarrowOp::Gte => format!("{} >= ${param}", n.column),
            NarrowOp::Lte => format!("{} <= ${param}", n.column),
            NarrowOp::Eq => format!("{} = ${param}", n.column),
            NarrowOp::Contains => format!("{} CONTAINS ${param}", n.column),
        });
        binds.push((param, n.value.clone()));
    }
    let where_clause = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };

    let order = entry.list_order;
    let direction = if order.descending { "DESC" } else { "ASC" };
    let body_field = entry.text_fields[entry.text_fields.len() - 1];
    // ⚠️ The ordering column is selected as `order_key` and ordered by that alias.
    // SurrealDB requires an `ORDER BY` idiom to appear in the statement's
    // selection — "Missing order idiom `order_num` in statement selection" — which
    // is not how ordinary SQL behaves. Ordering by the bare column name only works
    // by accident when it is already selected for something else, as journal's
    // `date` is for its handle.
    let sql = format!(
        "SELECT meta::id(id) AS id, {handle} AS handle, {body_field} AS body,
                {order_col} AS order_key
         FROM {table} {where_clause}
         ORDER BY order_key {direction}
         LIMIT {limit}",
        handle = entry.handle,
        table = entry.table,
        order_col = order.column,
    );

    let mut q = db.query(&sql);
    for (param, value) in &binds {
        q = q.bind((param.clone(), value.clone()));
    }
    let raw: Vec<ListRow> = q.await?.take(0)?;

    let count_sql = format!(
        "SELECT count() AS n FROM {table} {where_clause} GROUP ALL",
        table = entry.table,
    );
    let mut cq = db.query(&count_sql);
    for (param, value) in &binds {
        cq = cq.bind((param.clone(), value.clone()));
    }
    let counts: Vec<Value> = cq.await?.take(0)?;
    let total_matches = counts
        .first()
        .and_then(|v| v["n"].as_u64())
        .unwrap_or(raw.len() as u64) as usize;

    let has_distinct_body = body_field != entry.handle;
    let hits = raw
        .into_iter()
        .map(|r| SearchHit {
            record_type: entry.name.to_string(),
            id: r.id.unwrap_or_default(),
            handle: r
                .handle
                .map(|h| match h {
                    Value::String(s) => s,
                    other => other.to_string(),
                })
                .unwrap_or_default(),
            // No relevance in a listing: nothing was matched, so a score would be
            // a number with no meaning rather than a weak signal.
            score: 0.0,
            snippet: has_distinct_body
                .then(|| truncate(&r.body.unwrap_or_default(), SNIPPET_CHARS)),
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
            Value::Array(fetch_children(db, child, id).await?),
        );
    }

    let derived = entry.derived.map(|view| derive(view, &children));

    Ok(Some(FullRecord {
        record_type: entry.name.to_string(),
        id: id.to_string(),
        handle,
        fields,
        children,
        derived,
    }))
}

/// Compute a declared [`DerivedView`] from the child rows already fetched.
///
/// Reads the children rather than querying again: they are the same rows, and a
/// second query could disagree with what was returned beside it — a rollup that
/// does not match its own raw rows is worse than no rollup.
fn derive(view: DerivedView, children: &serde_json::Map<String, Value>) -> Value {
    match view {
        DerivedView::CompletionRollup { items, completions } => {
            let empty = Vec::new();
            let item_rows = children.get(items).and_then(|v| v.as_array()).unwrap_or(&empty);
            let completion_rows = children
                .get(completions)
                .and_then(|v| v.as_array())
                .unwrap_or(&empty);

            // Current items only — `fetch_children` already excludes nothing, so
            // filter here the way the routines screen does, or a deleted item
            // would keep a day permanently incomplete.
            let item_ids: Vec<&str> = item_rows
                .iter()
                .filter(|i| !i["removed"].as_bool().unwrap_or(false))
                .filter_map(|i| i["id"].as_str())
                .collect();
            let records: Vec<CompletionRecord<'_>> = completion_rows
                .iter()
                .filter_map(|c| {
                    Some(CompletionRecord {
                        item_id: c["item_id"].as_str()?,
                        date: c["date"].as_str()?,
                        skipped: c["skipped"].as_bool().unwrap_or(false),
                    })
                })
                .collect();

            let days = crate::routines::roll_up(&item_ids, &records);
            json!({
                "days": days,
                // Said out loud, because the cap is in rows while every question
                // about a habit is in days: 60 rows over a three-item routine is
                // about twenty days, not sixty.
                "covers_days": days.len(),
                "note": "A day is complete when every current item has a completion \
                         row. A deliberate skip counts as done; `skipped` says how \
                         many of that day's items were skipped.",
            })
        }
    }
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
         ORDER BY {order_col} {direction}
         LIMIT {limit}",
        table = child.table,
        fk = child.foreign_key,
        order_col = child.order.column,
        direction = if child.order.descending { "DESC" } else { "ASC" },
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

    /// ⚠️ Regression. Completion ids are `{item_id}-{date}-{done|skip}`, so
    /// `ORDER BY id` sorts by *item* and only then by date. The older item's
    /// oldest completion sorted above the newer item's newest one, while the
    /// declaration promised "newest first" — wrong in a way nothing would report.
    #[tokio::test]
    async fn completion_history_comes_back_newest_first_across_items() {
        let db = test_db().await;
        let group = "01JKGROUP0000000000000002";
        db.query("CREATE type::record('routine_groups', $id) SET name = 'Morning', frequency = 'daily', order_num = 0, removed = false, created_at = time::now(), updated_at = time::now()")
            .bind(("id", group))
            .await
            .unwrap();

        // Item A is old and was done recently; item B is newer and was done long
        // ago. Sorting by id puts B's stale row first; sorting by date puts A's
        // recent one first, which is what "recent history" has to mean.
        for (item, date) in [("aaa-item", "2026-03-20"), ("zzz-item", "2026-03-01")] {
            db.query("CREATE type::record('routine_completions', $id) SET item_id = $i, group_id = $g, date = $d, completed_at = time::now(), skipped = false, reason = NONE")
                .bind(("id", format!("{item}-{date}-done")))
                .bind(("i", item))
                .bind(("g", group))
                .bind(("d", date))
                .await
                .unwrap();
        }

        let got = read(&db, entry("routine"), group).await.unwrap().unwrap();
        let completions = got.children["completions"].as_array().unwrap();
        let dates: Vec<&str> = completions
            .iter()
            .filter_map(|c| c["date"].as_str())
            .collect();
        assert_eq!(dates, vec!["2026-03-20", "2026-03-01"], "{completions:?}");
    }

    /// Items come back in the order the user arranged them, not by id.
    #[tokio::test]
    async fn routine_items_follow_the_users_arrangement() {
        let db = test_db().await;
        let group = "01JKGROUP0000000000000003";
        db.query("CREATE type::record('routine_groups', $id) SET name = 'Morning', frequency = 'daily', order_num = 0, removed = false, created_at = time::now(), updated_at = time::now()")
            .bind(("id", group))
            .await
            .unwrap();
        // Ids deliberately sort opposite to the arranged order.
        for (id, name, order) in [("zzz", "stretch", 0), ("aaa", "coffee", 1)] {
            db.query("CREATE type::record('routine_items', $id) SET group_id = $g, name = $n, estimated_duration_min = 5, order_num = $o, removed = false")
                .bind(("id", id))
                .bind(("g", group))
                .bind(("n", name))
                .bind(("o", order as i64))
                .await
                .unwrap();
        }

        let got = read(&db, entry("routine"), group).await.unwrap().unwrap();
        let names: Vec<&str> = got.children["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| i["name"].as_str())
            .collect();
        assert_eq!(names, vec!["stretch", "coffee"], "order_num, not id");
    }

    /// A kind whose handle and body are the same column — the shape that has no
    /// separate snippet, and the one `list` is easiest to get wrong on.
    #[tokio::test]
    async fn list_returns_routines_in_the_users_order() {
        let db = test_db().await;
        for (i, name) in ["Morning", "Evening winddown"].iter().enumerate() {
            db.query("CREATE type::record('routine_groups', $id) SET name = $n, frequency = 'daily', order_num = $o, removed = false, created_at = time::now(), updated_at = time::now()")
                .bind(("id", format!("01JKORD{i:019}")))
                .bind(("n", name.to_string()))
                .bind(("o", i as i64))
                .await
                .unwrap();
        }

        let out = list(&db, entry("routine"), &[], 20)
            .await
            .expect("list must not error");
        let names: Vec<&str> = out.hits.iter().map(|h| h.handle.as_str()).collect();
        assert_eq!(names, vec!["Morning", "Evening winddown"]);
        assert_eq!(out.total_matches, 2);
        assert!(out.hits[0].snippet.is_none(), "handle is the only text");
    }

    /// `read` carries the app's own answer to "did I do my routine", not just the
    /// raw rows the model would otherwise have to re-derive it from — and could
    /// re-derive differently from what the routines screen shows.
    #[tokio::test]
    async fn read_returns_a_completion_rollup_matching_the_screens_rule() {
        let db = test_db().await;
        let group = "01JKGROUP0000000000000004";
        db.query("CREATE type::record('routine_groups', $id) SET name = 'Morning', frequency = 'daily', order_num = 0, removed = false, created_at = time::now(), updated_at = time::now()")
            .bind(("id", group))
            .await
            .unwrap();
        for (id, name, order) in [("it-a", "stretch", 0), ("it-b", "coffee", 1)] {
            db.query("CREATE type::record('routine_items', $id) SET group_id = $g, name = $n, estimated_duration_min = 5, order_num = $o, removed = false")
                .bind(("id", id))
                .bind(("g", group))
                .bind(("n", name))
                .bind(("o", order as i64))
                .await
                .unwrap();
        }
        // The 14th: one done, one skipped -> complete, because a deliberate skip
        // is not a miss. The 15th: only one item -> incomplete.
        for (item, date, skipped) in [
            ("it-a", "2026-03-14", false),
            ("it-b", "2026-03-14", true),
            ("it-a", "2026-03-15", false),
        ] {
            db.query("CREATE type::record('routine_completions', $id) SET item_id = $i, group_id = $g, date = $d, completed_at = time::now(), skipped = $s, reason = NONE")
                .bind(("id", format!("{item}-{date}-x")))
                .bind(("i", item))
                .bind(("g", group))
                .bind(("d", date))
                .bind(("s", skipped))
                .await
                .unwrap();
        }

        let got = read(&db, entry("routine"), group).await.unwrap().unwrap();
        let days = got.derived.as_ref().expect("routine has a rollup")["days"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(days.len(), 2, "{days:?}");

        // Newest first.
        assert_eq!(days[0]["date"], "2026-03-15");
        assert_eq!(days[0]["complete"], false);

        assert_eq!(days[1]["date"], "2026-03-14");
        assert_eq!(days[1]["complete"], true, "a skip counts as done");
        assert_eq!(days[1]["skipped"], 1, "but stays visible as a skip");
    }

    /// Kinds with nothing to derive say nothing rather than an empty object.
    #[tokio::test]
    async fn a_kind_with_no_derived_view_returns_none() {
        let db = test_db().await;
        db.query("CREATE type::record('generic_notes', $id) SET title = 'x', raw_text = 'y', tags = [], created_at = time::now(), updated_at = time::now()")
            .bind(("id", "01JKNOTE00000000000000000D"))
            .await
            .unwrap();
        let got = read(&db, entry("note"), "01JKNOTE00000000000000000D")
            .await
            .unwrap()
            .unwrap();
        assert!(got.derived.is_none());
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
