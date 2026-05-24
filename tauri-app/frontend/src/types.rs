use serde::{Deserialize, Serialize};

/// A journal entry (one per day). Mirrors `JournalEntryRow` from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntryItem {
    /// The date this journal is keyed by (YYYY-MM-DD). Also the SurrealDB record id.
    pub id: String,
    pub journal_id: String,
    pub date: String,
    pub raw_text: String,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub closed: bool,
    pub complete: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A free-form (generic) note. Mirrors `GenericNoteRow` from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericNoteItem {
    pub id: String,
    pub title: String,
    pub raw_text: String,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine group. Mirrors `RoutineGroupRow` from the backend.
///
/// Phase 0 dropped `time_of_day` and introduced `order` + a `removed` flag
/// (soft-delete). The frontend filters removed groups out of the default list view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineGroup {
    pub id: String,
    pub name: String,
    pub frequency: String,
    #[serde(default)]
    pub order_num: i64,
    #[serde(default)]
    pub removed: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine item. Mirrors `RoutineItemRow` from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineItem {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub estimated_duration_min: i64,
    pub order_num: i64,
    #[serde(default)]
    pub removed: bool,
}

/// A routine completion entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionEntry {
    pub id: String,
    pub item_id: String,
    pub group_id: String,
    pub date: String,
    pub skipped: bool,
    pub reason: Option<String>,
}

/// Result of a manual sync operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncStatus {
    pub pulled: usize,
    pub pushed: usize,
}

/// Current sync configuration info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncInfo {
    pub server_url: String,
    pub device_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimezoneInfo {
    pub timezone: String,
    pub is_override: bool,
}

/// LLM processing result from the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmResult {
    pub tags: Vec<String>,
    pub tasks: Vec<TaskResult>,
    pub dates: Vec<DateResult>,
    pub expenses: Vec<ExpenseResult>,
    pub summary: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResult {
    pub description: String,
    pub priority: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateResult {
    pub date: String,
    pub context: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpenseResult {
    pub amount: f64,
    pub currency: String,
    pub description: String,
}

/// 4-state sync status reported by the background debouncer/retry loop.
/// Matches `SyncStatus` exposed by the Phase 2 `get_sync_status` Tauri command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SyncState {
    #[default]
    Idle,
    Syncing,
    Retrying,
    Error,
}

/// Mirrors `core::sync::SyncStatusSnapshot` — the full payload returned by
/// the `get_sync_status` Tauri command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatusSnapshot {
    pub status: SyncState,
    pub retry_attempt: u32,
    pub last_error: Option<String>,
}

impl Default for SyncStatusSnapshot {
    fn default() -> Self {
        Self {
            status: SyncState::Idle,
            retry_attempt: 0,
            last_error: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Obsidian import / export
// ---------------------------------------------------------------------------

/// Mirrors backend `commands::import::PreviewRow`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportPreviewRow {
    pub path: String,
    pub relative_path: String,
    /// One of `"journal"`, `"generic"`, `"error"`.
    pub kind: String,
    pub key: String,
    pub tags: Vec<String>,
    pub body_preview: String,
    pub body_len: usize,
    pub has_legacy_properties: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportPreviewSummary {
    pub root: String,
    pub rows: Vec<ImportPreviewRow>,
    pub journal_count: usize,
    pub generic_count: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportCommitSummary {
    pub journal_created: usize,
    pub generic_created: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedImportRow {
    pub path: String,
    pub kind: String,
    pub override_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSummary {
    pub target: String,
    pub journal_written: usize,
    pub generic_written: usize,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Capture / extraction (Phase 3.1+)
// ---------------------------------------------------------------------------

/// Single extracted posting line. Amount is wire-side string (server uses
/// `rust_decimal::serde::str`); frontend never does math on it — just display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedPostingView {
    #[serde(default)]
    pub account_hint: Option<String>,
    pub commodity: String,
    pub amount: String,
    #[serde(default)]
    pub line_label: Option<String>,
}

/// Content-addressable attachment metadata. Mirrors `core::events::AttachmentRef`.
/// Populated server-side by `/documents/extract?attach=true`; the UI threads it
/// through `TransactionForm` → `record_transaction` so it lives on the event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRef {
    pub sha256: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

/// Frontend view of `core::extraction::ExtractionResult` — fields normalised
/// to wire-friendly types (string amounts, ISO date strings) so the UI doesn't
/// pull in `rust_decimal` or `chrono`. Carries the server-minted
/// `AttachmentRef` when the request was made with `attach=true`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedDraft {
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub postings: Vec<ExtractedPostingView>,
    #[serde(default)]
    pub total: Option<String>,
    pub confidence: f64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub attachment: Option<AttachmentRef>,
}

/// Single posting line in a TransactionDraft submission. Mirrors the wire
/// shape of `core::events::Posting` after `DisplayFromStr` serialization:
/// `amount` is the decimal-as-string the backend's `serde_with` adapter
/// expects, and `tags` are flat strings (Tag::Bare / Tag::KeyValue both
/// roundtrip through `Display`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostingInput {
    pub account: String,
    pub commodity: String,
    pub amount: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Frontend → backend submission for `record_transaction` command. Matches
/// the JSON shape of `commands::budget::TransactionDraft`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionFormDraft {
    pub date: String,
    pub description: String,
    pub postings: Vec<PostingInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<AttachmentRef>,
}

// ---------------------------------------------------------------------------
// Auto-import observability (Phase 3.9) — mirrors server's SourceStatusView.
// ---------------------------------------------------------------------------

/// Wire shape returned by the server's `/auto_import/status` route. The
/// `health` string is one of "unknown" | "healthy" | "stale" | "degraded";
/// the UI maps it to a colored badge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoImportSourceView {
    pub name: String,
    #[serde(default)]
    pub last_tick_at: Option<String>,
    /// Last tick outcome — tagged enum on the wire:
    /// `{ "kind": "not_yet_run" }` |
    /// `{ "kind": "success", "events_appended": N }` |
    /// `{ "kind": "failure", "error": "..." }`.
    pub last_outcome: serde_json::Value,
    pub interval_secs: u64,
    pub health: String,
}

/// Captured payload from an Android share-target SEND intent (Phase 3.3).
/// `MainActivity.kt` writes bytes + meta to filesDir; `take_pending_share_intent`
/// reads + clears them and ships the pair up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingShareCapture {
    pub mime: String,
    pub filename: String,
    pub size: u64,
    pub bytes: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Auto-import batch review (Phase 3.10.6)
// ---------------------------------------------------------------------------

/// Frontend view of one draft transaction inside a pending batch. Mirrors
/// `core::events::DraftTransaction` after JSON serialisation — dates as
/// `YYYY-MM-DD` strings, postings as the same `PostingInput` shape used by
/// the manual capture form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftTransactionView {
    pub external_id: String,
    pub date: String,
    pub description: String,
    pub postings: Vec<PostingInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingBatchView {
    pub batch_id: String,
    pub source: String,
    pub dedup_key: String,
    pub fetched_at: String,
    pub draft_postings: Vec<DraftTransactionView>,
    #[serde(default)]
    pub source_metadata: Option<serde_json::Value>,
}

/// Frontend mirror of `core::db::queries::TxnFilter`. All fields optional;
/// blank strings get normalized to None by the backend before the WHERE
/// clause builds, so empty inputs don't need pre-trimming here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TxnFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

impl TxnFilter {
    /// True when no axis is set — used to skip the "clear filters" UI and
    /// to suppress the "filtered" badge on the list header.
    pub fn is_empty(&self) -> bool {
        self.date_from.is_none()
            && self.date_to.is_none()
            && self.account.is_none()
            && self.tag.is_none()
            && self.category.is_none()
    }
}

/// Frontend mirror of `commands::budget::TransactionView`. The `postings` and
/// `attachment` fields land as `serde_json::Value` because the backend stores
/// them as SurrealDB FLEXIBLE objects and routes through `into_json_value()` —
/// see the doc-comment in `commands::budget`. List views project a friendlier
/// shape inline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionView {
    pub id: String,
    pub date: String,
    pub description: String,
    pub postings: serde_json::Value,
    #[serde(default)]
    pub attachment: Option<serde_json::Value>,
    pub category: Option<String>,
    #[serde(default)]
    pub tags_top: Vec<String>,
    pub cleared: bool,
    pub statement_source: Option<String>,
    pub cleared_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitBatchResult {
    pub events_appended: usize,
    pub txns_recorded: usize,
    pub fx_recorded: bool,
}

/// One commodity holding on an account. Mirrors
/// `core::balances::CommodityBalance`. Quantity arrives as a string so the
/// frontend doesn't need a Decimal dep — display-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommodityBalanceView {
    pub commodity: String,
    pub quantity: String,
    pub value_in_base: Option<String>,
}

/// Per-account summary for the Accounts screen (Phase 4.4). Mirrors
/// `core::balances::AccountSummary`. `total_in_base` and `value_in_base` are
/// `None` when no FX rate is available for a commodity in the journal — the
/// UI renders an em-dash in that case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountSummaryView {
    pub account: String,
    pub display_name: Option<String>,
    pub last_reconciled_through: Option<String>,
    pub last_statement_balance: Option<String>,
    pub balances: Vec<CommodityBalanceView>,
    pub total_in_base: Option<String>,
}

/// One month's income / spending bucket. Mirrors
/// `core::dashboard::MonthlyTrendBucket` with Decimals stringified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonthlyTrendBucketView {
    pub month: String,
    pub income: String,
    pub spending: String,
}

/// One confirmed recurring obligation. Mirrors
/// `core::dashboard::RecurringObligation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecurringObligationView {
    pub vendor: String,
    pub amount: String,
    pub commodity: String,
    pub cadence_days: u32,
}

/// R1 dashboard payload (Phase 4.5 + 4.6). Mirrors
/// `core::dashboard::DashboardSummary`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardSummaryView {
    pub base_currency: String,
    pub net_worth_in_base: Option<String>,
    pub unmatched_balance: Option<String>,
    pub monthly_buckets: Vec<MonthlyTrendBucketView>,
    pub recurring: Vec<RecurringObligationView>,
}

/// Verdict for a "Can I afford X?" query. Mirrors
/// `core::dashboard::AffordVerdict` plus the base currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AffordVerdictView {
    pub can_afford: bool,
    pub remaining_in_base: String,
    pub base_currency: String,
    pub policy_label: String,
}

/// One row from the `budgets` projection. Mirrors `core::db::queries::BudgetRow`
/// (Phase 5.1). `id` is the category path; `amount` is the per-period target
/// as a decimal string; `period` is one of `"weekly"` / `"biweekly"` /
/// `"monthly"` / `"custom:N"` and parses through `core::budget::period_to_days`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetRow {
    pub id: String,
    pub amount: String,
    pub period: String,
    pub removed: bool,
}

/// Actual-vs-planned snapshot for one budget over its current period window
/// (Phase 5.2). Mirrors `commands::budget::BudgetProgressView`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetProgress {
    pub category: String,
    pub period: String,
    pub period_start: String,
    pub period_end: String,
    pub target: String,
    pub actual: String,
    pub percent_used: f64,
    pub over_budget: bool,
}
