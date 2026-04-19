use serde::Serialize;
use surrealdb::types::{SurrealValue, Value as DbValue};

use super::{Database, DbError};

/// A journal entry (one per day) from the `journal_entries` projection table.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct JournalEntryRow {
    /// SurrealDB record id — equal to `date`, e.g. "2026-04-19".
    pub id: String,
    pub journal_id: String,
    pub date: String,
    pub raw_text: String,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub closed: bool,
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_properties: Option<DbValue>,
    pub created_at: String,
    pub updated_at: String,
}

/// A free-form note from the `generic_notes` projection table.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct GenericNoteRow {
    pub id: String,
    pub title: String,
    pub raw_text: String,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_properties: Option<DbValue>,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine group. `removed` rows are included in sync history but filtered
/// out of the default list view.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct RoutineGroupRow {
    pub id: String,
    pub name: String,
    pub frequency: String,
    pub order_num: i64,
    pub removed: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine item.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct RoutineItemRow {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub estimated_duration_min: i64,
    pub order_num: i64,
    pub removed: bool,
}

/// A routine completion (complete or skip).
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct CompletionRow {
    pub id: String,
    pub item_id: String,
    pub group_id: String,
    pub date: String,
    pub completed_at: String,
    pub skipped: bool,
    pub reason: Option<String>,
}

// --- Journal entries ---

pub async fn get_journal_by_date(
    db: &Database,
    date: &str,
) -> Result<Option<JournalEntryRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, journal_id, date, raw_text, tags, summary,
                    closed, complete, legacy_properties,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM type::record('journal_entries', $date)",
        )
        .bind(("date", date.to_string()))
        .await?;

    let rows: Vec<JournalEntryRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

pub async fn get_journal_by_id(
    db: &Database,
    journal_id: &str,
) -> Result<Option<JournalEntryRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, journal_id, date, raw_text, tags, summary,
                    closed, complete, legacy_properties,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM journal_entries WHERE journal_id = $journal_id LIMIT 1",
        )
        .bind(("journal_id", journal_id.to_string()))
        .await?;

    let rows: Vec<JournalEntryRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

pub async fn list_journal_entries(
    db: &Database,
    limit: u32,
    offset: u32,
) -> Result<Vec<JournalEntryRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, journal_id, date, raw_text, tags, summary,
                    closed, complete, legacy_properties,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM journal_entries
             ORDER BY date DESC
             LIMIT $limit START $offset",
        )
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await?;

    let rows: Vec<JournalEntryRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn list_journal_dates(
    db: &Database,
    from_date: &str,
    to_date: &str,
) -> Result<Vec<String>, DbError> {
    let mut resp = db
        .query(
            "SELECT date FROM journal_entries
             WHERE date >= $from_date AND date <= $to_date
             ORDER BY date ASC",
        )
        .bind(("from_date", from_date.to_string()))
        .bind(("to_date", to_date.to_string()))
        .await?;

    #[derive(SurrealValue)]
    struct DateOnly {
        date: String,
    }
    let rows: Vec<DateOnly> = resp.take(0)?;
    Ok(rows.into_iter().map(|r| r.date).collect())
}

// --- Generic notes ---

pub async fn get_generic_note(
    db: &Database,
    id: &str,
) -> Result<Option<GenericNoteRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, title, raw_text, tags, summary, legacy_properties,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM type::record('generic_notes', $id)",
        )
        .bind(("id", id.to_string()))
        .await?;

    let rows: Vec<GenericNoteRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

pub async fn list_generic_notes(
    db: &Database,
    limit: u32,
    offset: u32,
) -> Result<Vec<GenericNoteRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, title, raw_text, tags, summary, legacy_properties,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM generic_notes
             ORDER BY updated_at DESC
             LIMIT $limit START $offset",
        )
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await?;

    let rows: Vec<GenericNoteRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn search_generic_notes(
    db: &Database,
    query: &str,
) -> Result<Vec<GenericNoteRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, title, raw_text, tags, summary, legacy_properties,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM generic_notes
             WHERE string::lowercase(raw_text) CONTAINS string::lowercase($query)
                OR string::lowercase(title) CONTAINS string::lowercase($query)
                OR tags CONTAINS $query
             ORDER BY updated_at DESC
             LIMIT 50",
        )
        .bind(("query", query.to_string()))
        .await?;

    let rows: Vec<GenericNoteRow> = resp.take(0)?;
    Ok(rows)
}

// --- Routines ---

/// List active (non-removed) routine groups, ordered by user-defined order.
pub async fn list_routine_groups(db: &Database) -> Result<Vec<RoutineGroupRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, name, frequency, order_num, removed,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM routine_groups
             WHERE removed = false
             ORDER BY order_num ASC, created_at ASC",
        )
        .await?;

    let rows: Vec<RoutineGroupRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn get_routine_group(
    db: &Database,
    id: &str,
) -> Result<Option<RoutineGroupRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, name, frequency, order_num, removed,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM type::record('routine_groups', $id)",
        )
        .bind(("id", id.to_string()))
        .await?;

    let rows: Vec<RoutineGroupRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

pub async fn list_routine_items(
    db: &Database,
    group_id: &str,
) -> Result<Vec<RoutineItemRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, group_id, name, estimated_duration_min, order_num, removed
             FROM routine_items
             WHERE group_id = $group_id AND removed = false
             ORDER BY order_num ASC",
        )
        .bind(("group_id", group_id.to_string()))
        .await?;

    let rows: Vec<RoutineItemRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn get_completions_for_date(
    db: &Database,
    group_id: &str,
    date: &str,
) -> Result<Vec<CompletionRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, item_id, group_id, date,
                    <string> completed_at AS completed_at, skipped, reason
             FROM routine_completions
             WHERE group_id = $group_id AND date = $date
             ORDER BY completed_at ASC",
        )
        .bind(("group_id", group_id.to_string()))
        .bind(("date", date.to_string()))
        .await?;

    let rows: Vec<CompletionRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn get_completion_history(
    db: &Database,
    group_id: &str,
    days: u32,
) -> Result<Vec<CompletionRow>, DbError> {
    let cutoff = chrono::Utc::now()
        .date_naive()
        .checked_sub_days(chrono::Days::new(days as u64))
        .unwrap_or(chrono::Utc::now().date_naive())
        .format("%Y-%m-%d")
        .to_string();

    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, item_id, group_id, date,
                    <string> completed_at AS completed_at, skipped, reason
             FROM routine_completions
             WHERE group_id = $group_id AND date >= $cutoff
             ORDER BY date ASC, completed_at ASC",
        )
        .bind(("group_id", group_id.to_string()))
        .bind(("cutoff", cutoff))
        .await?;

    let rows: Vec<CompletionRow> = resp.take(0)?;
    Ok(rows)
}

/// Find journal entries that are complete but not yet closed — used by the
/// auto-close tick to identify candidates.
pub async fn list_completable_unclosed_journals(
    db: &Database,
    up_to_date: &str,
) -> Result<Vec<JournalEntryRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, journal_id, date, raw_text, tags, summary,
                    closed, complete, legacy_properties,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM journal_entries
             WHERE complete = true AND closed = false AND date <= $up_to_date",
        )
        .bind(("up_to_date", up_to_date.to_string()))
        .await?;

    let rows: Vec<JournalEntryRow> = resp.take(0)?;
    Ok(rows)
}
