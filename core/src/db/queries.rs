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

/// A transaction row from the `transactions` projection table. Nested
/// complex fields (postings, attachment, balancing_posting) come back as
/// `DbValue` since they're stored as FLEXIBLE objects.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct TransactionRow {
    pub id: String,
    pub date: String,
    pub description: String,
    pub postings: DbValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<DbValue>,
    pub category: Option<String>,
    pub tags_top: Vec<String>,
    pub removed: bool,
    pub superseded_by: Option<String>,
    pub merged_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balancing_posting: Option<DbValue>,
    pub cleared: bool,
    pub statement_source: Option<String>,
    pub cleared_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A declared account row.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct AccountRow {
    pub id: String,
    pub commodity: String,
    pub display_name: Option<String>,
    pub last_reconciled_through: Option<String>,
    pub last_statement_balance: Option<String>,
}

/// A budget row.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct BudgetRow {
    pub id: String,
    pub amount: String,
    pub period: String,
    pub removed: bool,
}

/// A detected/confirmed/dismissed recurring pattern.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct RecurringPatternRow {
    pub id: String,
    pub pattern: DbValue,
    pub status: String,
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

/// A pending auto-import batch awaiting user review. Mirrors the projection
/// table `pending_auto_import_batches`. `draft_postings` round-trips the raw
/// `DraftTransaction` array as `DbValue` since the schema declares it as a
/// FLEXIBLE array — the Tauri command deserialises into `DraftTransaction`
/// on its way out.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct PendingBatchRow {
    pub batch_id: String,
    pub source: String,
    pub dedup_key: String,
    pub fetched_at: String,
    pub draft_postings: DbValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_metadata: Option<DbValue>,
    pub status: String,
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

// --- Budget projection (transactions, accounts, budgets, recurring) ---

const TXN_FIELDS: &str = "meta::id(id) AS id, date, description, postings, attachment,
        category, tags_top, removed, superseded_by, merged_ids, balancing_posting,
        cleared, statement_source, cleared_date,
        <string> created_at AS created_at, <string> updated_at AS updated_at";

pub async fn list_transactions(
    db: &Database,
    limit: u32,
    offset: u32,
) -> Result<Vec<TransactionRow>, DbError> {
    let q = format!(
        "SELECT {TXN_FIELDS} FROM transactions
         WHERE removed = false AND superseded_by IS NONE
         ORDER BY date DESC, created_at DESC
         LIMIT $limit START $offset"
    );
    let mut resp = db
        .query(q.as_str())
        .bind(("limit", limit as i64))
        .bind(("offset", offset as i64))
        .await?;
    let rows: Vec<TransactionRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn get_transaction(
    db: &Database,
    txn_id: &str,
) -> Result<Option<TransactionRow>, DbError> {
    let q = format!(
        "SELECT {TXN_FIELDS} FROM type::record('transactions', $txn_id)"
    );
    let mut resp = db
        .query(q.as_str())
        .bind(("txn_id", txn_id.to_string()))
        .await?;
    let rows: Vec<TransactionRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

pub async fn list_accounts(db: &Database) -> Result<Vec<AccountRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, commodity, display_name,
                    last_reconciled_through, last_statement_balance
             FROM accounts
             ORDER BY id ASC",
        )
        .await?;
    let rows: Vec<AccountRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn list_budgets(db: &Database) -> Result<Vec<BudgetRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, amount, period, removed
             FROM budgets
             WHERE removed = false
             ORDER BY id ASC",
        )
        .await?;
    let rows: Vec<BudgetRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn list_recurring_patterns(
    db: &Database,
    status_filter: Option<&str>,
) -> Result<Vec<RecurringPatternRow>, DbError> {
    let (sql, has_filter) = match status_filter {
        Some(_) => (
            "SELECT meta::id(id) AS id, pattern, status
             FROM recurring_patterns
             WHERE status = $status
             ORDER BY id ASC",
            true,
        ),
        None => (
            "SELECT meta::id(id) AS id, pattern, status
             FROM recurring_patterns
             ORDER BY id ASC",
            false,
        ),
    };
    let mut q = db.query(sql);
    if has_filter {
        q = q.bind(("status", status_filter.unwrap().to_string()));
    }
    let mut resp = q.await?;
    let rows: Vec<RecurringPatternRow> = resp.take(0)?;
    Ok(rows)
}

// --- Auto-import pending batches (Phase 3.10.5) ---

const PENDING_BATCH_FIELDS: &str =
    "batch_id, source, dedup_key, fetched_at, draft_postings, source_metadata, status";

pub async fn list_pending_batches(db: &Database) -> Result<Vec<PendingBatchRow>, DbError> {
    let q = format!(
        "SELECT {PENDING_BATCH_FIELDS} FROM pending_auto_import_batches
         WHERE status = 'pending'
         ORDER BY fetched_at DESC"
    );
    let mut resp = db.query(q.as_str()).await?;
    let rows: Vec<PendingBatchRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn count_pending_batches(db: &Database) -> Result<u64, DbError> {
    let mut resp = db
        .query(
            "SELECT count() AS c FROM pending_auto_import_batches
             WHERE status = 'pending' GROUP ALL",
        )
        .await?;
    let counts: Vec<i64> = resp.take("c").unwrap_or_default();
    Ok(counts.first().copied().unwrap_or(0).max(0) as u64)
}

pub async fn get_pending_batch_by_id(
    db: &Database,
    batch_id: &str,
) -> Result<Option<PendingBatchRow>, DbError> {
    let q = format!(
        "SELECT {PENDING_BATCH_FIELDS} FROM pending_auto_import_batches
         WHERE batch_id = $batch_id LIMIT 1"
    );
    let mut resp = db
        .query(q.as_str())
        .bind(("batch_id", batch_id.to_string()))
        .await?;
    let rows: Vec<PendingBatchRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}
