//! Wire types for the Tauri IPC boundary.
//!
//! Every struct here hand-mirrors one the backend owns, and that IS the design
//! — reviewed and kept, not an omission waiting to be generated away. The
//! frontend compiles to wasm and deliberately does not depend on `omni-me-core`
//! (pulling it in drags a database driver and its transitive dependencies into
//! the browser bundle), so there is no shared crate for these to come from.
//!
//! What makes it safe is that the mirroring is enforced at the only place it
//! can go wrong: `serde` deserialisation of the actual IPC payload. A renamed
//! or retyped field fails at runtime on the first call, in development, on the
//! screen that uses it. A review of all of these against their backend
//! counterparts found no drift.
//!
//! Trip-wire: if drift ever DOES ship to a user, stop hand-mirroring and
//! generate this file from the backend types instead. Until then, generating it
//! would add a build step to solve a problem that has not occurred.

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

/// One day's calendar-widget stats. Mirrors `JournalDayStat` from the backend.
///
/// `raw_text` arrives unprocessed on purpose — the word count is derived here,
/// with the same token-stripping reader the editor footer uses, so the two can
/// never disagree. See the backend type for why it is not counted there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalDayStat {
    pub date: String,
    pub complete: bool,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub raw_text: String,
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
    /// Whether this device has a bearer token for the box. The token itself is
    /// never returned — Settings renders state, it does not re-display secrets
    /// (same rule as the LLM key's `has_key`).
    #[serde(default)]
    pub has_server_token: bool,
}

/// Which data this run is pointed at. Drives the non-production banner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeProfile {
    /// True when the app-data root was overridden via `OMNI_DATA_DIR`, i.e. this
    /// build is deliberately NOT on real data.
    #[serde(default)]
    pub non_production: bool,
    pub data_dir: String,
    pub server_url: String,
}

// ── Configuration ───────────────────────────────────────────────────────────
//
// Mirrors of `omni_me_core::config` and `commands::config::ConfigEntryView`.
// This crate is its own wasm workspace and does not depend on `omni-me-core`, so
// the shapes are restated here — the same arrangement `SyncInfo` and
// `RuntimeProfile` already live under. Serde is what keeps them honest: a field
// renamed on one side fails to decode on the other.

/// A configured value. Adjacently tagged, matching core's encoding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ConfigValue {
    Bool(bool),
    Text(String),
    Int(i64),
}

impl ConfigValue {
    /// How the value reads in a settings row.
    pub fn display(&self) -> String {
        match self {
            ConfigValue::Bool(true) => "On".to_string(),
            ConfigValue::Bool(false) => "Off".to_string(),
            ConfigValue::Int(n) => n.to_string(),
            ConfigValue::Text(t) => {
                let mut chars = t.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        }
    }
}

/// Which layer supplied the value in effect.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigLayer {
    Device,
    Global,
    Default,
}

/// Which settings section a key renders under.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigGroup {
    Features,
    Appearance,
    /// Retrieval settings the headless agent reads. Rendered here because this is
    /// the only screen there is — the agent runs on a box and has none.
    ///
    /// ⚠️ **Mirrors `core::config::ConfigGroup` and must stay exhaustive.** The
    /// backend sends this as a field of every `ConfigEntry`; a variant missing
    /// here fails deserialization of the *whole* config list, so one unrecognised
    /// group empties the settings screen rather than hiding one section.
    Assistant,
}

/// A feature that can be switched off whole. Mirror of `core::config::Feature`.
///
/// Identified by its config key's wire name rather than by a serde tag: what the
/// frontend receives is a `Vec<ConfigEntry>`, and the key string is what links a
/// row to the feature it switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Feature {
    Journal,
    Notes,
    Routines,
    Finances,
    AutoImport,
    Llm,
}

pub const ALL_FEATURES: &[Feature] = &[
    Feature::Journal,
    Feature::Notes,
    Feature::Routines,
    Feature::Finances,
    Feature::AutoImport,
    Feature::Llm,
];

impl Feature {
    pub fn key(self) -> &'static str {
        match self {
            Feature::Journal => "feature.journal",
            Feature::Notes => "feature.notes",
            Feature::Routines => "feature.routines",
            Feature::Finances => "feature.finances",
            Feature::AutoImport => "feature.auto_import",
            Feature::Llm => "feature.llm",
        }
    }
}

/// The features that are on, read once at boot.
///
/// A set rather than a map because "absent" and "off" are the same thing to every
/// caller, and because the empty set is the wrong default — see
/// [`Features::from_entries`] for why an unresolved read reports everything on.
#[derive(Debug, Clone, PartialEq)]
pub struct Features(std::collections::BTreeSet<Feature>);

impl Default for Features {
    /// Everything on, matching core's per-key defaults.
    fn default() -> Self {
        Features(ALL_FEATURES.iter().copied().collect())
    }
}

impl Features {
    /// Read the feature set out of a fetched config list.
    ///
    /// A key the list does not mention, or whose value is not a boolean, counts
    /// as **on** — the same direction `ResolvedConfig::bool_of` falls back in. A
    /// missing key must never hide a tab: an empty or failed read would blank the
    /// whole app, which looks like data loss rather than a preference.
    pub fn from_entries(entries: &[ConfigEntry]) -> Self {
        Features(
            ALL_FEATURES
                .iter()
                .copied()
                .filter(|f| {
                    entries
                        .iter()
                        .find(|e| e.key == f.key())
                        .and_then(|e| match e.effective {
                            ConfigValue::Bool(b) => Some(b),
                            _ => None,
                        })
                        .unwrap_or(true)
                })
                .collect(),
        )
    }

    pub fn on(&self, feature: Feature) -> bool {
        self.0.contains(&feature)
    }

    pub fn any(&self, features: &[Feature]) -> bool {
        features.iter().any(|f| self.on(*f))
    }
}

/// One configurable key, with both layers exposed so the row can say which one
/// is winning rather than only showing the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub label: String,
    pub group: ConfigGroup,
    pub effective: ConfigValue,
    pub layer: ConfigLayer,
    pub global: Option<ConfigValue>,
    pub device: Option<ConfigValue>,
    pub default: ConfigValue,
    pub applies_immediately: bool,
    pub choices: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Record types — the declared shape of a record
// ---------------------------------------------------------------------------
//
// Mirrors `core::record_type`. The frontend has no `core` dependency, so the
// shape is restated here; the `#[serde(default)]` attributes must match the
// backend's, or a declaration written by an older build stops decoding.

/// What makes one record of this type distinct from another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Identity {
    Date,
}

/// How a property's value is entered and stored. Only prose today; the field
/// exists so adding a kind later is a value change rather than a payload
/// migration on an append-only log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    #[default]
    Text,
}

/// One declared property: the frontmatter key, and how to draw it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyDecl {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub kind: PropertyKind,
    #[serde(default)]
    pub required: bool,
}

/// A record type as declared, in declaration order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordType {
    pub name: String,
    pub identity: Identity,
    #[serde(default)]
    pub auto_close: bool,
    #[serde(default)]
    pub properties: Vec<PropertyDecl>,
}

impl RecordType {
    /// Every declared key, in declaration order. This is the set
    /// `split_journal` lifts out of frontmatter; anything else stays in
    /// `legacy_raw` rather than being dropped.
    pub fn property_keys(&self) -> Vec<&str> {
        self.properties.iter().map(|p| p.key.as_str()).collect()
    }
}

/// Build + installation identity, shown in the capture modal so a user can see
/// what a problem report will carry before sending it. Distinct from
/// [`RuntimeProfile`], which is on the render path of a banner drawn on every
/// screen and deliberately carries nothing it does not need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppContext {
    pub app_version: String,
    pub platform: String,
    pub device_id: String,
    pub server_url: String,
    #[serde(default)]
    pub non_production: bool,
    /// Which app-data root this run is on. Shown in the modal only when
    /// `non_production` is set — on a real run the path is noise.
    #[serde(default)]
    pub data_dir: String,
}

/// Dry run of an Obsidian export — what it would overwrite, before it does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportPreview {
    pub target: String,
    pub would_overwrite: Vec<String>,
    pub overwrite_count: usize,
    pub total_files: usize,
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
    /// `{ "kind": "success", "summary": { "fetched": N, "appended": N, … } }` |
    /// `{ "kind": "failure", "error": "..." }`.
    pub last_outcome: serde_json::Value,
    pub interval_secs: u64,
    pub health: String,
    /// Authentication state — tagged enum on the wire:
    /// `{ "kind": "active" }` | `{ "kind": "needs_reauth", "reason": "..." }`.
    /// Drives the inline "Reconnect" affordance. `#[serde(default)]` (→ `Null`)
    /// keeps pre-Step-2b mocks/servers parseable; the UI treats `Null`/missing
    /// `kind` as active.
    #[serde(default)]
    pub auth_state: serde_json::Value,
    /// Whether the source supports interactive re-auth (only the WS subprocess
    /// source does today). Gates whether the Reconnect button/OTP field render.
    #[serde(default)]
    pub reauth_capable: bool,
    /// Whether the user has paused this source (runtime off-switch, #367). When
    /// true the row shows a "Paused" state + a Resume action instead of Pause;
    /// the source keeps its config but does not auto-poll. `#[serde(default)]`
    /// keeps older-server snapshots parseable (→ not paused).
    #[serde(default)]
    pub paused: bool,
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
    pub balances: Vec<CommodityBalanceView>,
    pub total_in_base: Option<String>,
}

/// One tag-value slice of the Accounts drill-down (per institution / product).
/// Mirrors `commands::budget::AccountTagGroupView`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountTagGroupView {
    pub value: String,
    pub balances: Vec<CommodityBalanceView>,
    pub total_in_base: Option<String>,
}

/// Full per-account tag breakdown returned by `account_tag_breakdown`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountTagBreakdownView {
    pub account: String,
    pub group_by: String,
    pub groups: Vec<AccountTagGroupView>,
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
    pub pattern_id: String,
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

/// One net-worth-history point. Mirrors `commands::budget::NetWorthPointView`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetWorthPointView {
    /// `YYYY-MM-DD` boundary date.
    pub date: String,
    /// Net worth in base currency at that boundary; `None` when nothing was
    /// convertible yet.
    pub net_worth_in_base: Option<String>,
}

/// Net-worth-over-time series for the Overview hero chart. Mirrors
/// `commands::budget::NetWorthSeriesView`. The last point equals the live
/// net-worth number (same journal + roster/Unmatched policy).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetWorthSeriesView {
    pub base_currency: String,
    /// Echoed range key (`1m`/`3m`/`6m`/`1y`/`ytd`/`all`).
    pub range: String,
    pub points: Vec<NetWorthPointView>,
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

/// One row from the `recurring_patterns` projection, parsed into a clean
/// shape (Phase 5.4). Mirrors `commands::budget::RecurringPatternView`.
/// `status` is one of `"detected"` / `"confirmed"` / `"dismissed"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecurringPattern {
    pub pattern_id: String,
    pub status: String,
    pub vendor: String,
    pub amount: String,
    pub commodity: String,
    pub cadence_days: u32,
    pub occurrences: u32,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
}

/// Result of a recurring-pattern scan. Mirrors
/// `commands::budget::ScanRecurringResult`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRecurringResult {
    pub detected: usize,
    pub new_emitted: usize,
    pub already_tracked: usize,
}

/// Result of a Summit chequing CSV import. Mirrors
/// One line the parser refused to read. Mirrors
/// `commands::budget::SkippedLineView`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkippedLineView {
    pub line_no: usize,
    pub raw: String,
    pub reason: String,
}

/// `commands::budget::ImportStatementResult` — one shape for every statement
/// format, CSV export or rendered PDF alike.
///
/// ⚠️ **Empty `blockers` does not mean verified.** A format carrying no balance
/// and declaring no totals has nothing to fail, so it clears the gate by
/// offering no gate — `verifiability` carries that distinction in words, and
/// the UI must show it rather than inferring a clean bill from an empty list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportStatementResult {
    pub imported: usize,
    /// True when nothing was written because the statement did not check out.
    /// `imported: 0` alone is ambiguous — an empty statement produces it too.
    pub refused: bool,
    pub skipped_zero_rows: usize,
    pub skipped: Vec<SkippedLineView>,
    pub structural: usize,
    pub rows_parsed: usize,
    pub closing_balance: Option<String>,
    pub blockers: Vec<String>,
    pub verifiability: String,
}

/// Compact preview for one side of a reconciliation pair (Phase 5.7).
/// Mirrors `commands::budget::ReconciliationTxnPreview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationTxnPreview {
    pub txn_id: String,
    pub date: String,
    pub description: String,
    pub unmatched_amount: String,
    pub unmatched_commodity: String,
    pub statement_source: Option<String>,
}

/// One reconciliation candidate pair with inline previews. Mirrors
/// `commands::budget::MatchCandidateView`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchCandidateView {
    pub primary_id: String,
    pub secondary_id: String,
    pub score: f64,
    pub days_apart: u32,
    pub description_similarity: f64,
    pub clears_statement: bool,
    pub primary: ReconciliationTxnPreview,
    pub secondary: ReconciliationTxnPreview,
}

/// Result of a balance check (Phase 5.8). Mirrors
/// `commands::budget::BalanceCheckView`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceCheckView {
    pub account: String,
    pub commodity: String,
    pub cleared_total: String,
    pub statement_balance: String,
    pub discrepancy: String,
    pub ok: bool,
}

// ---------------------------------------------------------------------------
// Phase 6.2 / 6.3 — hledger journal import.
//
// Wire-shape mirrors of the Tauri command DTOs in
// `tauri-app/src-tauri/src/commands/journal_import.rs`. Kept here so views
// can compose them without crate-crossing imports.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalImportAccountStats {
    pub account: String,
    pub transaction_count: usize,
    pub posting_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalImportPosting {
    pub account: String,
    pub commodity: String,
    pub amount: String,
    pub fx_quote: Option<String>,
    pub fx_rate: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalImportSampleTxn {
    pub source_index: usize,
    pub txn_id: String,
    pub date: String,
    pub description: String,
    pub postings: Vec<JournalImportPosting>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalImportParseError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalImportPreview {
    pub root: String,
    pub files_parsed: usize,
    pub total_bytes: usize,
    pub transactions_count: usize,
    pub per_account: Vec<JournalImportAccountStats>,
    pub commodities: Vec<String>,
    pub sample_transactions: Vec<JournalImportSampleTxn>,
    pub parse_errors: Vec<JournalImportParseError>,
    pub balance_failures: Vec<String>,
    pub already_imported_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalImportPlan {
    pub accounts_to_drop: Vec<String>,
    pub account_renames: std::collections::HashMap<String, String>,
    pub apply_a2_rewriter: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalImportResult {
    pub committed_count: usize,
    pub skipped_existing_count: usize,
    pub dropped_count: usize,
    pub balance_failures: Vec<String>,
    pub parse_errors: Vec<JournalImportParseError>,
    pub a2_rewrites: usize,
}

// --- Assistant (Phase D-1) ---

/// A conversation in the thread list.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantThread {
    pub thread_id: String,
    /// Taken from the thread's opening question. `None` only while that question
    /// has not synced to this device yet.
    pub title: Option<String>,
    pub created_at: String,
    pub last_message_at: String,
    pub message_count: i64,
}

/// A record an answer actually opened, for the citation links under it.
///
/// ⚠️ **`title` is always `None` off the wire** — the event deliberately does not
/// freeze one, so a renamed record still cites correctly. Resolve the display
/// name locally; see `docs`/`assistant::answer::records_read`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecordCitation {
    /// `journal`, `note`, `routine`.
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// What an answer cost and how it ended.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct AssistantUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    /// Billed and rate-limited separately, and were ~95% of output on the
    /// baseline model — folded into a total it would misstate both cost and
    /// where the time went.
    #[serde(default)]
    pub reasoning_tokens: u32,
}

/// One turn of a conversation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantMessage {
    pub message_id: String,
    pub thread_id: String,
    /// `user` or `assistant`.
    pub role: String,
    /// `None` on an answer that failed, exhausted its turns, or was skipped as
    /// stale — `stopped` says which, and `detail` may carry the reason.
    pub text: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub in_reply_to: Option<String>,
    /// `answered` | `turn_budget` | `failed` | `stale`.
    #[serde(default)]
    pub stopped: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub elapsed_ms: Option<i64>,
    #[serde(default)]
    pub verbs: Option<Vec<String>>,
    #[serde(default)]
    pub records_read: Option<Vec<RecordCitation>>,
    #[serde(default)]
    pub usage: Option<AssistantUsage>,
    /// True when the agent raised this question on the user's behalf rather than
    /// the user typing it.
    ///
    /// ⚠️ Worth showing. An answer to something you never asked, rendered
    /// identically to one you did, is the difference between a check-in and the
    /// assistant appearing to have opinions out of nowhere.
    #[serde(default)]
    pub scheduled: Option<bool>,
}

impl AssistantMessage {
    pub fn is_user(&self) -> bool {
        self.role == "user"
    }

    /// Whether the assistant raised this itself, on a schedule.
    pub fn is_scheduled(&self) -> bool {
        self.scheduled.unwrap_or(false)
    }

    /// The sentence to show in place of an answer that never arrived.
    ///
    /// Returns `None` for a normal answer. Every other ending gets prose rather
    /// than a status code, because `turn_budget` on screen tells a person
    /// nothing about what to do next.
    pub fn failure_note(&self) -> Option<String> {
        match self.stopped.as_deref() {
            None | Some("answered") => None,
            Some("turn_budget") => Some(
                "The assistant ran out of steps before it finished. Try asking for one thing at a time."
                    .into(),
            ),
            Some("stale") => Some(
                "This question sat unanswered too long, so the assistant skipped it. Ask again to retry."
                    .into(),
            ),
            Some("failed") => Some(match self.detail.as_deref() {
                Some(why) if !why.trim().is_empty() => format!("The assistant could not answer: {why}"),
                _ => "The assistant could not answer this one.".into(),
            }),
            // An ending this build does not know about. Say so plainly rather
            // than rendering a blank bubble — a new `stopped` variant shipping
            // from another device must not read as the app hanging.
            Some(other) => Some(format!("The assistant stopped early ({other}).")),
        }
    }
}

/// One conversation, with whether it is still waiting on an answer.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantThreadView {
    pub messages: Vec<AssistantMessage>,
    /// The `message_id` of the question still waiting, if any. Computed in
    /// `core` so this screen and the agent's sweep cannot disagree about it.
    #[serde(default)]
    pub pending: Option<String>,
    #[serde(default)]
    pub pending_since: Option<String>,
}

/// What `ask_assistant` hands back.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantAsked {
    pub thread_id: String,
    pub message_id: String,
}

/// Something the assistant has offered to do, waiting on the user.
///
/// ⚠️ Nothing has happened when one of these exists. The assistant has no write
/// path — accepting it here is what authors the change.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantProposal {
    pub proposal_id: String,
    pub thread_id: String,
    pub message_id: String,
    /// `{type}.{operation}`, e.g. `note.create`.
    pub action: String,
    pub args: serde_json::Value,
    /// The assistant's one-sentence reason, written to stand alone — the inbox
    /// shows it without the conversation around it.
    pub rationale: String,
    /// What the action declared **when it was proposed**, not now.
    pub reversible: bool,
    pub created_at: String,
    /// `None` while waiting. `approved` | `rejected`, or a value a newer build
    /// wrote — any value at all means it is no longer in the inbox.
    #[serde(default)]
    pub decision: Option<String>,
}

impl AssistantProposal {
    pub fn is_pending(&self) -> bool {
        self.decision.is_none()
    }

    /// What accepting this would do, in the user's words rather than the
    /// action's name.
    ///
    /// ⚠️ Falls back to the raw action name for anything this build does not
    /// recognise. A proposal made by a newer build must render as *something* a
    /// person can decide on — a blank card is the one outcome that makes the
    /// inbox unusable.
    pub fn summary(&self) -> String {
        let arg = |key: &str| self.args.get(key).and_then(|v| v.as_str());
        match self.action.as_str() {
            "note.create" => match arg("title") {
                Some(title) => format!("Create a note: {title}"),
                None => "Create a note".to_string(),
            },
            "belief.record" => "Remember something about you".to_string(),
            "belief.supersede" => "Retire something it believed".to_string(),
            "routine.create" => match (arg("name"), arg("frequency")) {
                (Some(name), Some(freq)) => format!("Create a {} routine: {name}", readable(freq)),
                (Some(name), None) => format!("Create a routine: {name}"),
                _ => "Create a routine".to_string(),
            },
            "routine.add_item" => match arg("name") {
                Some(name) => format!("Add to a routine: {name}"),
                None => "Add something to a routine".to_string(),
            },
            "routine.modify_item" => "Change an item in a routine".to_string(),
            other => other.to_string(),
        }
    }

    /// What the assistant is proposing, in enough detail to decide on.
    ///
    /// ⚠️ Falls back to the raw arguments for an action this build does not know,
    /// rather than showing nothing. A proposal from a newer build must still be
    /// decidable — an empty card is the one outcome that makes the inbox useless.
    pub fn detail(&self) -> Option<String> {
        let arg = |key: &str| self.args.get(key).and_then(|v| v.as_str());
        match self.action.as_str() {
            "note.create" => return arg("body").map(str::to_string),
            "belief.record" => {
                let statement = arg("statement")?;
                return Some(match arg("confidence") {
                    Some(c) => format!("{statement}\n\nHow sure it is: {c}"),
                    None => statement.to_string(),
                });
            }
            "belief.supersede" => return arg("reason").map(str::to_string),
            "routine.add_item" => {
                return arg("estimated_duration_min").map(|m| format!("About {m} minutes"));
            }
            "routine.modify_item" => {
                let mut parts = Vec::new();
                if let Some(name) = arg("name") {
                    parts.push(format!("New wording: {name}"));
                }
                if let Some(minutes) = arg("estimated_duration_min") {
                    parts.push(format!("New estimate: {minutes} minutes"));
                }
                return (!parts.is_empty()).then(|| parts.join("\n"));
            }
            "routine.create" => return None,
            _ => {}
        }
        // An action this build does not know: show its arguments verbatim so the
        // card is still something a person can weigh up.
        let obj = self.args.as_object()?;
        let rendered: Vec<String> = obj
            .iter()
            .map(|(key, value)| match value.as_str() {
                Some(text) => format!("{key}: {text}"),
                None => format!("{key}: {value}"),
            })
            .collect();
        (!rendered.is_empty()).then(|| rendered.join("\n"))
    }
}

/// A routine frequency as a person says it.
///
/// ⚠️ `custom:N` is the wire form and must never reach the screen as-is — it is
/// the app's encoding, not language. An unrecognised value is shown verbatim
/// rather than guessed at.
fn readable(frequency: &str) -> String {
    match frequency {
        "daily" => "daily".to_string(),
        "weekly" => "weekly".to_string(),
        "biweekly" => "fortnightly".to_string(),
        "monthly" => "monthly".to_string(),
        other => match other.strip_prefix("custom:") {
            Some(n) => format!("every {n} days"),
            None => other.to_string(),
        },
    }
}

/// One record a belief was drawn from.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EvidenceRef {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// A lasting conclusion the assistant drew and the user accepted.
///
/// ⚠️ Nothing here was written by the assistant alone — every belief went through
/// the approval gate, which is what makes the set auditable rather than merely
/// visible.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Belief {
    pub belief_id: String,
    pub statement: String,
    /// `low` | `medium` | `high`, or anything a newer build recorded.
    pub confidence: String,
    pub recorded_at: String,
    #[serde(default)]
    pub review_after: Option<String>,
    /// The records the assistant had actually opened. May be empty — an
    /// unsupported conclusion is shown as unsupported rather than hidden.
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    /// `None` while it still holds.
    #[serde(default)]
    pub superseded_at: Option<String>,
    #[serde(default)]
    pub superseded_reason: Option<String>,
}

impl Belief {
    pub fn is_live(&self) -> bool {
        self.superseded_at.is_none()
    }

    /// How well supported it is, in words a person can act on.
    ///
    /// ⚠️ Falls through to the raw value rather than guessing. A confidence this
    /// build does not know was recorded by a newer one, and showing it verbatim
    /// is honest where silently calling it "low" would be a lie about the record.
    pub fn confidence_label(&self) -> String {
        match self.confidence.as_str() {
            "high" => "Well supported".to_string(),
            "medium" => "Some support".to_string(),
            "low" => "Thin evidence".to_string(),
            other => other.to_string(),
        }
    }
}

/// How one action has fared at the approval gate, and whether it is granted.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ActionRecord {
    pub action: String,
    pub approved: u32,
    pub rejected: u32,
    /// Still waiting. Not evidence either way.
    pub pending: u32,
    /// Whether the assistant may currently do this without asking.
    pub granted: bool,
    /// Whether it can be undone. ⛔ An irreversible action can never be granted.
    pub reversible: bool,
}

impl ActionRecord {
    pub fn decided(&self) -> u32 {
        self.approved + self.rejected
    }

    /// Share of decided proposals that were approved, or `None` when none have
    /// been decided.
    ///
    /// ⚠️ `None` rather than zero: "never used" and "always rejected" are
    /// opposite facts, and one number cannot say both.
    pub fn approval_rate(&self) -> Option<f64> {
        let decided = self.decided();
        (decided > 0).then(|| f64::from(self.approved) / f64::from(decided))
    }

    /// The evidence, as a sentence rather than a bare ratio.
    pub fn evidence_label(&self) -> String {
        match self.approval_rate() {
            None => "Never proposed yet".to_string(),
            Some(rate) => format!(
                "You accepted {} of {} ({:.0}%)",
                self.approved,
                self.decided(),
                rate * 100.0
            ),
        }
    }

    /// Whether it makes sense to offer the toggle at all.
    pub fn can_be_granted(&self) -> bool {
        self.reversible
    }
}

#[cfg(test)]
mod action_record_tests {
    use super::*;

    fn record(approved: u32, rejected: u32, reversible: bool) -> ActionRecord {
        ActionRecord {
            action: "note.create".into(),
            approved,
            rejected,
            pending: 0,
            granted: false,
            reversible,
        }
    }

    #[test]
    fn an_unused_action_says_so_rather_than_showing_zero_percent() {
        assert_eq!(record(0, 0, true).evidence_label(), "Never proposed yet");
        assert_eq!(record(0, 0, true).approval_rate(), None);
    }

    #[test]
    fn the_evidence_reads_as_a_count_and_a_share() {
        assert_eq!(
            record(3, 1, true).evidence_label(),
            "You accepted 3 of 4 (75%)"
        );
    }

    /// A pending proposal must not move the number — it is not evidence yet.
    #[test]
    fn pending_proposals_do_not_count() {
        let mut r = record(2, 0, true);
        r.pending = 5;
        assert_eq!(r.decided(), 2);
        assert_eq!(r.evidence_label(), "You accepted 2 of 2 (100%)");
    }

    /// ⚠️ `custom:N` is the wire encoding, not language. It must never reach a
    /// card as-is.
    #[test]
    fn a_custom_frequency_reads_as_words_not_as_its_encoding() {
        assert_eq!(readable("custom:3"), "every 3 days");
        assert_eq!(readable("biweekly"), "fortnightly");
        assert_eq!(readable("daily"), "daily");
        // Unrecognised: shown verbatim rather than guessed at.
        assert_eq!(readable("whenever"), "whenever");
    }

    /// ⛔ The published rule, at the surface that offers the choice.
    #[test]
    fn an_irreversible_action_is_not_grantable() {
        assert!(!record(9, 0, false).can_be_granted());
        assert!(record(0, 9, true).can_be_granted());
    }
}

#[cfg(test)]
mod belief_tests {
    use super::*;

    fn belief(confidence: &str) -> Belief {
        Belief {
            belief_id: "b1".into(),
            statement: "You underestimate admin tasks.".into(),
            confidence: confidence.into(),
            recorded_at: "2026-09-10T00:00:00Z".into(),
            review_after: None,
            evidence: Vec::new(),
            superseded_at: None,
            superseded_reason: None,
        }
    }

    #[test]
    fn the_three_levels_read_as_prose() {
        assert_eq!(belief("high").confidence_label(), "Well supported");
        assert_eq!(belief("medium").confidence_label(), "Some support");
        assert_eq!(belief("low").confidence_label(), "Thin evidence");
    }

    /// A value from a newer build must show as itself, not be flattened into one
    /// of ours — that would misreport what is actually in the log.
    #[test]
    fn an_unknown_confidence_is_shown_verbatim() {
        assert_eq!(belief("certain").confidence_label(), "certain");
    }

    #[test]
    fn any_supersession_timestamp_retires_it() {
        let mut b = belief("low");
        assert!(b.is_live());
        b.superseded_at = Some("2026-09-11T00:00:00Z".into());
        assert!(!b.is_live());
    }
}

#[cfg(test)]
mod proposal_tests {
    use super::*;

    fn proposal(action: &str, args: serde_json::Value) -> AssistantProposal {
        AssistantProposal {
            proposal_id: "p1".into(),
            thread_id: "t1".into(),
            message_id: "m1".into(),
            action: action.into(),
            args,
            rationale: "because".into(),
            reversible: true,
            created_at: "2026-09-10T00:00:00Z".into(),
            decision: None,
        }
    }

    #[test]
    fn a_note_proposal_reads_as_what_it_would_do() {
        let p = proposal(
            "note.create",
            serde_json::json!({ "title": "Renew passport", "body": "Expires May." }),
        );
        assert_eq!(p.summary(), "Create a note: Renew passport");
        assert_eq!(p.detail().as_deref(), Some("Expires May."));
    }

    /// ⚠️ A proposal from a newer build must still render something decidable.
    /// A blank card is the one outcome that makes the inbox unusable — the user
    /// cannot accept or decline what they cannot read.
    #[test]
    fn an_unknown_action_still_renders_a_decidable_card() {
        let p = proposal("routine.complete", serde_json::json!({ "item": "x" }));
        assert_eq!(p.summary(), "routine.complete");
        // Its arguments, verbatim. This test previously asserted `None` here,
        // which contradicted its own premise: a card showing only an action name
        // is not something a person can weigh up.
        assert_eq!(p.detail().as_deref(), Some("item: x"));
    }

    /// A note proposal missing its title must not render an empty heading.
    #[test]
    fn a_note_proposal_without_a_title_still_says_what_it_is() {
        let p = proposal("note.create", serde_json::json!({ "body": "b" }));
        assert_eq!(p.summary(), "Create a note");
    }

    #[test]
    fn a_routine_proposal_reads_as_a_sentence() {
        let p = proposal(
            "routine.create",
            serde_json::json!({ "name": "Morning", "frequency": "custom:3" }),
        );
        assert_eq!(p.summary(), "Create a every 3 days routine: Morning");

        let item = proposal(
            "routine.add_item",
            serde_json::json!({ "name": "Stretch", "estimated_duration_min": "10" }),
        );
        assert_eq!(item.summary(), "Add to a routine: Stretch");
        assert_eq!(item.detail().as_deref(), Some("About 10 minutes"));
    }

    /// ⚠️ An unknown action must still render something decidable — its
    /// arguments verbatim rather than a blank card.
    #[test]
    fn an_unknown_actions_arguments_are_shown_verbatim() {
        let p = proposal(
            "future.thing",
            serde_json::json!({ "target": "x", "count": 3 }),
        );
        let detail = p.detail().expect("some detail");
        assert!(detail.contains("target: x"), "{detail}");
        assert!(detail.contains("count: 3"), "{detail}");
    }

    /// Any decision at all takes it out of the inbox — including one this build
    /// cannot name, which the projection and `core` both treat as terminal.
    #[test]
    fn any_decision_value_means_it_is_no_longer_waiting() {
        let mut p = proposal("note.create", serde_json::json!({ "title": "t" }));
        assert!(p.is_pending());
        p.decision = Some("superseded-by-a-later-build".into());
        assert!(!p.is_pending());
    }
}

#[cfg(test)]
mod feature_tests {
    use super::*;

    fn entry(key: &str, on: bool) -> ConfigEntry {
        ConfigEntry {
            key: key.to_string(),
            label: key.to_string(),
            group: ConfigGroup::Features,
            effective: ConfigValue::Bool(on),
            layer: ConfigLayer::Global,
            global: Some(ConfigValue::Bool(on)),
            device: None,
            default: ConfigValue::Bool(true),
            applies_immediately: false,
            choices: None,
        }
    }

    /// The wire names must match core's, or a feature reads as absent — which
    /// falls back to *on*, so the mismatch is silent rather than loud.
    #[test]
    fn keys_match_the_wire_names_core_publishes() {
        let expected = [
            "feature.journal",
            "feature.notes",
            "feature.routines",
            "feature.finances",
            "feature.auto_import",
            "feature.llm",
        ];
        let actual: Vec<&str> = ALL_FEATURES.iter().map(|f| f.key()).collect();
        assert_eq!(actual, expected);
    }

    /// An empty or partial response must read as everything on. The opposite
    /// default would blank the whole app on a failed read, which looks like data
    /// loss rather than a preference.
    #[test]
    fn a_missing_key_reads_as_on() {
        assert_eq!(Features::from_entries(&[]), Features::default());

        let partial = Features::from_entries(&[entry("feature.finances", false)]);
        assert!(!partial.on(Feature::Finances));
        assert!(
            partial.on(Feature::Journal),
            "an absent key must read as on"
        );
    }

    /// A value of the wrong type is a bug in the writer; reading it as "off"
    /// would hide a tab over it. Mirrors `ResolvedConfig::bool_of`.
    #[test]
    fn a_wrong_typed_value_reads_as_on() {
        let mut e = entry("feature.routines", false);
        e.effective = ConfigValue::Text("nope".into());
        assert!(Features::from_entries(&[e]).on(Feature::Routines));
    }

    #[test]
    fn any_is_true_when_one_owner_is_on() {
        let f = Features::from_entries(&[entry("feature.journal", false)]);
        assert!(f.any(&[Feature::Journal, Feature::Notes]));

        let neither = Features::from_entries(&[
            entry("feature.journal", false),
            entry("feature.notes", false),
        ]);
        assert!(!neither.any(&[Feature::Journal, Feature::Notes]));
    }
}
