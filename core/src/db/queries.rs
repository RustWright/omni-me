use std::collections::HashSet;

use serde::{Deserialize, Serialize};
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

/// A declared account row (also the override carrier for auto-detected
/// accounts — `hidden`/`display_name` are set via `account_added` upserts).
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct AccountRow {
    pub id: String,
    pub commodity: String,
    pub display_name: Option<String>,
    /// 3.9: when true, drop this account from the auto-detected Accounts screen.
    #[serde(default)]
    pub hidden: bool,
    /// 3.10: when true, this account is a liquid (spendable) asset that counts
    /// toward the "Can I afford X?" verdict. Opt-in — default not-liquid.
    #[serde(default)]
    pub is_liquid: bool,
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
    /// Proposals a later message about the same order displaced, newest last.
    /// Present so a merge is visible to whoever reviews the survivor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<DbValue>,
    /// Set when this batch arrived after an earlier one for the same order was
    /// committed or dismissed. Its books are never touched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revises_batch_id: Option<String>,
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

/// One day's calendar-widget stats. Presence in the result set means an entry
/// exists; the fields carry what the calendar encodes on each cell.
///
/// `raw_text` is returned deliberately rather than a pre-computed word count.
/// Counting means stripping the `⟦timestamp⟧` line tokens first, and the only
/// reader of that format lives in the frontend (`journal.rs`), guarded by a
/// drift test whose whole point is that nothing links the JS writer to its Rust
/// reader. Counting here would create a *third* copy of that format knowledge
/// and a third thing to drift. The frontend counts with the reader it already
/// has and keeps only the resulting number.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct JournalDayStat {
    pub date: String,
    pub complete: bool,
    /// Whether the day has been closed (auto-close or by hand). Distinct from
    /// `complete`: a complete-but-open past day means auto-close did not run.
    pub closed: bool,
    pub raw_text: String,
}

pub async fn list_journal_day_stats(
    db: &Database,
    from_date: &str,
    to_date: &str,
) -> Result<Vec<JournalDayStat>, DbError> {
    let mut resp = db
        .query(
            "SELECT date, complete, closed, raw_text FROM journal_entries
             WHERE date >= $from_date AND date <= $to_date
             ORDER BY date ASC",
        )
        .bind(("from_date", from_date.to_string()))
        .bind(("to_date", to_date.to_string()))
        .await?;

    let rows: Vec<JournalDayStat> = resp.take(0)?;
    Ok(rows)
}

// --- Generic notes ---

pub async fn get_generic_note(db: &Database, id: &str) -> Result<Option<GenericNoteRow>, DbError> {
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

/// Filters for `list_transactions`. All fields optional; an empty struct
/// returns every visible row. Empty/whitespace strings are treated as
/// absent by `normalize` so the frontend can send blank inputs without
/// a separate clear step.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TxnFilter {
    /// Inclusive lower bound on `date` (YYYY-MM-DD).
    pub date_from: Option<String>,
    /// Inclusive upper bound on `date` (YYYY-MM-DD).
    pub date_to: Option<String>,
    /// Case-insensitive substring match against any posting's `account`.
    pub account: Option<String>,
    /// Exact match against `tags_top`.
    pub tag: Option<String>,
    /// Exact match against `category`.
    pub category: Option<String>,
}

impl TxnFilter {
    /// Drop blank strings so the dynamic WHERE clause skips them entirely.
    fn normalize(mut self) -> Self {
        fn blank(s: &Option<String>) -> bool {
            s.as_deref().map(|v| v.trim().is_empty()).unwrap_or(true)
        }
        if blank(&self.date_from) {
            self.date_from = None;
        }
        if blank(&self.date_to) {
            self.date_to = None;
        }
        if blank(&self.account) {
            self.account = None;
        }
        if blank(&self.tag) {
            self.tag = None;
        }
        if blank(&self.category) {
            self.category = None;
        }
        self
    }
}

/// Every distinct value carried by a `key:value` transaction-level tag, read
/// from the **event log**. `known_top_tag_values(db, "globepay-id")` answers
/// "which upstream reference numbers has this ledger already recorded?".
///
/// This exists because a polling auto-import source otherwise has no way to ask
/// what it has already contributed. Without it each tick re-proposes its whole
/// lookback window — the rows are already in the ledger, but nothing downstream
/// compares a draft against what is recorded, so the review queue grows forever.
///
/// **Reads `events`, not the `transactions` projection, and that is the whole
/// point.** The only caller is an auto-import source, which runs in the server
/// — and the server builds its `ProjectionRunner` with an empty projection list
/// (`server/src/lib.rs`), because projecting is the clients' job. Querying the
/// projection here returned an empty set on every real tick while passing a test
/// whose harness happened to include `BudgetProjection`: a filter that looks
/// correct, tests green, and silently does nothing in production. The event log
/// is the one store both sides always have.
///
/// Matching is on the `key:` prefix of a `payload.tags` entry — the shape the
/// ledger importer writes for an inline header tag (`; globepay-id: X`) and the
/// shape [`TransactionRecordedPayload::with_tags`] renders.
///
/// Deliberately counts a transaction that was later removed as still "known":
/// the log has no cheap notion of current visibility, and for a dedup filter
/// re-proposing something the user deleted is the worse failure.
pub async fn known_top_tag_values(db: &Database, key: &str) -> Result<HashSet<String>, DbError> {
    let prefix = format!("{key}:");
    let mut resp = db
        .query(
            "SELECT VALUE payload.tags FROM events
             WHERE event_type = 'transaction_recorded'
               AND payload.tags IS NOT NONE
               AND array::any(payload.tags, |$t| string::starts_with($t, $prefix))",
        )
        .bind(("prefix", prefix.clone()))
        .await?;

    let rows: Vec<Vec<String>> = resp.take(0)?;
    Ok(rows
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.strip_prefix(prefix.as_str()).map(str::to_owned))
        .filter(|value| !value.is_empty())
        .collect())
}

/// How the triage seat's verdicts compare with what the user did with each
/// proposal. Its shadow-mode scorecard: a `none` verdict on a batch the user
/// committed is a purchase the seat would have skipped had it been gating.
#[derive(Debug, Default, serde::Serialize)]
pub struct TriageScore {
    /// `"<verdict> <status>"` → batches, e.g. `"none committed"`.
    pub counts: std::collections::BTreeMap<String, usize>,
    /// Batches with no verdict: proposed before the seat existed, or with it off.
    pub unscored: usize,
    /// The `none` verdicts the user committed, to be read one by one.
    pub misses: Vec<TriageMiss>,
}

#[derive(Debug, serde::Serialize)]
pub struct TriageMiss {
    pub batch_id: String,
    pub sender: Option<String>,
    pub subject: Option<String>,
}

pub async fn triage_score(db: &Database) -> Result<TriageScore, DbError> {
    let mut resp = db
        .query(
            "SELECT batch_id, status,
                    source_metadata.triage AS verdict,
                    source_metadata.subject AS subject,
                    source_metadata.from AS sender
             FROM pending_auto_import_batches",
        )
        .await?;
    let rows: Vec<DbValue> = resp.take(0)?;
    let mut score = TriageScore::default();
    for row in rows.into_iter().map(DbValue::into_json_value) {
        let text = |key: &str| row[key].as_str().map(str::to_owned);
        let (Some(verdict), Some(status)) = (text("verdict"), text("status")) else {
            score.unscored += 1;
            continue;
        };
        if verdict == "none" && status == "committed" {
            score.misses.push(TriageMiss {
                batch_id: text("batch_id").unwrap_or_default(),
                sender: text("sender"),
                subject: text("subject"),
            });
        }
        *score
            .counts
            .entry(format!("{verdict} {status}"))
            .or_default() += 1;
    }
    Ok(score)
}

/// Every batch this source has proposed, as raw payload JSON.
pub async fn proposed_payloads(
    db: &Database,
    source: &str,
) -> Result<Vec<serde_json::Value>, DbError> {
    let mut resp = db
        .query(
            "SELECT VALUE payload FROM events
             WHERE event_type = 'auto_import_batch_proposed'
               AND payload.source = $source",
        )
        .bind(("source", source.to_string()))
        .await?;
    let rows: Vec<DbValue> = resp.take(0)?;
    Ok(rows.into_iter().map(DbValue::into_json_value).collect())
}

/// Every `external_id` this source has already offered for review, read from
/// the `auto_import_batch_proposed` events in the log.
///
/// Complements [`known_top_tag_values`], which only sees rows that made it into
/// the journal. Some upstream rows can never match a journal tag — the ledger
/// records reference numbers for transfers and balance moves but not for card
/// charges — so without this a polling source keeps re-offering them each time
/// its lookback window shifts. Together the two give the semantic the review
/// queue actually wants: **each upstream row is proposed at most once**.
///
/// Deliberately counts dismissed batches as proposed. Dismissing means "not
/// wanted"; bringing the row back on the next tick is the behaviour being fixed.
pub async fn proposed_external_ids(
    db: &Database,
    source: &str,
) -> Result<HashSet<String>, DbError> {
    let mut resp = db
        .query(
            "SELECT VALUE payload.draft_postings FROM events
             WHERE event_type = 'auto_import_batch_proposed'
               AND payload.source = $source
               AND payload.draft_postings IS NOT NONE",
        )
        .bind(("source", source.to_string()))
        .await?;

    let batches: Vec<DbValue> = resp.take(0)?;
    let mut ids = HashSet::new();
    for batch in batches {
        let Some(drafts) = batch.into_json_value().as_array().cloned() else {
            continue;
        };
        for draft in drafts {
            if let Some(id) = draft.get("external_id").and_then(|v| v.as_str())
                && !id.trim().is_empty()
            {
                ids.insert(id.trim().to_owned());
            }
        }
    }
    Ok(ids)
}

pub async fn list_transactions(
    db: &Database,
    filter: TxnFilter,
    limit: u32,
    offset: u32,
) -> Result<Vec<TransactionRow>, DbError> {
    let filter = filter.normalize();
    let mut where_clauses: Vec<&str> = vec!["removed = false", "superseded_by IS NONE"];
    if filter.date_from.is_some() {
        where_clauses.push("date >= $date_from");
    }
    if filter.date_to.is_some() {
        where_clauses.push("date <= $date_to");
    }
    if filter.category.is_some() {
        where_clauses.push("category = $category");
    }
    if filter.tag.is_some() {
        where_clauses.push("$tag IN tags_top");
    }
    if filter.account.is_some() {
        // SurrealDB v3: array::any with a closure returns true if any
        // posting's account contains the substring (case-insensitive).
        where_clauses.push(
            "array::any(postings, |$p| \
             string::lowercase($p.account) CONTAINS string::lowercase($account))",
        );
    }
    let where_sql = where_clauses.join(" AND ");
    let q = format!(
        "SELECT {TXN_FIELDS} FROM transactions
         WHERE {where_sql}
         ORDER BY date DESC, created_at DESC
         LIMIT $limit START $offset"
    );

    let mut query = db
        .query(q.as_str())
        .bind(("limit", limit as i64))
        .bind(("offset", offset as i64));
    if let Some(v) = filter.date_from {
        query = query.bind(("date_from", v));
    }
    if let Some(v) = filter.date_to {
        query = query.bind(("date_to", v));
    }
    if let Some(v) = filter.category {
        query = query.bind(("category", v));
    }
    if let Some(v) = filter.tag {
        query = query.bind(("tag", v));
    }
    if let Some(v) = filter.account {
        query = query.bind(("account", v));
    }
    let mut resp = query.await?;
    let rows: Vec<TransactionRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn get_transaction(
    db: &Database,
    txn_id: &str,
) -> Result<Option<TransactionRow>, DbError> {
    let q = format!("SELECT {TXN_FIELDS} FROM type::record('transactions', $txn_id)");
    let mut resp = db
        .query(q.as_str())
        .bind(("txn_id", txn_id.to_string()))
        .await?;
    let rows: Vec<TransactionRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

/// Fetch every live transaction (not removed, not superseded) for in-Rust query
/// evaluation (Phase 7.2 R2). Unlike [`list_transactions`], this applies no
/// field filter and no DB-side pagination — the query DSL is evaluated host-side
/// over the full set, then the *filtered* result is paginated at the command
/// boundary. At personal scale (a few thousand transactions) loading the live
/// set per query is cheap; a date-range push-down is a possible Cycle-4
/// optimization if this ever grows.
pub async fn query_candidate_transactions(db: &Database) -> Result<Vec<TransactionRow>, DbError> {
    let q = format!(
        "SELECT {TXN_FIELDS} FROM transactions
         WHERE removed = false AND superseded_by IS NONE
         ORDER BY date DESC, created_at DESC"
    );
    let mut resp = db.query(q.as_str()).await?;
    let rows: Vec<TransactionRow> = resp.take(0)?;
    Ok(rows)
}

pub async fn list_accounts(db: &Database) -> Result<Vec<AccountRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, commodity, display_name,
                    hidden, is_liquid
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

const PENDING_BATCH_FIELDS: &str = "batch_id, source, dedup_key, fetched_at, draft_postings, \
     source_metadata, status, superseded, revises_batch_id";

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

// --- Dashboard primitives (Phase 4.5+4.6) ---

/// One transaction's date + raw postings — minimal columns for monthly
/// trend bucketing. Avoid pulling the full `TransactionRow` shape into the
/// aggregator since the dashboard doesn't need merge/clear/attachment
/// fields.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct TxnPostingsRow {
    pub date: String,
    pub postings: DbValue,
}

/// Fetch visible transactions on or after `cutoff_date`, ordered by date.
/// Used by `core::dashboard::monthly_buckets` to compute the income /
/// spending trend without round-tripping the full TransactionRow shape.
pub async fn list_transactions_since(
    db: &Database,
    cutoff_date: &str,
) -> Result<Vec<TxnPostingsRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT date, postings FROM transactions
             WHERE removed = false AND superseded_by IS NONE AND date >= $cutoff
             ORDER BY date ASC",
        )
        .bind(("cutoff", cutoff_date.to_string()))
        .await?;
    let rows: Vec<TxnPostingsRow> = resp.take(0)?;
    Ok(rows)
}

/// Fetch all visible CLEARED transactions on or before `as_of_date`,
/// used by the 5.8 balance check to total cleared activity for an
/// account against a statement closing balance. Returns the minimal
/// posting shape (no need for the full TransactionRow surface).
pub async fn list_cleared_transactions(
    db: &Database,
    as_of_date: &str,
) -> Result<Vec<TxnPostingsRow>, DbError> {
    let mut resp = db
        .query(
            "SELECT date, postings FROM transactions
             WHERE removed = false AND superseded_by IS NONE
               AND cleared = true AND date <= $as_of
             ORDER BY date ASC",
        )
        .bind(("as_of", as_of_date.to_string()))
        .await?;
    let rows: Vec<TxnPostingsRow> = resp.take(0)?;
    Ok(rows)
}

/// Fetch all visible transactions whose `postings` array contains an
/// `Unmatched` account leg — these are the candidates for reconciliation
/// pairing (Phase 5.6). Returns the full `TransactionRow` so the caller
/// can read `statement_source` (drives the clears-statement flag) and
/// `description` (drives the description-similarity signal) in addition
/// to the posting amounts.
pub async fn list_unmatched_transactions(db: &Database) -> Result<Vec<TransactionRow>, DbError> {
    // MUST use `TXN_FIELDS`, never a hand-written field list. This query
    // previously spelled the columns out and so selected a raw `id` (a
    // SurrealDB record id) into `TransactionRow.id: String`, plus raw datetimes
    // into two `String` fields — three deserialize failures that took down the
    // whole reconciliation screen. The constant carries the `meta::id(id)` and
    // `<string>` casts that make the row shape match the struct.
    let q = format!(
        "SELECT {TXN_FIELDS} FROM transactions
         WHERE removed = false
           AND superseded_by IS NONE
           AND array::any(postings, |$p| $p.account = 'Unmatched')
         ORDER BY date ASC"
    );
    let mut resp = db.query(q.as_str()).await?;
    let rows: Vec<TransactionRow> = resp.take(0)?;
    Ok(rows)
}

/// How many transactions still carry an `Unmatched` leg: the reconcile queue's
/// size. The clearing account's balance cannot stand in for it, since rows that
/// net to zero leave hundreds waiting behind a balance of nothing.
pub async fn count_unmatched_transactions(db: &Database) -> Result<u64, DbError> {
    let mut resp = db
        .query(
            "SELECT count() AS c FROM transactions
             WHERE removed = false
               AND superseded_by IS NONE
               AND array::any(postings, |$p| $p.account = 'Unmatched')
             GROUP ALL",
        )
        .await?;
    let counts: Vec<i64> = resp.take("c").unwrap_or_default();
    Ok(counts.first().copied().unwrap_or(0).max(0) as u64)
}

/// The newest settled transactions (no `Unmatched` leg), the history that
/// `reconciliation::CategoryHistory` suggests categories from.
pub async fn list_settled_transactions(
    db: &Database,
    limit: u32,
) -> Result<Vec<TransactionRow>, DbError> {
    let q = format!(
        "SELECT {TXN_FIELDS} FROM transactions
         WHERE removed = false
           AND superseded_by IS NONE
           AND array::any(postings, |$p| $p.account = 'Unmatched') = false
         ORDER BY date DESC
         LIMIT $limit"
    );
    let mut resp = db.query(q.as_str()).bind(("limit", limit)).await?;
    let rows: Vec<TransactionRow> = resp.take(0)?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------

/// Internal row shape for the feedback query. `payload` stays a native
/// SurrealDB value and is converted with `into_json_value()` — the FLEXIBLE
/// object rule.
#[derive(Debug, SurrealValue)]
struct FeedbackEventRow {
    eid: String,
    device_id: String,
    ts: String,
    payload: DbValue,
}

/// One filed problem report: the event envelope's identity and clock, plus the
/// decoded payload.
#[derive(Debug, Clone, Serialize)]
pub struct FeedbackReport {
    /// Event id. Distinct from `payload.feedback_id`, which the client minted —
    /// both are kept because a mismatch between them is itself a finding.
    pub id: String,
    pub device_id: String,
    pub timestamp: String,
    pub report: crate::events::FeedbackCapturedPayload,
}

/// Problem reports, newest first.
///
/// **This is the one query in this module that reads the `events` log directly
/// rather than a projection**, and that is the design rather than a shortcut:
/// feedback has no projection because nothing derives state from it (see
/// `FeedbackCapturedPayload`). With no derived table there is nothing for a
/// direct read to disagree with.
///
/// A report whose payload will not decode is **skipped, not fatal** — one
/// malformed row must not hide every other report from the reader. The count of
/// skipped rows is returned alongside so the caller can say so out loud instead
/// of silently serving a partial list.
pub async fn list_feedback(
    db: &Database,
    since: Option<&str>,
    limit: u32,
) -> Result<(Vec<FeedbackReport>, usize), DbError> {
    // ⚠️ **`timestamp` is projected twice under two different aliases, and each
    // of the three tempting simplifications is a distinct bug.** SurrealDB
    // requires every `ORDER BY` idiom to appear in the selection.
    //   1. `ORDER BY timestamp` with only `AS ts` present → refused as a parse
    //      error. The endpoint shipped this way and reported it as HTTP 200.
    //   2. `ORDER BY ts` → parses, and sorts the *string*: `…00Z` lands above
    //      `…00.5Z` because `Z` (0x5A) > `.` (0x2E), so a fractional second
    //      reverses two reports. Silently.
    //   3. Projecting the bare column back as `timestamp` alongside `AS ts` →
    //      still sorts as a string. The engine resolves the `ORDER BY` idiom to
    //      the *cast* projection, not the raw one — so the ordering column has
    //      to carry a name of its own. Hence `sort_ts`.
    // `sort_ts` is deliberately absent from `FeedbackEventRow`; the driver
    // ignores columns the struct does not name.
    let base = "SELECT meta::id(id) AS eid, device_id,
                       <string> timestamp AS ts, timestamp AS sort_ts, payload
                FROM events
                WHERE event_type = 'feedback_captured'";
    let rows: Vec<FeedbackEventRow> = match since {
        Some(s) => {
            let mut resp = db
                .query(format!(
                    "{base} AND timestamp > type::datetime($since)
                     ORDER BY sort_ts DESC LIMIT $limit"
                ))
                .bind(("since", s.to_string()))
                .bind(("limit", limit))
                .await?;
            resp.take(0)?
        }
        None => {
            let mut resp = db
                .query(format!("{base} ORDER BY sort_ts DESC LIMIT $limit"))
                .bind(("limit", limit))
                .await?;
            resp.take(0)?
        }
    };

    let mut reports = Vec::with_capacity(rows.len());
    let mut skipped = 0usize;
    for row in rows {
        match serde_json::from_value(row.payload.into_json_value()) {
            Ok(report) => reports.push(FeedbackReport {
                id: row.eid,
                device_id: row.device_id,
                timestamp: row.ts,
                report,
            }),
            Err(_) => skipped += 1,
        }
    }
    Ok((reports, skipped))
}

// --- Documents (the archive) ---

/// One field folded onto a document.
///
/// ⚠️ Mirrors `events::DocumentField` rather than reusing it: `.take()` needs the
/// `SurrealValue` derive and the event type carries serde's, which is the right
/// derive for the wire and the wrong one for reading a row back.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct DocumentFieldRow {
    pub key: String,
    pub value: String,
    pub source: String,
    pub verified: bool,
}

/// One document from the `documents` projection table.
///
/// ⛔ **No `text` column, deliberately.** A document's extracted text exists so
/// the archive can be *searched*; it is matched inside the query below and never
/// shipped. Returning it would put the whole corpus's text through the IPC
/// boundary to render a list of filenames, and the detail view renders the
/// document itself rather than the text lifted out of it.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct DocumentRow {
    pub document_id: String,
    pub sha256: Option<String>,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub archived_at: Option<String>,
    pub ingest_source: Option<String>,
    pub text_source: Option<String>,
    pub kind: Option<String>,
    pub title: Option<String>,
    pub document_date: Option<String>,
    /// The tag set, hoisted out of the `tags` field into its own array column.
    ///
    /// Shipped on the row rather than left for the caller to pull out of
    /// `fields`, because the list renders it and the filter matches it — and a
    /// reader parsing the joined value itself is a second decoder to drift.
    pub tags: Option<Vec<String>>,
    /// Whether this document was purged on purpose.
    ///
    /// ⚠️ Carried on the row because the detail view must render a purged entry
    /// as *purged* rather than as a document whose file failed to load — a blob
    /// 404 cannot tell those apart, and the reader would see "couldn't load
    /// attachment" for bytes that were deliberately removed.
    pub purged: Option<bool>,
    pub fields: Option<Vec<DocumentFieldRow>>,
    /// The document this one arrived inside, for an email's attachments.
    ///
    /// ⛔ A link, never ownership — an attachment is a document in its own right
    /// and stays independently listed and searchable. Carried on the row so the
    /// UI can say "arrived inside …" rather than presenting a statement PDF as
    /// though it had been filed on its own.
    pub parent_document_id: Option<String>,
}

/// Rows a purge retired, excluded from every archive read.
///
/// ⛔ `!= true`, not `= false`, matching `store::hidden_clause`: the column is
/// `option<bool>` and absent on every document that was never purged, so
/// `= false` would hide the entire archive.
///
/// ⚠️ One constant because this is a **class** of query, not one query. The
/// enrichment work queues are the easy ones to forget — a purged document with no
/// `kind` would otherwise be selected forever, spending a model call per tick on
/// bytes that no longer exist.
const NOT_PURGED: &str = "purged != true";

/// Every column the archive reads, in one place so list and detail cannot drift.
///
/// ⚠️ `archived_at` is cast to a string because the column is a `datetime`; the
/// same cast the journal and note queries do. It stays in the selection because
/// ⛔ **v3 refuses `ORDER BY` over a field the selection omits.**
const DOCUMENT_COLUMNS: &str = "document_id, sha256, filename, mime_type, size,
     <string> archived_at AS archived_at, ingest_source, text_source,
     kind, title, document_date, tags, purged, fields, parent_document_id";

/// How a document list is narrowed.
///
/// ⚠️ **An empty string is "no filter", never "match empty".** Every field is
/// always bound; the `WHERE` clause is never built by concatenation, which would
/// be one interpolation away from a query a filename could steer.
///
/// A struct rather than five more parameters: four of them are `&str` in a row,
/// so a positional call site read `("", "", "", "receipt", false, …)` and only a
/// careful count told you which filter that was.
#[derive(Debug, Clone, Copy, Default)]
pub struct DocumentFilter<'a> {
    /// Free text over `filename`, `title` and the document's own extracted text,
    /// so a statement is findable by a merchant printed inside it and not only by
    /// whatever the exporting bank named the file.
    pub query: &'a str,
    pub kind: &'a str,
    pub mime: &'a str,
    pub tag: &'a str,
    /// Narrow to documents carrying a field no oracle has checked — the set the
    /// nav badge counts.
    pub unverified_only: bool,
}

/// Documents matching [`DocumentFilter`], newest first.
pub async fn list_documents(
    db: &Database,
    filter: &DocumentFilter<'_>,
    limit: u32,
    offset: u32,
) -> Result<Vec<DocumentRow>, DbError> {
    let DocumentFilter {
        query,
        kind,
        mime,
        tag,
        unverified_only,
    } = *filter;
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS}
         FROM documents
         -- ⚠️ `?? ''` is load-bearing on every one of these. All three columns
         -- are `option<>` (a document with no fields extracted has no title),
         -- and `string::lowercase(NONE)` is a hard query ERROR, not NONE — so
         -- without the coalesce one untitled document fails the whole search.
         WHERE ($q = '' OR string::lowercase(filename ?? '') CONTAINS $q
                       OR string::lowercase(title ?? '') CONTAINS $q
                       OR string::lowercase(text ?? '') CONTAINS $q)
           AND ($kind = '' OR kind = $kind)
           AND ($mime = '' OR mime_type = $mime)
           -- ⛔ No coalesce needed here, unlike the text columns above: checked
           -- against a real database, `NONE CONTAINS 'x'` is `false` rather than
           -- the hard error `string::lowercase(NONE)` gives.
           -- ⛔ `CONTAINS` on an ARRAY is whole-element equality; on a string it is
           -- substring. Verified: `'receipts-2026,taxes' CONTAINS 'receipt'` is
           -- true. That is why the column is an array and not the joined value —
           -- filtering the string would make `receipt` find `receipts-2026`.
           AND ($tag = '' OR tags CONTAINS $tag)
           -- ⛔ Must stay the same predicate as `approvals::
           -- count_documents_with_unverified_fields`. That count is the nav badge
           -- and the assistant's reminder row; this filter is the only way to
           -- reach what it counts, so a drift between them is a queue the user is
           -- told about and cannot open. Both also require NOT_PURGED — a purged
           -- document keeps its fields.
           AND ($unverified = false OR (fields[WHERE verified = false] ?? []) != [])
           AND {NOT_PURGED}
         ORDER BY archived_at DESC
         LIMIT $limit START $offset"
    );
    let mut resp = db
        .query(sql)
        .bind(("q", query.to_ascii_lowercase()))
        .bind(("kind", kind.to_string()))
        .bind(("mime", mime.to_string()))
        // Lowercased to match the stored form — `Tag::normalize` folds case, so a
        // filter that did not would miss every document it should find.
        .bind(("tag", tag.trim().to_lowercase()))
        .bind(("unverified", unverified_only))
        .bind(("limit", limit))
        .bind(("offset", offset))
        .await?;

    let rows: Vec<DocumentRow> = resp.take(0)?;
    Ok(rows)
}

/// The documents that arrived inside this one — an email's attachments.
///
/// ⚠️ **Ordered by filename, not by time.** Every part of one message is
/// archived in a single operation, so `archived_at` orders them arbitrarily and
/// the order would change between rebuilds.
///
/// ⛔ Attachments stay in [`list_documents`] as well. They are documents in
/// their own right — a statement PDF is worth finding whether or not the reader
/// remembers it came by email — and hiding them from the list would make the
/// archive's own breadth claim false.
pub async fn document_children(
    db: &Database,
    parent_id: &str,
) -> Result<Vec<DocumentRow>, DbError> {
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS} FROM documents
         WHERE parent_document_id = $parent AND {NOT_PURGED}
         ORDER BY filename"
    );
    let mut resp = db
        .query(sql)
        .bind(("parent", parent_id.to_string()))
        .await?;

    let rows: Vec<DocumentRow> = resp.take(0)?;
    Ok(rows)
}

/// One document by its id, or `None` when nothing has folded onto that row.
pub async fn get_document(db: &Database, id: &str) -> Result<Option<DocumentRow>, DbError> {
    let sql = format!("SELECT {DOCUMENT_COLUMNS} FROM type::record('documents', $id)");
    let mut resp = db.query(sql).bind(("id", id.to_string())).await?;

    let rows: Vec<DocumentRow> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

/// One document's extracted text.
///
/// ⚠️ **Its own query, deliberately.** [`DocumentRow`] omits `text` so a list of
/// a thousand filenames does not drag the whole corpus's text through the IPC
/// boundary. This is the single-document read for the surfaces that genuinely
/// need the words: an archived email shown beside the draft transactions it
/// produced, where the reviewer's only alternative is approving a value whose
/// source they cannot see.
pub async fn document_text(db: &Database, id: &str) -> Result<Option<String>, DbError> {
    let mut resp = db
        .query("SELECT VALUE text FROM type::record('documents', $id)")
        .bind(("id", id.to_string()))
        .await?;

    let rows: Vec<Option<String>> = resp.take(0)?;
    Ok(rows
        .into_iter()
        .flatten()
        .next()
        .map(|t| crate::mime::strip_invisible_padding(&t)))
}

/// Every distinct `kind` present, for the filter control.
///
/// ⛔ Derived from the data, never a hardcoded list. Kinds come from parsers and
/// from an open-ended model vocabulary, so a fixed list would silently hide any
/// document whose kind the UI had not been taught about.
pub async fn document_kinds(db: &Database) -> Result<Vec<String>, DbError> {
    let mut resp = db
        .query(format!(
            "SELECT VALUE kind FROM documents
                 WHERE kind != NONE AND {NOT_PURGED} GROUP BY kind ORDER BY kind"
        ))
        .await?;

    let rows: Vec<Option<String>> = resp.take(0)?;
    Ok(rows.into_iter().flatten().collect())
}

/// Every tag in use across the archive, for the filter control.
///
/// ⛔ Derived from the data for [`document_kinds`]' reason, and more so: tags
/// come from a person typing them, so no list written in advance could be right.
///
/// ⚠️ **The split between the two halves is forced, not stylistic.** Under
/// `GROUP ALL`, `array::group` collects each row's array *without* flattening, so
/// the flatten is needed — and it is the only wrapper allowed: `array::distinct`
/// and `array::sort` are aggregates too, and nesting one over `array::group`
/// fails with "Nested aggregate functions are not supported". So the database
/// flattens and the caller deduplicates.
///
/// ⛔ Verified against a real database, not inferred from the function names. The
/// first attempt here returned an array of arrays that decoded as an error, and
/// the shape a wrong-but-parseable aggregate returns is an empty list — which
/// reads exactly like an archive with no tags in it.
pub async fn document_tags(db: &Database) -> Result<Vec<String>, DbError> {
    let mut resp = db
        .query(format!(
            "SELECT VALUE array::flatten(array::group(tags))
                 FROM documents WHERE tags != NONE AND {NOT_PURGED} GROUP ALL"
        ))
        .await?;

    let rows: Vec<Vec<String>> = resp.take(0)?;
    let mut tags = rows.into_iter().next().unwrap_or_default();
    tags.sort();
    tags.dedup();
    Ok(tags)
}

/// How many live things still reference a blob.
///
/// 🔴 **Two referrers, and they are read from two different places.**
/// `documents.sha256` comes from the projection. Transaction attachments come
/// from the **event log**, because the server — the one host that holds the
/// blobs, and therefore the only place a purge can delete them — builds its
/// `ProjectionRunner` with `DocumentsProjection` alone. `transactions` does not
/// exist there, and querying a missing table is a hard error, not an empty set.
///
/// ⛔ That distinction is the difference between a purge that fails loudly on the
/// server and one that silently reports "no references" and deletes bytes a
/// committed receipt still points at, leaving a ledger entry whose evidence is
/// gone. `known_top_tag_values` learned the same lesson: a filter that looks
/// correct, tests green against a harness carrying `BudgetProjection`, and does
/// nothing in production.
///
/// ⚠️ **Deliberately conservative.** The log has no cheap notion of current
/// visibility, so a transaction the user later deleted still counts. Over-counting
/// keeps bytes that might be reclaimable; under-counting destroys bytes something
/// needs. Only one of those is recoverable.
///
/// ⛔ Purged documents do not count — that is what makes a blob become
/// reclaimable once every document naming it has gone.
/// The id and `text_source` of an unpurged document holding exactly these
/// bytes, if any. What lets a bulk upload be re-run without filing every
/// document twice.
pub async fn archived_document_with_sha(
    db: &Database,
    sha256: &str,
) -> Result<Option<(String, String)>, DbError> {
    let sql = format!(
        "SELECT document_id, text_source FROM documents
         WHERE sha256 = $h AND {NOT_PURGED} LIMIT 1"
    );
    let mut resp = db.query(sql).bind(("h", sha256.to_string())).await?;
    let rows: Vec<DbValue> = resp.take(0)?;
    Ok(rows.into_iter().next().map(|row| {
        let row = row.into_json_value();
        let text = |key: &str| row[key].as_str().unwrap_or_default().to_string();
        (text("document_id"), text("text_source"))
    }))
}

pub async fn blob_reference_count(db: &Database, sha256: &str) -> Result<usize, DbError> {
    let docs_sql = format!(
        "SELECT VALUE count() FROM documents
         WHERE sha256 = $h AND {NOT_PURGED} GROUP ALL"
    );
    let mut resp = db.query(docs_sql).bind(("h", sha256.to_string())).await?;
    let docs: Vec<i64> = resp.take(0)?;

    // ⚠️ Both shapes, and both are needed: a merge writes the surviving
    // attachment as `combined_attachment` on a different event type, so counting
    // `transaction_recorded` alone would miss every reconciled receipt.
    let mut resp = db
        .query(
            "SELECT VALUE count() FROM events
             WHERE (event_type = 'transaction_recorded'
                    AND payload.attachment.sha256 = $h)
                OR (event_type = 'transactions_merged'
                    AND payload.combined_attachment.sha256 = $h)
             GROUP ALL",
        )
        .bind(("h", sha256.to_string()))
        .await?;
    let txns: Vec<i64> = resp.take(0)?;

    let total = docs.first().copied().unwrap_or(0) + txns.first().copied().unwrap_or(0);
    Ok(total.max(0) as usize)
}

/// Every live document carrying `tag`, oldest first.
///
/// ⛔ The **same selection the purge applies**, so a preview cannot promise one
/// thing and the confirm do another — the rule `preview_obsidian_export` states
/// outright and the reason it resolves names exactly as the real export does.
///
/// ⚠️ Oldest first, unlike every other archive read. A purge queue is worked from
/// the end a retention window reaches first, and newest-first would put the items
/// least likely to be purged at the top of the list a person has to scroll.
///
/// ⚠️ Uncapped on purpose: the caller needs the true total and the true byte
/// figures, and truncating here would understate both. The *display* list is cut
/// by `purge::MAX_PREVIEW_ITEMS`, which is a different decision.
pub async fn documents_tagged(
    db: &Database,
    tag: &str,
    archived_before: Option<&str>,
) -> Result<Vec<DocumentRow>, DbError> {
    // ⚠️ The cutoff narrows the same selection rather than forming a second one,
    // so a retention group and a whole-tag group go through one query. ⛔ And it
    // is cast: comparing a `datetime` column against a bound string matches
    // nothing instead of erroring.
    let cutoff = match archived_before {
        Some(_) => "AND archived_at != NONE AND archived_at < type::datetime($before)",
        None => "",
    };
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS}
         FROM documents
         WHERE tags CONTAINS $tag AND {NOT_PURGED} {cutoff}
         ORDER BY archived_at ASC"
    );
    let mut query = db.query(sql).bind(("tag", tag.trim().to_lowercase()));
    if let Some(before) = archived_before {
        query = query.bind(("before", before.to_string()));
    }
    let mut resp = query.await?;
    Ok(resp.take(0)?)
}

/// Every tag with a live retention rule, and the days it keeps for.
///
/// ⛔ A cleared rule (`keep_days = NONE`) is **omitted**, never returned as zero —
/// zero would purge that tag's whole history. See `crate::retention`.
pub async fn retention_rules(db: &Database) -> Result<Vec<(String, u32)>, DbError> {
    #[derive(Debug, SurrealValue)]
    struct RuleRow {
        tag: String,
        keep_days: Option<i64>,
    }

    let mut resp = db
        .query("SELECT tag, keep_days FROM document_retention WHERE keep_days != NONE")
        .await?;
    let rows: Vec<RuleRow> = resp.take(0)?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let days = r.keep_days?;
            u32::try_from(days).ok().map(|d| (r.tag, d))
        })
        .collect())
}

/// Live documents archived before `boundary`, oldest first.
///
/// The prefilter under retention's exact evaluation — see `crate::retention`. It
/// carries `tags`, because the rule the caller applies is a property of them.
///
/// ⚠️ `boundary` is RFC3339 and **must** be cast: the column is a `datetime`, and
/// comparing it against a bound string matches nothing at all rather than erroring
/// — an empty candidate list that reads exactly like "nothing is due".
pub async fn documents_archived_before(
    db: &Database,
    boundary: &str,
    limit: u32,
) -> Result<Vec<DocumentRow>, DbError> {
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS}
         FROM documents
         WHERE archived_at != NONE AND archived_at < type::datetime($boundary) AND {NOT_PURGED}
         ORDER BY archived_at ASC
         LIMIT $limit"
    );
    let mut resp = db
        .query(sql)
        .bind(("boundary", boundary.to_string()))
        .bind(("limit", limit))
        .await?;
    Ok(resp.take(0)?)
}

/// Documents no producer has catalogued yet, newest first.
///
/// Why the query is the work queue, why newest-first, and why the MIME filter
/// belongs here rather than in the caller: `docs/src/archive.md`.
pub async fn documents_awaiting_fields(
    db: &Database,
    mimes: &[&str],
    limit: u32,
    held: &[String],
) -> Result<Vec<DocumentRow>, DbError> {
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS}
         FROM documents
         WHERE kind = NONE AND (mime_type ?? '') IN $mimes AND document_id NOT IN $held
           AND {NOT_PURGED}
         ORDER BY archived_at DESC
         LIMIT $limit"
    );
    let mut resp = db
        .query(sql)
        .bind((
            "mimes",
            mimes.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
        ))
        .bind(("limit", limit))
        .bind(("held", held.to_vec()))
        .await?;

    let rows: Vec<DocumentRow> = resp.take(0)?;
    Ok(rows)
}

/// How many uncatalogued documents no configured reader could ever accept.
///
/// Reported rather than silently excluded: these never enter the candidate set,
/// so without a count they would look like documents that simply never arrived.
pub async fn documents_unreadable_count(db: &Database, mimes: &[&str]) -> Result<usize, DbError> {
    let mut resp = db
        .query(format!(
            "SELECT VALUE count() FROM documents
                 WHERE kind = NONE AND (mime_type ?? '') NOT IN $mimes
                   AND {NOT_PURGED} GROUP ALL"
        ))
        .bind((
            "mimes",
            mimes.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
        ))
        .await?;

    let rows: Vec<i64> = resp.take(0)?;
    Ok(rows.into_iter().next().unwrap_or(0).max(0) as usize)
}

/// Which models have already tried to read this document, oldest first.
///
/// ⛔ From the **event log**, not the projection: the log holds one
/// `document_text_transcribed` per read, so it is already the attempt record and
/// needs no column and no projection bump. What reads it:
/// `crate::document_enrichment`.
pub async fn transcription_attempts(
    db: &Database,
    document_id: &str,
) -> Result<Vec<String>, DbError> {
    // ⚠️ `sort_ts` rather than ordering by `timestamp` directly, and the same
    // reason as `feedback_events`: SurrealDB resolves an `ORDER BY` idiom against
    // the *selection*, so a column the projection does not name is a parse error
    // rather than a sort. `AttemptRow` ignores it; the driver drops what the
    // struct does not name.
    #[derive(Debug, SurrealValue)]
    struct AttemptRow {
        model: Option<String>,
    }

    let mut resp = db
        .query(
            "SELECT payload.model AS model, timestamp AS sort_ts FROM events
             WHERE event_type = 'document_text_transcribed'
               AND payload.document_id = $id
             ORDER BY sort_ts ASC",
        )
        .bind(("id", document_id.to_string()))
        .await?;

    let rows: Vec<AttemptRow> = resp.take(0)?;
    Ok(rows.into_iter().filter_map(|r| r.model).collect())
}

/// Documents that still have no text, newest first.
///
/// Selects the scans a transcriber exists for — including ones a model has
/// already read and returned nothing for. See the note on the query.
pub async fn documents_awaiting_text(
    db: &Database,
    mimes: &[&str],
    limit: u32,
    held: &[String],
) -> Result<Vec<DocumentRow>, DbError> {
    // ⚠️ Keyed on the text being EMPTY, not on `text_source = 'none'`: an empty
    // transcription still lifts the source off `none`, so the narrower rule retired
    // a document on one blank answer. What bounds the re-reading instead is
    // `transcription_attempts` — see `crate::document_enrichment`.
    let sql = format!(
        "SELECT {DOCUMENT_COLUMNS}
         FROM documents
         WHERE (text ?? '') = '' AND (mime_type ?? '') IN $mimes
           AND document_id NOT IN $held
           AND {NOT_PURGED}
         ORDER BY archived_at DESC
         LIMIT $limit"
    );
    let mut resp = db
        .query(sql)
        .bind((
            "mimes",
            mimes.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
        ))
        .bind(("limit", limit))
        .bind(("held", held.to_vec()))
        .await?;

    let rows: Vec<DocumentRow> = resp.take(0)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{BudgetProjection, Projection};

    #[tokio::test]
    async fn triage_score_counts_verdicts_against_decisions_and_lists_misses() {
        let db = crate::db::test_db().await;
        crate::events::AutoImportProjection
            .init_schema(&db)
            .await
            .unwrap();
        for (id, verdict, status) in [
            ("b1", Some("money"), "committed"),
            ("b2", Some("none"), "dismissed"),
            ("b3", Some("none"), "committed"),
            ("b4", None, "committed"),
        ] {
            let meta = match verdict {
                Some(v) => serde_json::json!({ "triage": v, "from": "shop@x.com", "subject": id }),
                None => serde_json::json!({ "from": "shop@x.com", "subject": id }),
            };
            db.query(
                "CREATE pending_auto_import_batches CONTENT {
                    batch_id: $id, source: 'mail', dedup_key: $id, fetched_at: '2026-10-01',
                    draft_postings: [], source_metadata: $meta, status: $status }",
            )
            .bind(("id", id))
            .bind(("meta", meta))
            .bind(("status", status))
            .await
            .unwrap()
            .check()
            .unwrap();
        }

        let score = triage_score(&db).await.unwrap();

        assert_eq!(score.counts["money committed"], 1);
        assert_eq!(score.counts["none dismissed"], 1);
        assert_eq!(score.counts["none committed"], 1);
        assert_eq!(
            score.unscored, 1,
            "a batch from before the seat is not scored"
        );
        assert_eq!(score.misses.len(), 1);
        assert_eq!(score.misses[0].batch_id, "b3");
        assert_eq!(score.misses[0].subject.as_deref(), Some("b3"));
    }

    /// Temp DB with the `transactions` table defined. Uses the real projection
    /// schema rather than a hand-written DEFINE, so a schema change that breaks
    /// a row shape shows up here instead of only in production.
    async fn txn_db() -> Database {
        let db = crate::db::test_db().await;
        BudgetProjection.init_schema(&db).await.unwrap();
        db
    }

    /// `id` goes into the record id unescaped: plain alphanumerics only. A hyphen
    /// fails the CREATE, and `unwrap` here does not see a statement's error.
    async fn insert_txn(db: &Database, id: &str, account: &str) {
        db.query(
            format!(
                "CREATE transactions:{id} CONTENT {{
                    date: '2026-09-01',
                    description: 'test txn',
                    postings: [{{ account: '{account}', amount: '10.00', commodity: 'CAD' }}],
                    category: NONE,
                    tags_top: [],
                    removed: false,
                    superseded_by: NONE,
                    merged_ids: [],
                    cleared: false,
                    statement_source: NONE,
                    cleared_date: NONE,
                    created_at: d'2026-09-01T00:00:00Z',
                    updated_at: d'2026-09-01T00:00:00Z'
                }}"
            )
            .as_str(),
        )
        .await
        .unwrap();
    }

    // --- Documents (the archive) ---

    async fn doc_db() -> Database {
        let db = crate::db::test_db().await;
        crate::events::DocumentsProjection
            .init_schema(&db)
            .await
            .unwrap();
        db
    }

    /// Fold one document in through the real projection, so a schema or fold
    /// change breaks these reads rather than only production.
    async fn fold_doc(
        db: &Database,
        id: &str,
        filename: &str,
        text: &str,
        archived_at: &str,
        kind: Option<&str>,
    ) {
        use crate::events::{Event, EventType, Projection, validate_payload};

        let mut events = vec![serde_json::json!({
            "document_id": id,
            "sha256": "a".repeat(64),
            "filename": filename,
            "mime_type": "application/pdf",
            "size": 1024u64,
            "archived_at": archived_at,
            "source": "bulk",
            "text": text,
            "text_source": "extracted",
        })]
        .into_iter()
        .map(|p| (EventType::DocumentArchived, p))
        .collect::<Vec<_>>();

        if let Some(kind) = kind {
            events.push((
                EventType::DocumentFieldsExtracted,
                serde_json::json!({
                    "document_id": id,
                    "extracted_at": archived_at,
                    "fields": [
                        { "key": "kind", "value": kind,
                          "source": "parser:statement-brokerage", "verified": false },
                        { "key": "closing_balance", "value": "950.00",
                          "source": "parser:statement-brokerage", "verified": true },
                    ],
                }),
            ));
        }

        for (event_type, payload) in events {
            validate_payload(&event_type, &payload).expect("payload must be valid");
            let event = Event {
                id: ulid::Ulid::new().to_string(),
                event_type: event_type.to_string(),
                aggregate_id: id.to_string(),
                timestamp: chrono::Utc::now(),
                device_id: "test-device".to_string(),
                payload,
                received_at: None,
            };
            crate::events::DocumentsProjection
                .apply(&event, db)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn documents_come_back_newest_first() {
        let db = doc_db().await;
        fold_doc(&db, "old", "a.pdf", "", "2024-01-01T00:00:00Z", None).await;
        fold_doc(&db, "new", "b.pdf", "", "2026-09-01T00:00:00Z", None).await;

        let rows = list_documents(&db, &DocumentFilter::default(), 50, 0)
            .await
            .unwrap();

        let ids: Vec<&str> = rows.iter().map(|r| r.document_id.as_str()).collect();
        assert_eq!(ids, vec!["new", "old"]);
    }

    #[tokio::test]
    async fn a_search_reaches_inside_the_document_not_only_its_name() {
        // ⚠️ The point of storing text on the event. A bank names its export
        // `stmt_0041.pdf`; the only way to find it is by what it says.
        let db = doc_db().await;
        fold_doc(
            &db,
            "stmt",
            "stmt_0041.pdf",
            "Payment to HYDRO QUEBEC",
            "2026-01-01T00:00:00Z",
            None,
        )
        .await;
        fold_doc(
            &db,
            "other",
            "lease.pdf",
            "nothing relevant",
            "2026-01-02T00:00:00Z",
            None,
        )
        .await;

        let hits = list_documents(
            &db,
            &DocumentFilter {
                query: "hydro quebec",
                ..Default::default()
            },
            50,
            0,
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].document_id, "stmt");

        // Case-insensitive both ways, and the filename still matches.
        assert_eq!(
            list_documents(
                &db,
                &DocumentFilter {
                    query: "LEASE",
                    ..Default::default()
                },
                50,
                0
            )
            .await
            .unwrap()
            .len(),
            1
        );
        // An empty query is not a filter.
        assert_eq!(
            list_documents(&db, &DocumentFilter::default(), 50, 0)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn a_kind_filter_narrows_and_kinds_come_from_the_data() {
        let db = doc_db().await;
        fold_doc(
            &db,
            "s1",
            "s1.csv",
            "",
            "2026-01-01T00:00:00Z",
            Some("brokerage_statement"),
        )
        .await;
        fold_doc(&db, "u1", "u1.pdf", "", "2026-01-02T00:00:00Z", None).await;

        let narrowed = list_documents(
            &db,
            &DocumentFilter {
                kind: "brokerage_statement",
                ..Default::default()
            },
            50,
            0,
        )
        .await
        .unwrap();
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].document_id, "s1");

        assert_eq!(
            document_kinds(&db).await.unwrap(),
            vec!["brokerage_statement"],
            "⛔ derived from the data — a hardcoded list would hide any new kind"
        );
    }

    /// Fold one archived document with an explicit MIME type and parent link —
    /// the two columns `fold_doc` hardcodes, and the two the mail view turns on.
    async fn fold_part(db: &Database, id: &str, filename: &str, mime: &str, parent: Option<&str>) {
        use crate::events::{Event, EventType, Projection, validate_payload};

        let mut payload = serde_json::json!({
            "document_id": id,
            "sha256": "a".repeat(64),
            "filename": filename,
            "mime_type": mime,
            "size": 1024u64,
            "archived_at": "2026-03-04T07:12:00Z",
            "source": "email",
            "text": "",
            "text_source": "extracted",
        });
        if let Some(parent) = parent {
            payload["parent_document_id"] = serde_json::json!(parent);
        }
        validate_payload(&EventType::DocumentArchived, &payload).expect("payload must be valid");
        crate::events::DocumentsProjection
            .apply(
                &Event {
                    id: ulid::Ulid::new().to_string(),
                    event_type: EventType::DocumentArchived.to_string(),
                    aggregate_id: id.to_string(),
                    timestamp: chrono::Utc::now(),
                    device_id: "test-device".to_string(),
                    payload,
                    received_at: None,
                },
                db,
            )
            .await
            .unwrap();
    }

    /// ⛔ Scoping the archive to mail filters on `mime_type`, never on `kind`.
    /// Mail has no kind — field extraction has not run for it — so a kind-based
    /// filter would return an empty list and read as an empty archive.
    #[tokio::test]
    async fn the_mime_filter_scopes_the_archive_to_mail() {
        let db = doc_db().await;
        fold_part(&db, "mail", "statement.eml", "message/rfc822", None).await;
        fold_part(&db, "pdf", "statement.pdf", "application/pdf", Some("mail")).await;

        let mail = list_documents(
            &db,
            &DocumentFilter {
                mime: "message/rfc822",
                ..Default::default()
            },
            50,
            0,
        )
        .await
        .unwrap();
        assert_eq!(mail.len(), 1, "{mail:?}");
        assert_eq!(mail[0].document_id, "mail");
        assert_eq!(mail[0].kind, None, "the filter must not depend on a kind");

        assert_eq!(
            list_documents(&db, &DocumentFilter::default(), 50, 0)
                .await
                .unwrap()
                .len(),
            2,
            "⛔ an attachment stays listed in its own right — hiding it would \
             make the archive's breadth claim false"
        );
    }

    /// Fold a document carrying a tag set, through the real projection.
    ///
    /// `tags_value` is the stored joined form, so a test can hand in exactly what
    /// a badly-normalized writer would and see what the hoist makes of it.
    async fn fold_doc_with_tags(db: &Database, id: &str, tags_value: &str) {
        use crate::events::{Event, EventType, Projection, validate_payload};

        let payloads = vec![
            (
                EventType::DocumentArchived,
                serde_json::json!({
                    "document_id": id,
                    "sha256": "b".repeat(64),
                    "filename": format!("{id}.pdf"),
                    "mime_type": "application/pdf",
                    "size": 10u64,
                    "archived_at": "2026-09-01T00:00:00Z",
                    "source": "bulk",
                    "text_source": "none",
                }),
            ),
            (
                EventType::DocumentFieldsExtracted,
                serde_json::json!({
                    "document_id": id,
                    "extracted_at": "2026-09-01T00:00:00Z",
                    "fields": [
                        { "key": "tags", "value": tags_value,
                          "source": "human", "verified": true },
                    ],
                }),
            ),
        ];

        for (event_type, payload) in payloads {
            validate_payload(&event_type, &payload).expect("payload must be valid");
            let event = Event {
                id: ulid::Ulid::new().to_string(),
                event_type: event_type.to_string(),
                aggregate_id: id.to_string(),
                timestamp: chrono::Utc::now(),
                device_id: "test-device".to_string(),
                payload,
                received_at: None,
            };
            crate::events::DocumentsProjection
                .apply(&event, db)
                .await
                .unwrap();
        }
    }

    /// Fold one document carrying a single field with the given `verified` flag,
    /// or, for `None`, archived and never extracted: no `fields` at all.
    async fn fold_doc_with_verified_field(db: &Database, id: &str, verified: Option<bool>) {
        use crate::events::{Event, EventType, Projection, validate_payload};

        let mut payloads = vec![(
            EventType::DocumentArchived,
            serde_json::json!({
                "document_id": id,
                "sha256": "c".repeat(64),
                "filename": format!("{id}.pdf"),
                "mime_type": "application/pdf",
                "size": 10u64,
                "archived_at": "2026-09-01T00:00:00Z",
                "source": "bulk",
                "text_source": "none",
            }),
        )];
        if let Some(verified) = verified {
            payloads.push((
                EventType::DocumentFieldsExtracted,
                serde_json::json!({
                    "document_id": id,
                    "extracted_at": "2026-09-01T00:00:00Z",
                    "fields": [
                        { "key": "total", "value": "12.00",
                          "source": "model", "verified": verified },
                    ],
                }),
            ));
        }

        for (event_type, payload) in payloads {
            validate_payload(&event_type, &payload).expect("payload must be valid");
            let event = Event {
                id: ulid::Ulid::new().to_string(),
                event_type: event_type.to_string(),
                aggregate_id: id.to_string(),
                timestamp: chrono::Utc::now(),
                device_id: "test-device".to_string(),
                payload,
                received_at: None,
            };
            crate::events::DocumentsProjection
                .apply(&event, db)
                .await
                .unwrap();
        }
    }

    /// ⛔ The set the nav badge counts must be reachable. `approvals::summary`
    /// counts documents with any `verified = false` field; this filter is the
    /// only way into that set, so the two predicates have to agree — if they
    /// drift, the badge names a queue the archive cannot open.
    #[tokio::test]
    async fn the_unverified_filter_returns_exactly_what_the_badge_counts() {
        let db = doc_db().await;
        fold_doc_with_verified_field(&db, "unchecked", Some(false)).await;
        fold_doc_with_verified_field(&db, "checked", Some(true)).await;
        // Never extracted: nothing to verify. Counted as waiting once, 795 times
        // over on dev (2026-10-07), because filtering an absent array is NONE.
        fold_doc_with_verified_field(&db, "unread", None).await;

        let narrowed = list_documents(
            &db,
            &DocumentFilter {
                unverified_only: true,
                ..Default::default()
            },
            50,
            0,
        )
        .await
        .unwrap();
        assert_eq!(
            narrowed.len(),
            1,
            "only the document with an unchecked field is listed"
        );
        assert_eq!(narrowed[0].document_id, "unchecked");

        // ⚠️ `false` is "no filter", not "only verified" — the same convention
        // every other filter on this query follows.
        let all = list_documents(&db, &DocumentFilter::default(), 50, 0)
            .await
            .unwrap();
        assert_eq!(all.len(), 3, "off, the filter narrows nothing");
    }

    /// A document is counted once however many of its fields are unchecked, and
    /// one checked field alongside does not exempt it.
    #[tokio::test]
    async fn a_partly_checked_document_still_needs_review() {
        use crate::events::{Event, EventType, Projection, validate_payload};
        let db = doc_db().await;

        let payloads = vec![
            (
                EventType::DocumentArchived,
                serde_json::json!({
                    "document_id": "mixed",
                    "sha256": "d".repeat(64),
                    "filename": "mixed.pdf",
                    "mime_type": "application/pdf",
                    "size": 10u64,
                    "archived_at": "2026-09-01T00:00:00Z",
                    "source": "bulk",
                    "text_source": "none",
                }),
            ),
            (
                EventType::DocumentFieldsExtracted,
                serde_json::json!({
                    "document_id": "mixed",
                    "extracted_at": "2026-09-01T00:00:00Z",
                    "fields": [
                        { "key": "total", "value": "12.00",
                          "source": "human", "verified": true },
                        { "key": "tax", "value": "1.20",
                          "source": "model", "verified": false },
                    ],
                }),
            ),
        ];
        for (event_type, payload) in payloads {
            validate_payload(&event_type, &payload).expect("payload must be valid");
            let event = Event {
                id: ulid::Ulid::new().to_string(),
                event_type: event_type.to_string(),
                aggregate_id: "mixed".to_string(),
                timestamp: chrono::Utc::now(),
                device_id: "test-device".to_string(),
                payload,
                received_at: None,
            };
            crate::events::DocumentsProjection
                .apply(&event, &db)
                .await
                .unwrap();
        }

        let narrowed = list_documents(
            &db,
            &DocumentFilter {
                unverified_only: true,
                ..Default::default()
            },
            50,
            0,
        )
        .await
        .unwrap();
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].document_id, "mixed");
    }

    /// The joined field value becomes an array column, and the filter matches a
    /// whole element.
    #[tokio::test]
    async fn the_tag_field_is_hoisted_into_an_array_and_filters_on_it() {
        let db = doc_db().await;
        fold_doc_with_tags(&db, "groceries", "receipt,groceries").await;
        fold_doc_with_tags(&db, "lease", "lease").await;

        let all = list_documents(&db, &DocumentFilter::default(), 50, 0)
            .await
            .unwrap();
        let hoisted = all
            .iter()
            .find(|r| r.document_id == "groceries")
            .expect("the row is listed");
        assert_eq!(
            hoisted.tags.as_deref(),
            Some(&["receipt".to_string(), "groceries".to_string()][..]),
            "the joined value must arrive as elements, not one string"
        );

        let hits = list_documents(
            &db,
            &DocumentFilter {
                tag: "receipt",
                ..Default::default()
            },
            50,
            0,
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].document_id, "groceries");

        // Case folds, matching how the tag was normalized on the way in.
        assert_eq!(
            list_documents(
                &db,
                &DocumentFilter {
                    tag: "RECEIPT",
                    ..Default::default()
                },
                50,
                0
            )
            .await
            .unwrap()
            .len(),
            1
        );
        // An empty tag is not a filter.
        assert_eq!(
            list_documents(&db, &DocumentFilter::default(), 50, 0)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    /// ⛔ `CONTAINS` on an array is whole-element equality. A substring match
    /// would make `receipt` find `receipts-2026`, which is the bug the array
    /// column exists to prevent.
    #[tokio::test]
    async fn a_tag_filter_never_matches_a_prefix_of_another_tag() {
        let db = doc_db().await;
        fold_doc_with_tags(&db, "plural", "receipts-2026").await;

        let hits = list_documents(
            &db,
            &DocumentFilter {
                tag: "receipt",
                ..Default::default()
            },
            50,
            0,
        )
        .await
        .unwrap();
        assert!(hits.is_empty(), "matched a prefix: {hits:?}");
    }

    /// An untagged document must not empty the filter.
    ///
    /// ⚠️ Most of a real archive has no tags, so this is the ordinary case rather
    /// than an edge one. It holds because `NONE CONTAINS 'x'` is `false` — which is
    /// worth a test precisely because the neighbouring `string::lowercase(NONE)`
    /// *is* a hard error, so the behaviour here is not the one you would guess.
    #[tokio::test]
    async fn an_untagged_document_does_not_break_the_tag_filter() {
        let db = doc_db().await;
        fold_doc(&db, "bare", "bare.pdf", "", "2026-01-01T00:00:00Z", None).await;
        fold_doc_with_tags(&db, "tagged", "receipt").await;

        let hits = list_documents(
            &db,
            &DocumentFilter {
                tag: "receipt",
                ..Default::default()
            },
            50,
            0,
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].document_id, "tagged");

        let untagged = list_documents(&db, &DocumentFilter::default(), 50, 0)
            .await
            .unwrap();
        assert_eq!(
            untagged
                .iter()
                .find(|r| r.document_id == "bare")
                .and_then(|r| r.tags.as_ref()),
            None,
            "absent must stay absent, not become an empty array"
        );
    }

    /// The filter control's vocabulary, flattened and deduplicated in the query.
    ///
    /// ⚠️ This is the test that proves the SurrealQL, not the intent: an
    /// aggregate spelled wrong returns an empty list rather than failing, which
    /// reads exactly like an archive with no tags.
    #[tokio::test]
    async fn every_tag_in_use_comes_back_once_sorted() {
        let db = doc_db().await;
        fold_doc_with_tags(&db, "a", "receipt,groceries").await;
        fold_doc_with_tags(&db, "b", "receipt,institution:rbc").await;
        fold_doc(&db, "c", "c.pdf", "", "2026-01-01T00:00:00Z", None).await;

        let tags = document_tags(&db).await.unwrap();
        assert_eq!(
            tags,
            vec!["groceries", "institution:rbc", "receipt"],
            "flattened, deduplicated and sorted"
        );
    }

    async fn seed_doc_with_hash(db: &Database, id: &str, sha: &str, purged: bool) {
        db.query(
            "UPSERT type::record('documents', $id) SET document_id = $id,
             filename = 'f.pdf', sha256 = $s, purged = $p",
        )
        .bind(("id", id.to_string()))
        .bind(("s", sha.to_string()))
        .bind(("p", purged))
        .await
        .unwrap()
        .check()
        .unwrap();
    }

    /// Append a raw event, the way the server's log holds one.
    async fn seed_event(db: &Database, event_type: &str, payload: serde_json::Value) {
        db.query(
            "CREATE events CONTENT {
                event_type: $t, aggregate_id: 'a', timestamp: time::now(),
                device_id: 'test-device', payload: $p, received_at: time::now() }",
        )
        .bind(("t", event_type.to_string()))
        .bind(("p", payload))
        .await
        .unwrap()
        .check()
        .unwrap();
    }

    /// 🔴 The bug this function exists to prevent: deleting bytes a committed
    /// transaction still points at, leaving a ledger entry whose evidence is gone.
    ///
    /// ⛔ The fixture is the **server's** shape — `documents` present, no budget
    /// projection — because that is the only host that holds blobs and so the only
    /// one that can delete them. A fixture carrying `BudgetProjection` would pass
    /// while production counted nothing.
    #[tokio::test]
    async fn a_transaction_attachment_keeps_a_blob_alive_without_the_budget_table() {
        let db = doc_db().await;
        let sha = "c".repeat(64);
        seed_doc_with_hash(&db, "doc", &sha, true).await; // the only document, purged
        seed_event(
            &db,
            "transaction_recorded",
            serde_json::json!({
                "txn_id": "t1", "date": "2026-09-01", "description": "receipt", "postings": [],
                "attachment": { "sha256": sha, "filename": "r.pdf",
                                "mime_type": "application/pdf", "size": 10 },
            }),
        )
        .await;

        assert_eq!(
            blob_reference_count(&db, &sha).await.unwrap(),
            1,
            "every document is purged, so counting documents alone says 0"
        );
    }

    /// A merge writes the surviving attachment under a different key on a
    /// different event type, so counting `transaction_recorded` alone misses every
    /// reconciled receipt.
    #[tokio::test]
    async fn a_merged_transactions_attachment_counts_too() {
        let db = doc_db().await;
        let sha = "f".repeat(64);
        seed_event(
            &db,
            "transactions_merged",
            serde_json::json!({
                "primary_id": "t1", "merged_ids": ["t2"], "combined_postings": [],
                "combined_description": "receipt",
                "combined_attachment": { "sha256": sha, "filename": "r.pdf",
                                         "mime_type": "application/pdf", "size": 10 },
            }),
        )
        .await;

        assert_eq!(blob_reference_count(&db, &sha).await.unwrap(), 1);
    }

    /// Bytes shared by two documents survive purging one of them.
    #[tokio::test]
    async fn a_blob_two_documents_share_is_not_reclaimable_until_both_go() {
        let db = doc_db().await;
        let sha = "d".repeat(64);
        seed_doc_with_hash(&db, "emailed", &sha, false).await;
        seed_doc_with_hash(&db, "scanned", &sha, false).await;
        assert_eq!(blob_reference_count(&db, &sha).await.unwrap(), 2);

        seed_doc_with_hash(&db, "scanned", &sha, true).await;
        assert_eq!(
            blob_reference_count(&db, &sha).await.unwrap(),
            1,
            "the other filing still needs these bytes"
        );

        seed_doc_with_hash(&db, "emailed", &sha, true).await;
        assert_eq!(
            blob_reference_count(&db, &sha).await.unwrap(),
            0,
            "now, and only now, the bytes are reclaimable"
        );
    }

    /// The email view's second half: what arrived inside the message.
    #[tokio::test]
    async fn an_emails_children_are_the_documents_that_arrived_inside_it() {
        let db = doc_db().await;
        fold_part(&db, "mail", "statement.eml", "message/rfc822", None).await;
        fold_part(&db, "b-att", "b.pdf", "application/pdf", Some("mail")).await;
        fold_part(&db, "a-att", "a.pdf", "application/pdf", Some("mail")).await;
        // A document of its own, to prove the parent link selects the children
        // rather than the query simply returning everything else.
        fold_part(&db, "loose", "loose.pdf", "application/pdf", None).await;

        let kids = document_children(&db, "mail").await.unwrap();
        assert_eq!(kids.len(), 2, "{kids:?}");
        assert_eq!(
            kids.iter()
                .map(|k| k.document_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a-att", "b-att"],
            "⚠️ ordered by filename — every part is archived in one operation, \
             so archived_at orders them arbitrarily"
        );
        assert_eq!(kids[0].parent_document_id.as_deref(), Some("mail"));

        assert!(
            document_children(&db, "loose").await.unwrap().is_empty(),
            "a document with no children must come back empty, not with siblings"
        );
    }

    #[tokio::test]
    async fn a_document_carries_its_fields_and_their_provenance() {
        let db = doc_db().await;
        fold_doc(
            &db,
            "s1",
            "s1.csv",
            "",
            "2026-01-01T00:00:00Z",
            Some("brokerage_statement"),
        )
        .await;

        let doc = get_document(&db, "s1").await.unwrap().expect("exists");
        assert_eq!(doc.kind.as_deref(), Some("brokerage_statement"));
        assert_eq!(doc.filename.as_deref(), Some("s1.csv"));

        let fields = doc.fields.expect("fields folded onto the row");
        let balance = fields
            .iter()
            .find(|f| f.key == "closing_balance")
            .expect("closing_balance");
        assert!(balance.verified, "a walked balance chain is an oracle");
        assert!(balance.source.starts_with("parser:"));

        assert!(
            get_document(&db, "nope").await.unwrap().is_none(),
            "a missing document is None, not an error"
        );
    }

    /// The regression guard for the reconciliation-screen crash.
    ///
    /// The bug was a hand-written field list selecting a raw record id into
    /// `TransactionRow.id: String`. Asserting the id has **no** `transactions:`
    /// prefix is what pins the `meta::id(id)` projection in place — a plain
    /// "does it return a row" assertion would pass with the raw id and miss it.
    #[tokio::test]
    async fn list_unmatched_transactions_returns_bare_ids() {
        let db = txn_db().await;
        insert_txn(&db, "u1", "Unmatched").await;

        let rows = list_unmatched_transactions(&db).await.unwrap();

        assert_eq!(rows.len(), 1, "the Unmatched-leg txn should be returned");
        assert_eq!(rows[0].id, "u1", "id must be bare, not a record id");
        assert!(
            !rows[0].id.contains(':'),
            "id still carries a table prefix: {}",
            rows[0].id
        );
        // `created_at`/`updated_at` are `datetime` in the schema but `String` on
        // the row, so they need the `<string>` cast the constant carries. Without
        // it these fail to convert exactly like `id` did.
        assert!(
            rows[0].created_at.contains("2026-09-01"),
            "created_at did not round-trip as a string: {}",
            rows[0].created_at
        );
        assert!(rows[0].updated_at.contains("2026-09-01"));
    }

    /// A transaction with no `Unmatched` leg must not appear — otherwise the
    /// reconciliation screen would propose pairs for already-matched rows.
    #[tokio::test]
    async fn list_unmatched_transactions_skips_matched_rows() {
        let db = txn_db().await;
        insert_txn(&db, "matched", "Assets:Chequing").await;
        insert_txn(&db, "unmatched", "Unmatched").await;

        let rows = list_unmatched_transactions(&db).await.unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "unmatched");
    }

    /// The complement, and bounded: category suggestions read only settled rows.
    #[tokio::test]
    async fn list_settled_transactions_is_the_complement_and_honours_its_limit() {
        let db = txn_db().await;
        insert_txn(&db, "settled1", "Assets:Chequing").await;
        insert_txn(&db, "settled2", "Assets:Chequing").await;
        insert_txn(&db, "unmatched", "Unmatched").await;

        let rows = list_settled_transactions(&db, 10).await.unwrap();
        let mut ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["settled1", "settled2"]);
        assert_eq!(list_settled_transactions(&db, 1).await.unwrap().len(), 1);
    }

    /// The count is the list's length, and zero is zero rather than an error.
    #[tokio::test]
    async fn count_unmatched_transactions_counts_the_reconcile_queue() {
        let db = txn_db().await;
        assert_eq!(count_unmatched_transactions(&db).await.unwrap(), 0);
        insert_txn(&db, "settled", "Assets:Chequing").await;
        insert_txn(&db, "open1", "Unmatched").await;
        insert_txn(&db, "open2", "Unmatched").await;
        assert_eq!(count_unmatched_transactions(&db).await.unwrap(), 2);
    }

    // --- Feedback ---

    /// Temp DB holding real `feedback_captured` events, appended through the
    /// real `EventStore`.
    ///
    /// ⚠️ The defect these tests cover was a statement the **engine** refused,
    /// so nothing short of a live query could have found it: the write path was
    /// fine, the payloads were fine, and the endpoint answered `200` with an
    /// empty body for two months.
    async fn feedback_db(rows: &[(&str, &str, &str)]) -> Database {
        use crate::events::{EventStore, FeedbackCapturedPayload, NewEvent, SurrealEventStore};

        let db = crate::db::test_db().await;
        let store = SurrealEventStore::new(db.clone());

        for (id, body, ts) in rows {
            let payload = FeedbackCapturedPayload {
                feedback_id: (*id).to_string(),
                body: (*body).to_string(),
                ..Default::default()
            };
            let mut event = NewEvent::feedback_captured("dev-a", &payload).unwrap();
            event.timestamp = ts.parse::<chrono::DateTime<chrono::Utc>>().unwrap();
            store.append(event).await.unwrap();
        }
        db
    }

    fn ids(reports: &[FeedbackReport]) -> Vec<&str> {
        reports
            .iter()
            .map(|r| r.report.feedback_id.as_str())
            .collect()
    }

    /// ⚠️ **SurrealDB requires every `ORDER BY` idiom to appear in the
    /// selection**, and refuses the whole statement when it does not — an
    /// `Err`, never an empty list. `timestamp` is projected under the alias
    /// `ts`, so ordering must name `ts`.
    ///
    /// The sub-second row is the point of the third entry: ordering a *string*
    /// is only equivalent to ordering a datetime if the rendering is
    /// fixed-width, so a fractional second is where lexicographic ordering
    /// would diverge from chronological.
    #[tokio::test]
    async fn list_feedback_returns_reports_newest_first() {
        let db = feedback_db(&[
            ("fb-old", "oldest", "2026-09-01T10:00:00Z"),
            ("fb-new", "newest", "2026-09-03T10:00:00Z"),
            ("fb-mid", "middle", "2026-09-02T10:00:00Z"),
            (
                "fb-mid-frac",
                "middle, half a second later",
                "2026-09-02T10:00:00.5Z",
            ),
        ])
        .await;

        let (reports, skipped) = list_feedback(&db, None, 10).await.unwrap();

        assert_eq!(skipped, 0);
        assert_eq!(ids(&reports), ["fb-new", "fb-mid-frac", "fb-mid", "fb-old"]);
    }

    /// The `since` branch is a second statement string, so it is a second chance
    /// to get the ordering wrong — and it was broken identically.
    #[tokio::test]
    async fn list_feedback_since_excludes_older_and_keeps_the_order() {
        let db = feedback_db(&[
            ("fb-old", "oldest", "2026-09-01T10:00:00Z"),
            ("fb-new", "newest", "2026-09-03T10:00:00Z"),
            ("fb-mid", "middle", "2026-09-02T10:00:00Z"),
        ])
        .await;

        let (reports, _) = list_feedback(&db, Some("2026-09-01T10:00:00Z"), 10)
            .await
            .unwrap();

        // Strictly newer: the boundary row is excluded, not included.
        assert_eq!(ids(&reports), ["fb-new", "fb-mid"]);
    }

    /// `LIMIT` must cut the *oldest*, which is only true if the sort runs first.
    /// A statement that ordered after limiting would pass the ordering test
    /// above and still hand a puller an arbitrary slice.
    #[tokio::test]
    async fn list_feedback_limit_keeps_the_newest() {
        let db = feedback_db(&[
            ("fb-old", "oldest", "2026-09-01T10:00:00Z"),
            ("fb-new", "newest", "2026-09-03T10:00:00Z"),
            ("fb-mid", "middle", "2026-09-02T10:00:00Z"),
        ])
        .await;

        let (reports, _) = list_feedback(&db, None, 1).await.unwrap();

        assert_eq!(ids(&reports), ["fb-new"]);
    }
}
