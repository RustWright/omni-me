use serde::Serialize;
use surrealdb::types::SurrealValue;

use super::{Database, DbError};

/// A note from the `notes` projection table.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct NoteRow {
    pub id: String,
    pub raw_text: String,
    pub date: String,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub mood: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine group from the `routine_groups` projection table.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct RoutineGroupRow {
    pub id: String,
    pub name: String,
    pub frequency: String,
    pub time_of_day: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine item from the `routine_items` projection table.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct RoutineItemRow {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub estimated_duration_min: i64,
    pub order_num: i64,
}

/// A routine completion record.
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

/// List notes ordered by date DESC.
pub async fn list_notes(db: &Database, limit: u32, offset: u32) -> Result<Vec<NoteRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, raw_text, date, tags, summary, mood,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM notes
             ORDER BY date DESC, created_at DESC
             LIMIT $limit START $offset",
        )
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await?;

    let rows: Vec<NoteRow> = resp.take(0)?;
    Ok(rows)
}

/// Get a single note by ID.
pub async fn get_note(db: &Database, id: &str) -> Result<Option<NoteRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, raw_text, date, tags, summary, mood,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM type::record('notes', $id)",
        )
        .bind(("id", id.to_string()))
        .await?;

    let rows: Vec<NoteRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

/// Search notes by raw_text or tags containing the query string.
pub async fn search_notes(db: &Database, query: &str) -> Result<Vec<NoteRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, raw_text, date, tags, summary, mood,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM notes
             WHERE string::lowercase(raw_text) CONTAINS string::lowercase($query)
                OR tags CONTAINS $query
             ORDER BY date DESC, created_at DESC
             LIMIT 50",
        )
        .bind(("query", query.to_string()))
        .await?;

    let rows: Vec<NoteRow> = resp.take(0)?;
    Ok(rows)
}

/// List all routine groups.
pub async fn list_routine_groups(db: &Database) -> Result<Vec<RoutineGroupRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, name, frequency, time_of_day,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM routine_groups
             ORDER BY created_at ASC",
        )
        .await?;

    let rows: Vec<RoutineGroupRow> = resp.take(0)?;
    Ok(rows)
}

/// Get a single routine group by ID.
pub async fn get_routine_group(
    db: &Database,
    id: &str,
) -> Result<Option<RoutineGroupRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, name, frequency, time_of_day,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM type::record('routine_groups', $id)",
        )
        .bind(("id", id.to_string()))
        .await?;

    let rows: Vec<RoutineGroupRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

/// List routine items for a group, ordered by order_num.
pub async fn list_routine_items(
    db: &Database,
    group_id: &str,
) -> Result<Vec<RoutineItemRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, group_id, name, estimated_duration_min, order_num
             FROM routine_items
             WHERE group_id = $group_id
             ORDER BY order_num ASC",
        )
        .bind(("group_id", group_id.to_string()))
        .await?;

    let rows: Vec<RoutineItemRow> = resp.take(0)?;
    Ok(rows)
}

/// Get completions for a specific group on a specific date.
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

/// Get completion history for a group over the last N days.
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
