use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use std::fmt;
use std::str::FromStr;

/// All known event types in the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType {
    // Journal (date-keyed, one per day, templated)
    JournalEntryCreated,
    JournalEntryUpdated,
    JournalEntryClosed,
    JournalEntryReopened,
    // Generic notes (id-keyed, user-titled, free-form)
    GenericNoteCreated,
    GenericNoteUpdated,
    GenericNoteRenamed,
    // LLM (applies to either journal or generic via aggregate_id)
    NoteLlmProcessed,
    // Routines
    RoutineGroupCreated,
    RoutineGroupReordered,
    RoutineGroupRemoved,
    RoutineItemAdded,
    RoutineItemModified,
    RoutineItemRemoved,
    RoutineItemCompleted,
    RoutineItemCompletionUndone,
    RoutineItemSkipped,
    RoutineItemSkipUndone,
    // Budget — transactions
    TransactionRecorded,
    TransactionCategorized,
    TransactionTagged,
    TransactionUpdated,
    TransactionDeleted,
    TransactionCleared,
    TransactionsMerged,
    // Budget — budgets
    BudgetSet,
    BudgetUpdated,
    BudgetRemoved,
    // Budget — accounts
    AccountAdded,
    AccountReconciled,
    // Budget — recurring
    RecurringTransactionDetected,
    RecurringTransactionConfirmed,
    RecurringTransactionDismissed,
    // Budget — FX
    ExchangeRateRecorded,
    // Budget — auto-import batch review (Phase 3.10 / closes 2.12b)
    AutoImportBatchProposed,
    AutoImportBatchCommitted,
    AutoImportBatchDismissed,
    // Meta
    DataWiped,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            EventType::JournalEntryCreated => "journal_entry_created",
            EventType::JournalEntryUpdated => "journal_entry_updated",
            EventType::JournalEntryClosed => "journal_entry_closed",
            EventType::JournalEntryReopened => "journal_entry_reopened",
            EventType::GenericNoteCreated => "generic_note_created",
            EventType::GenericNoteUpdated => "generic_note_updated",
            EventType::GenericNoteRenamed => "generic_note_renamed",
            EventType::NoteLlmProcessed => "note_llm_processed",
            EventType::RoutineGroupCreated => "routine_group_created",
            EventType::RoutineGroupReordered => "routine_group_reordered",
            EventType::RoutineGroupRemoved => "routine_group_removed",
            EventType::RoutineItemAdded => "routine_item_added",
            EventType::RoutineItemModified => "routine_item_modified",
            EventType::RoutineItemRemoved => "routine_item_removed",
            EventType::RoutineItemCompleted => "routine_item_completed",
            EventType::RoutineItemCompletionUndone => "routine_item_completion_undone",
            EventType::RoutineItemSkipped => "routine_item_skipped",
            EventType::RoutineItemSkipUndone => "routine_item_skip_undone",
            EventType::TransactionRecorded => "transaction_recorded",
            EventType::TransactionCategorized => "transaction_categorized",
            EventType::TransactionTagged => "transaction_tagged",
            EventType::TransactionUpdated => "transaction_updated",
            EventType::TransactionDeleted => "transaction_deleted",
            EventType::TransactionCleared => "transaction_cleared",
            EventType::TransactionsMerged => "transactions_merged",
            EventType::BudgetSet => "budget_set",
            EventType::BudgetUpdated => "budget_updated",
            EventType::BudgetRemoved => "budget_removed",
            EventType::AccountAdded => "account_added",
            EventType::AccountReconciled => "account_reconciled",
            EventType::RecurringTransactionDetected => "recurring_transaction_detected",
            EventType::RecurringTransactionConfirmed => "recurring_transaction_confirmed",
            EventType::RecurringTransactionDismissed => "recurring_transaction_dismissed",
            EventType::ExchangeRateRecorded => "exchange_rate_recorded",
            EventType::AutoImportBatchProposed => "auto_import_batch_proposed",
            EventType::AutoImportBatchCommitted => "auto_import_batch_committed",
            EventType::AutoImportBatchDismissed => "auto_import_batch_dismissed",
            EventType::DataWiped => "data_wiped",
        };
        write!(f, "{s}")
    }
}

impl FromStr for EventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "journal_entry_created" => Ok(EventType::JournalEntryCreated),
            "journal_entry_updated" => Ok(EventType::JournalEntryUpdated),
            "journal_entry_closed" => Ok(EventType::JournalEntryClosed),
            "journal_entry_reopened" => Ok(EventType::JournalEntryReopened),
            "generic_note_created" => Ok(EventType::GenericNoteCreated),
            "generic_note_updated" => Ok(EventType::GenericNoteUpdated),
            "generic_note_renamed" => Ok(EventType::GenericNoteRenamed),
            "note_llm_processed" => Ok(EventType::NoteLlmProcessed),
            "routine_group_created" => Ok(EventType::RoutineGroupCreated),
            "routine_group_reordered" => Ok(EventType::RoutineGroupReordered),
            "routine_group_removed" => Ok(EventType::RoutineGroupRemoved),
            "routine_item_added" => Ok(EventType::RoutineItemAdded),
            "routine_item_modified" => Ok(EventType::RoutineItemModified),
            "routine_item_removed" => Ok(EventType::RoutineItemRemoved),
            "routine_item_completed" => Ok(EventType::RoutineItemCompleted),
            "routine_item_completion_undone" => Ok(EventType::RoutineItemCompletionUndone),
            "routine_item_skipped" => Ok(EventType::RoutineItemSkipped),
            "routine_item_skip_undone" => Ok(EventType::RoutineItemSkipUndone),
            "transaction_recorded" => Ok(EventType::TransactionRecorded),
            "transaction_categorized" => Ok(EventType::TransactionCategorized),
            "transaction_tagged" => Ok(EventType::TransactionTagged),
            "transaction_updated" => Ok(EventType::TransactionUpdated),
            "transaction_deleted" => Ok(EventType::TransactionDeleted),
            "transaction_cleared" => Ok(EventType::TransactionCleared),
            "transactions_merged" => Ok(EventType::TransactionsMerged),
            "budget_set" => Ok(EventType::BudgetSet),
            "budget_updated" => Ok(EventType::BudgetUpdated),
            "budget_removed" => Ok(EventType::BudgetRemoved),
            "account_added" => Ok(EventType::AccountAdded),
            "account_reconciled" => Ok(EventType::AccountReconciled),
            "recurring_transaction_detected" => Ok(EventType::RecurringTransactionDetected),
            "recurring_transaction_confirmed" => Ok(EventType::RecurringTransactionConfirmed),
            "recurring_transaction_dismissed" => Ok(EventType::RecurringTransactionDismissed),
            "exchange_rate_recorded" => Ok(EventType::ExchangeRateRecorded),
            "auto_import_batch_proposed" => Ok(EventType::AutoImportBatchProposed),
            "auto_import_batch_committed" => Ok(EventType::AutoImportBatchCommitted),
            "auto_import_batch_dismissed" => Ok(EventType::AutoImportBatchDismissed),
            "data_wiped" => Ok(EventType::DataWiped),
            other => Err(format!("unknown event type: {other}")),
        }
    }
}

// --- Typed payload structs ---

// Journal

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntryCreatedPayload {
    pub journal_id: String,
    pub date: chrono::NaiveDate,
    pub raw_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_properties: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntryUpdatedPayload {
    pub journal_id: String,
    pub raw_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseTrigger {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntryClosedPayload {
    pub journal_id: String,
    pub trigger: CloseTrigger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntryReopenedPayload {
    pub journal_id: String,
}

// Generic notes

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericNoteCreatedPayload {
    pub note_id: String,
    pub title: String,
    pub raw_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_properties: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericNoteUpdatedPayload {
    pub note_id: String,
    pub raw_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericNoteRenamedPayload {
    pub note_id: String,
    pub title: String,
}

// LLM — aggregate_id routes to either a journal_id or a note_id.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteLlmProcessedPayload {
    pub aggregate_id: String,
    pub prompt_version: String,
    pub model: String,
    pub derived: serde_json::Value,
}

// Routines — groups

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineGroupCreatedPayload {
    pub name: String,
    pub frequency: String,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineGroupReorderedPayload {
    pub orderings: Vec<GroupOrdering>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupOrdering {
    pub group_id: String,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineGroupRemovedPayload {
    pub group_id: String,
}

// Routines — items

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemAddedPayload {
    pub group_id: String,
    pub name: String,
    pub estimated_duration_min: u32,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemModifiedPayload {
    pub item_id: String,
    pub changes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemRemovedPayload {
    pub item_id: String,
}

// Routines — completion events

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemCompletedPayload {
    pub item_id: String,
    pub group_id: String,
    pub date: chrono::NaiveDate,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemCompletionUndonePayload {
    pub item_id: String,
    pub date: chrono::NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemSkippedPayload {
    pub item_id: String,
    pub group_id: String,
    pub date: chrono::NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemSkipUndonePayload {
    pub item_id: String,
    pub date: chrono::NaiveDate,
}

// Budget — transactions

/// Content-addressable blob reference for a file attached to a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentRef {
    pub sha256: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

/// FX rate captured at posting time — sourced from a receipt's `@` rate or
/// auto-import metadata. `quote_commodity` is the unit `rate` is denominated in,
/// matching hledger `100 USD @ 1.37 CAD` semantics (posting amount in USD,
/// `quote_commodity = "CAD"`).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxRate {
    pub quote_commodity: String,
    #[serde_as(as = "DisplayFromStr")]
    pub rate: Decimal,
}

#[derive(Debug, Clone)]
pub enum Tag {
    Bare(String),
    KeyValue { key: String, value: String },
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bare(s) => write!(f, "{s}"),
            Self::KeyValue { key, value } => write!(f, "{key}:{value}"),
        }
    }
}

impl FromStr for Tag {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("empty tag".into());
        }
        match s.split_once(':') {
            Some((k, v)) => Ok(Tag::KeyValue {
                key: k.into(),
                value: v.into(),
            }),
            None => Ok(Tag::Bare(s.into())),
        }
    }
}

/// Single posting line within a `TransactionRecorded` event. Mirrors hledger's
/// posting model: an account + an amount in a commodity, with optional FX rate
/// and tags. Amount is `rust_decimal::Decimal` (exact base-10 arithmetic) and
/// serializes as a string so JSON consumers don't downgrade it to f64.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Posting {
    pub account: String,
    pub commodity: String,
    #[serde_as(as = "DisplayFromStr")]
    pub amount: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx_rate: Option<FxRate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecordedPayload {
    pub txn_id: String,
    pub date: chrono::NaiveDate,
    pub description: String,
    pub postings: Vec<Posting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<AttachmentRef>,
    /// Provenance tag for statement-imported transactions (Phase 5.5).
    /// `None` for capture/auto-import/manual entries; `Some("summit-chequing-2026-05")`
    /// for rows imported from a bank statement CSV. Used by the unified
    /// reconciliation engine (5.7) to mark the cleared side on merge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionCategorizedPayload {
    pub txn_id: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionDeletedPayload {
    pub txn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionClearedPayload {
    pub txn_id: String,
    pub statement_source: String,
    pub cleared_date: chrono::NaiveDate,
}

/// Replace the tag set on an existing transaction. Projection overwrites
/// `tags`; partial-add / partial-remove semantics live at the command layer.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionTaggedPayload {
    pub txn_id: String,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub tags: Vec<Tag>,
}

/// Partial update to a transaction. `changes` is a JSON object of field-name
/// to new-value; projection inspects and applies what it knows. Mirrors the
/// schema-flexible pattern used by `RoutineItemModifiedPayload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionUpdatedPayload {
    pub txn_id: String,
    pub changes: serde_json::Value,
}

/// Unified-reconciliation merge: collapse `merged_ids` into `primary_id` with
/// `combined_postings` as the visible projection row. Originals are preserved
/// in the event log for audit. `balancing_posting` carries hidden-fee
/// resolution (e.g. wire fee, FX spread); zero means the `Unmatched` invariant
/// holds without correction. See [[project-unmatched-account-pattern]].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionsMergedPayload {
    pub primary_id: String,
    pub merged_ids: Vec<String>,
    pub combined_postings: Vec<Posting>,
    pub combined_description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combined_attachment: Option<AttachmentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balancing_posting: Option<Posting>,
}

// Budget — budgets

/// Set a budget target for a category over a period. `period` is stored as a
/// string (`"monthly"`, `"weekly"`, `"biweekly"`, `"custom:N"`) — same pattern
/// as `RoutineGroupCreatedPayload.frequency`. Projection parses on read.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetSetPayload {
    pub category: String,
    #[serde_as(as = "DisplayFromStr")]
    pub amount: Decimal,
    pub period: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetUpdatedPayload {
    pub category: String,
    pub changes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetRemovedPayload {
    pub category: String,
}

// Budget — accounts

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountAddedPayload {
    pub account: String,
    pub commodity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 3.9 override: hide an auto-detected account from the Accounts screen /
    /// net worth. Accounts are auto-included by type, so this is the only
    /// per-account knob the user sets. `#[serde(default)]` keeps existing
    /// `account_added` events (which never carried it) deserializing as
    /// visible.
    #[serde(default)]
    pub hidden: bool,
    /// 3.10 override (opt-in): mark this account as a *liquid* (spendable)
    /// asset so it counts toward the "Can I afford X?" verdict. Opt-in — an
    /// account is liquid only if the user explicitly marks it. `#[serde(default)]`
    /// keeps pre-3.10 `account_added` events deserializing as not-liquid, so
    /// the verdict falls back to net worth until the user opts an account in.
    #[serde(default)]
    pub is_liquid: bool,
}

/// Mark an account reconciled against a real statement. `statement_balance`
/// is in `commodity`; `cleared_through` is the statement's closing date.
/// Used by Phase 5.8 balance check.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountReconciledPayload {
    pub account: String,
    pub commodity: String,
    #[serde_as(as = "DisplayFromStr")]
    pub statement_balance: Decimal,
    pub cleared_through: chrono::NaiveDate,
}

// Budget — recurring

/// Pattern detected by the W3 scanner. `pattern` is left as schema-flexible
/// JSON because the matcher shape is decided in Phase 5.3 — emitting events
/// against a stable id now lets the pattern definition evolve without an
/// event-store migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringTransactionDetectedPayload {
    pub pattern_id: String,
    pub pattern: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringTransactionConfirmedPayload {
    pub pattern_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringTransactionDismissedPayload {
    pub pattern_id: String,
}

/// Daily FX rate, sourced from Frankfurter (or, post-Cycle-4, ExchangeRate-API
/// for non-Frankfurter currencies like AED). The journal-file projection
/// emits this as an hledger `P` directive so ledger-utils balance computation
/// can value foreign-commodity postings in the base currency.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRateRecordedPayload {
    pub date: chrono::NaiveDate,
    pub base: String,
    pub quote: String,
    #[serde_as(as = "DisplayFromStr")]
    pub rate: Decimal,
    /// Where the rate came from — for audit when a user spots a wrong rate.
    /// Examples: "frankfurter", "manual:meridian-may-2026".
    pub source: String,
}

// Budget — auto-import batch review (Phase 3.10 / closes 2.12b)

/// One row in a proposed auto-import batch — a single draft transaction the
/// user will review, edit, accept, or skip. Mirrors `TransactionRecordedPayload`
/// in shape (it becomes one on commit) but keeps `external_id` so the projection
/// can render the upstream's stable identifier alongside the row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftTransaction {
    /// Upstream's stable id — used for dedup at the row level (Globepay transfer
    /// id, Northwind internal txn id, IMAP message-uid + line offset, etc.). Distinct
    /// from `batch_id` (which scopes the whole batch).
    pub external_id: String,
    pub date: chrono::NaiveDate,
    pub description: String,
    pub postings: Vec<Posting>,
}

/// Scheduler emits this when an auto-import source produces a batch of
/// candidate transactions. Bytes are kept in the event payload (verbose
/// choice) so replay re-creates the pending state without re-fetching from
/// the upstream — important for IMAP, where the source message may be deleted
/// by the time the user replays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoImportBatchProposedPayload {
    /// ULID for the batch. Used as the cross-event correlation key — both
    /// `Committed` and `Dismissed` reference this `batch_id`.
    pub batch_id: String,
    /// Source identifier (`"globepay"`, `"meridian-aed"`, `"imap_receipts"`, etc.).
    /// Matches the value `AutoImportSource::name()` returns.
    pub source: String,
    /// Per-source idempotency key — what the scheduler checks to avoid
    /// re-proposing a batch it has already produced. Shape is source-defined
    /// (e.g., Meridian AED uses `format!("{source}-uid-{message_uid}")`).
    pub dedup_key: String,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub draft_postings: Vec<DraftTransaction>,
    /// Source-specific metadata kept opaque at the core layer — e.g., IMAP
    /// senders use this to stash `from`/`subject`/`uid` so the review UI can
    /// surface "from: statement@meridian.example · subject: April statement".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_metadata: Option<serde_json::Value>,
}

/// User commits the batch. Fans out into `TransactionRecorded` events (one
/// per accepted row) and optionally one `ExchangeRateRecorded` (when the
/// batch had a commodity in `MANUAL_FX_CURRENCIES` and the user supplied a
/// rate). `accepted_indices` are positions in the `Proposed.draft_postings`
/// vec — rows not in the list are dropped on the floor (audit trail of "user
/// saw it and decided not to record it" stays in the `Proposed` event).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoImportBatchCommittedPayload {
    pub batch_id: String,
    pub accepted_indices: Vec<usize>,
    /// FX rate the user typed in for the batch's manual-FX commodity, if any.
    /// Paired with `fx_commodity`; both Some or both None.
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx_rate: Option<Decimal>,
    /// Commodity the `fx_rate` quotes (e.g., "AED"). The base is implicit —
    /// the user's configured base currency at commit time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx_commodity: Option<String>,
}

/// User dismisses the batch — no transactions recorded. Reason is free-form
/// (UI may surface a small set of canned reasons or a text field) and
/// optional. The event existing at all is what dedup checks against; without
/// it, re-fetched-then-rejected batches would re-propose on the next tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoImportBatchDismissedPayload {
    pub batch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// Meta

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataWipedPayload {
    pub initiated_at: chrono::DateTime<chrono::Utc>,
    pub device_id: String,
}

/// Validate that a payload JSON value matches the expected shape for the given event type.
pub fn validate_payload(
    event_type: &EventType,
    payload: &serde_json::Value,
) -> Result<(), super::store::EventError> {
    let result = match event_type {
        EventType::JournalEntryCreated => {
            serde_json::from_value::<JournalEntryCreatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::JournalEntryUpdated => {
            serde_json::from_value::<JournalEntryUpdatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::JournalEntryClosed => {
            serde_json::from_value::<JournalEntryClosedPayload>(payload.clone()).map(|_| ())
        }
        EventType::JournalEntryReopened => {
            serde_json::from_value::<JournalEntryReopenedPayload>(payload.clone()).map(|_| ())
        }
        EventType::GenericNoteCreated => {
            serde_json::from_value::<GenericNoteCreatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::GenericNoteUpdated => {
            serde_json::from_value::<GenericNoteUpdatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::GenericNoteRenamed => {
            serde_json::from_value::<GenericNoteRenamedPayload>(payload.clone()).map(|_| ())
        }
        EventType::NoteLlmProcessed => {
            serde_json::from_value::<NoteLlmProcessedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineGroupCreated => {
            serde_json::from_value::<RoutineGroupCreatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineGroupReordered => {
            serde_json::from_value::<RoutineGroupReorderedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineGroupRemoved => {
            serde_json::from_value::<RoutineGroupRemovedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemAdded => {
            serde_json::from_value::<RoutineItemAddedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemModified => {
            serde_json::from_value::<RoutineItemModifiedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemRemoved => {
            serde_json::from_value::<RoutineItemRemovedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemCompleted => {
            serde_json::from_value::<RoutineItemCompletedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemCompletionUndone => {
            serde_json::from_value::<RoutineItemCompletionUndonePayload>(payload.clone())
                .map(|_| ())
        }
        EventType::RoutineItemSkipped => {
            serde_json::from_value::<RoutineItemSkippedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemSkipUndone => {
            serde_json::from_value::<RoutineItemSkipUndonePayload>(payload.clone()).map(|_| ())
        }
        EventType::TransactionRecorded => {
            serde_json::from_value::<TransactionRecordedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TransactionCategorized => {
            serde_json::from_value::<TransactionCategorizedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TransactionDeleted => {
            serde_json::from_value::<TransactionDeletedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TransactionCleared => {
            serde_json::from_value::<TransactionClearedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TransactionTagged => {
            serde_json::from_value::<TransactionTaggedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TransactionUpdated => {
            serde_json::from_value::<TransactionUpdatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TransactionsMerged => {
            serde_json::from_value::<TransactionsMergedPayload>(payload.clone()).map(|_| ())
        }
        EventType::BudgetSet => {
            serde_json::from_value::<BudgetSetPayload>(payload.clone()).map(|_| ())
        }
        EventType::BudgetUpdated => {
            serde_json::from_value::<BudgetUpdatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::BudgetRemoved => {
            serde_json::from_value::<BudgetRemovedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AccountAdded => {
            serde_json::from_value::<AccountAddedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AccountReconciled => {
            serde_json::from_value::<AccountReconciledPayload>(payload.clone()).map(|_| ())
        }
        EventType::RecurringTransactionDetected => {
            serde_json::from_value::<RecurringTransactionDetectedPayload>(payload.clone())
                .map(|_| ())
        }
        EventType::RecurringTransactionConfirmed => {
            serde_json::from_value::<RecurringTransactionConfirmedPayload>(payload.clone())
                .map(|_| ())
        }
        EventType::RecurringTransactionDismissed => {
            serde_json::from_value::<RecurringTransactionDismissedPayload>(payload.clone())
                .map(|_| ())
        }
        EventType::ExchangeRateRecorded => {
            serde_json::from_value::<ExchangeRateRecordedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AutoImportBatchProposed => {
            serde_json::from_value::<AutoImportBatchProposedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AutoImportBatchCommitted => {
            serde_json::from_value::<AutoImportBatchCommittedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AutoImportBatchDismissed => {
            serde_json::from_value::<AutoImportBatchDismissedPayload>(payload.clone()).map(|_| ())
        }
        EventType::DataWiped => {
            serde_json::from_value::<DataWipedPayload>(payload.clone()).map(|_| ())
        }
    };

    result.map_err(|e| {
        super::store::EventError::Validation(format!("invalid payload for {event_type}: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_display_roundtrip() {
        let types = [
            EventType::JournalEntryCreated,
            EventType::JournalEntryUpdated,
            EventType::JournalEntryClosed,
            EventType::JournalEntryReopened,
            EventType::GenericNoteCreated,
            EventType::GenericNoteUpdated,
            EventType::GenericNoteRenamed,
            EventType::NoteLlmProcessed,
            EventType::RoutineGroupCreated,
            EventType::RoutineGroupReordered,
            EventType::RoutineGroupRemoved,
            EventType::RoutineItemAdded,
            EventType::RoutineItemModified,
            EventType::RoutineItemRemoved,
            EventType::RoutineItemCompleted,
            EventType::RoutineItemCompletionUndone,
            EventType::RoutineItemSkipped,
            EventType::RoutineItemSkipUndone,
            EventType::TransactionRecorded,
            EventType::TransactionCategorized,
            EventType::TransactionTagged,
            EventType::TransactionUpdated,
            EventType::TransactionDeleted,
            EventType::TransactionCleared,
            EventType::TransactionsMerged,
            EventType::BudgetSet,
            EventType::BudgetUpdated,
            EventType::BudgetRemoved,
            EventType::AccountAdded,
            EventType::AccountReconciled,
            EventType::RecurringTransactionDetected,
            EventType::RecurringTransactionConfirmed,
            EventType::RecurringTransactionDismissed,
            EventType::ExchangeRateRecorded,
            EventType::AutoImportBatchProposed,
            EventType::AutoImportBatchCommitted,
            EventType::AutoImportBatchDismissed,
            EventType::DataWiped,
        ];

        for t in &types {
            let s = t.to_string();
            let parsed: EventType = s.parse().unwrap();
            assert_eq!(&parsed, t);
        }
    }

    #[test]
    fn auto_import_batch_proposed_payload_roundtrip() {
        let payload = AutoImportBatchProposedPayload {
            batch_id: "01HF2K3M4N5P6Q7R8S9TVWXYZA".into(),
            source: "meridian-aed".into(),
            dedup_key: "meridian-aed-uid-42".into(),
            fetched_at: chrono::Utc::now(),
            draft_postings: vec![DraftTransaction {
                external_id: "meridian-aed-uid-42-row-0".into(),
                date: chrono::NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(),
                description: "POS Lagos Mall".into(),
                postings: vec![],
            }],
            source_metadata: Some(serde_json::json!({"from": "estatements@meridian.example"})),
        };
        let json = serde_json::to_value(&payload).unwrap();
        validate_payload(&EventType::AutoImportBatchProposed, &json).unwrap();
        let back: AutoImportBatchProposedPayload = serde_json::from_value(json).unwrap();
        assert_eq!(back.batch_id, payload.batch_id);
        assert_eq!(back.dedup_key, payload.dedup_key);
        assert_eq!(back.draft_postings.len(), 1);
    }

    #[test]
    fn auto_import_batch_committed_payload_roundtrip_with_fx() {
        use rust_decimal::Decimal;
        let payload = AutoImportBatchCommittedPayload {
            batch_id: "01HF...".into(),
            accepted_indices: vec![0, 2, 3],
            fx_rate: Some(Decimal::new(84, 5)), // 0.00084
            fx_commodity: Some("AED".into()),
        };
        let json = serde_json::to_value(&payload).unwrap();
        validate_payload(&EventType::AutoImportBatchCommitted, &json).unwrap();
        let back: AutoImportBatchCommittedPayload = serde_json::from_value(json).unwrap();
        assert_eq!(back.fx_rate, payload.fx_rate);
        assert_eq!(back.fx_commodity.as_deref(), Some("AED"));
        assert_eq!(back.accepted_indices, vec![0, 2, 3]);
    }

    #[test]
    fn auto_import_batch_committed_payload_roundtrip_without_fx() {
        // Globepay batch — all CAD/USD/EUR, no manual FX needed.
        let payload = AutoImportBatchCommittedPayload {
            batch_id: "01HG...".into(),
            accepted_indices: vec![0, 1, 2, 3, 4],
            fx_rate: None,
            fx_commodity: None,
        };
        let json = serde_json::to_value(&payload).unwrap();
        validate_payload(&EventType::AutoImportBatchCommitted, &json).unwrap();
        // fx_rate + fx_commodity should not serialize when None.
        let json_str = serde_json::to_string(&payload).unwrap();
        assert!(!json_str.contains("fx_rate"));
        assert!(!json_str.contains("fx_commodity"));
    }

    #[test]
    fn auto_import_batch_dismissed_payload_roundtrip() {
        let payload = AutoImportBatchDismissedPayload {
            batch_id: "01HH...".into(),
            reason: Some("Gemini hallucinated rows".into()),
        };
        let json = serde_json::to_value(&payload).unwrap();
        validate_payload(&EventType::AutoImportBatchDismissed, &json).unwrap();
        let back: AutoImportBatchDismissedPayload = serde_json::from_value(json).unwrap();
        assert_eq!(back.reason.as_deref(), Some("Gemini hallucinated rows"));
    }

    #[test]
    fn tag_display_from_str_roundtrip() {
        for raw in ["work", "type:business", "due:2026-04-15T10:00"] {
            let parsed: Tag = raw.parse().unwrap();
            assert_eq!(parsed.to_string(), raw);
        }
    }

    #[test]
    fn unknown_event_type_errors() {
        assert!("unknown_type".parse::<EventType>().is_err());
        // old Cycle 1 types must no longer parse — decisive rename, not an alias.
        assert!("note_created".parse::<EventType>().is_err());
        assert!("note_updated".parse::<EventType>().is_err());
    }

    #[test]
    fn validate_journal_entry_created_ok() {
        let payload = serde_json::json!({
            "journal_id": "01JKJRNL000000000000000000",
            "date": "2026-04-19",
            "raw_text": "Today I shipped."
        });
        assert!(validate_payload(&EventType::JournalEntryCreated, &payload).is_ok());
    }

    #[test]
    fn validate_journal_entry_created_with_legacy_properties() {
        let payload = serde_json::json!({
            "journal_id": "01JKJRNL000000000000000000",
            "date": "2026-04-19",
            "raw_text": "imported",
            "legacy_properties": { "mood": "tired", "weather": "rain" }
        });
        assert!(validate_payload(&EventType::JournalEntryCreated, &payload).is_ok());
    }

    #[test]
    fn validate_journal_entry_closed_trigger_enum() {
        let manual = serde_json::json!({
            "journal_id": "01JKJRNL000000000000000000",
            "trigger": "manual"
        });
        assert!(validate_payload(&EventType::JournalEntryClosed, &manual).is_ok());

        let auto = serde_json::json!({
            "journal_id": "01JKJRNL000000000000000000",
            "trigger": "auto"
        });
        assert!(validate_payload(&EventType::JournalEntryClosed, &auto).is_ok());

        let bogus = serde_json::json!({
            "journal_id": "01JKJRNL000000000000000000",
            "trigger": "whenever"
        });
        assert!(
            validate_payload(&EventType::JournalEntryClosed, &bogus).is_err(),
            "trigger must be exactly manual|auto"
        );
    }

    #[test]
    fn validate_generic_note_created_ok() {
        let payload = serde_json::json!({
            "note_id": "01JKNOTE00000000000000000",
            "title": "Ideas for the app",
            "raw_text": "random brain dump"
        });
        assert!(validate_payload(&EventType::GenericNoteCreated, &payload).is_ok());
    }

    #[test]
    fn validate_llm_processed_uses_aggregate_id() {
        let payload = serde_json::json!({
            "aggregate_id": "01JKAGGREGATE0000000000000",
            "prompt_version": "v2",
            "model": "gemini-flash",
            "derived": { "tags": ["focus"] }
        });
        assert!(validate_payload(&EventType::NoteLlmProcessed, &payload).is_ok());

        // `note_id` is no longer the key — must fail.
        let legacy = serde_json::json!({
            "note_id": "01JKAGGREGATE0000000000000",
            "prompt_version": "v1",
            "model": "gemini-flash",
            "derived": {}
        });
        assert!(
            validate_payload(&EventType::NoteLlmProcessed, &legacy).is_err(),
            "legacy note_id field must no longer satisfy the LLM payload"
        );
    }

    #[test]
    fn validate_routine_group_created_drops_time_of_day() {
        let payload = serde_json::json!({
            "name": "Morning",
            "frequency": "daily",
            "order": 0
        });
        assert!(validate_payload(&EventType::RoutineGroupCreated, &payload).is_ok());

        // time_of_day is dropped — old payloads missing `order` must fail.
        let legacy = serde_json::json!({
            "name": "Morning",
            "frequency": "daily",
            "time_of_day": "morning"
        });
        assert!(
            validate_payload(&EventType::RoutineGroupCreated, &legacy).is_err(),
            "order is now required — old time_of_day-based payloads must not validate"
        );
    }

    #[test]
    fn validate_routine_group_reordered() {
        let payload = serde_json::json!({
            "orderings": [
                { "group_id": "g1", "order": 0 },
                { "group_id": "g2", "order": 1 }
            ]
        });
        assert!(validate_payload(&EventType::RoutineGroupReordered, &payload).is_ok());
    }

    #[test]
    fn validate_routine_item_skipped_reason_is_optional() {
        let with_reason = serde_json::json!({
            "item_id": "i1",
            "group_id": "g1",
            "date": "2026-04-19",
            "reason": "traveling"
        });
        assert!(validate_payload(&EventType::RoutineItemSkipped, &with_reason).is_ok());

        let without_reason = serde_json::json!({
            "item_id": "i1",
            "group_id": "g1",
            "date": "2026-04-19"
        });
        assert!(validate_payload(&EventType::RoutineItemSkipped, &without_reason).is_ok());
    }

    #[test]
    fn validate_undo_events() {
        let completion_undo = serde_json::json!({
            "item_id": "i1",
            "date": "2026-04-19"
        });
        assert!(
            validate_payload(&EventType::RoutineItemCompletionUndone, &completion_undo).is_ok()
        );

        let skip_undo = serde_json::json!({
            "item_id": "i1",
            "date": "2026-04-19"
        });
        assert!(validate_payload(&EventType::RoutineItemSkipUndone, &skip_undo).is_ok());
    }

    #[test]
    fn validate_transaction_recorded_full() {
        let payload = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000000",
            "date": "2026-05-16",
            "description": "Loblaws grocery run",
            "postings": [
                {
                    "account": "Assets:Checking:Northwind",
                    "commodity": "CAD",
                    "amount": "-87.42",
                    "tags": []
                },
                {
                    "account": "Expenses:Groceries",
                    "commodity": "CAD",
                    "amount": "87.42",
                    "tags": ["type:business"]
                }
            ],
            "attachment": {
                "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "filename": "loblaws-2026-05-16.jpg",
                "mime_type": "image/jpeg",
                "size": 184320
            }
        });
        assert!(validate_payload(&EventType::TransactionRecorded, &payload).is_ok());
    }

    #[test]
    fn validate_transaction_recorded_minimal() {
        // No attachment, no fx_rate, empty tags allowed via defaults.
        let payload = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000001",
            "date": "2026-05-16",
            "description": "Coffee",
            "postings": [
                { "account": "Assets:Cash", "commodity": "CAD", "amount": "-5.25" },
                { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "5.25" }
            ]
        });
        assert!(validate_payload(&EventType::TransactionRecorded, &payload).is_ok());
    }

    #[test]
    fn validate_transaction_recorded_with_fx_rate() {
        let payload = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000002",
            "date": "2026-05-16",
            "description": "USD subscription",
            "postings": [
                {
                    "account": "Assets:Globepay:USD",
                    "commodity": "USD",
                    "amount": "-10.00",
                    "fx_rate": { "quote_commodity": "CAD", "rate": "1.37" }
                },
                { "account": "Expenses:Software", "commodity": "CAD", "amount": "13.70" }
            ]
        });
        assert!(validate_payload(&EventType::TransactionRecorded, &payload).is_ok());
    }

    #[test]
    fn validate_transaction_recorded_amount_must_be_string() {
        // serde_with::DisplayFromStr requires the wire form to be a JSON string,
        // not a JSON number — guards against silent f64-via-Decimal corruption.
        let payload = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000003",
            "date": "2026-05-16",
            "description": "Bad client",
            "postings": [
                { "account": "Assets:Cash", "commodity": "CAD", "amount": 5.25 }
            ]
        });
        assert!(
            validate_payload(&EventType::TransactionRecorded, &payload).is_err(),
            "Decimal must come over the wire as a string, not a JSON number"
        );
    }

    #[test]
    fn validate_transaction_categorized() {
        let payload = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000000",
            "category": "Groceries"
        });
        assert!(validate_payload(&EventType::TransactionCategorized, &payload).is_ok());
    }

    #[test]
    fn validate_transaction_deleted() {
        let payload = serde_json::json!({ "txn_id": "01JKTXN0000000000000000000" });
        assert!(validate_payload(&EventType::TransactionDeleted, &payload).is_ok());
    }

    #[test]
    fn validate_transaction_cleared() {
        let payload = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000000",
            "statement_source": "summit-chequing-2026-05",
            "cleared_date": "2026-05-15"
        });
        assert!(validate_payload(&EventType::TransactionCleared, &payload).is_ok());
    }

    #[test]
    fn validate_transaction_tagged() {
        let payload = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000000",
            "tags": ["type:business", "project:omni-me"]
        });
        assert!(validate_payload(&EventType::TransactionTagged, &payload).is_ok());

        let empty_tags_allowed = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000000",
            "tags": []
        });
        assert!(validate_payload(&EventType::TransactionTagged, &empty_tags_allowed).is_ok());
    }

    #[test]
    fn validate_transaction_updated() {
        let payload = serde_json::json!({
            "txn_id": "01JKTXN0000000000000000000",
            "changes": { "description": "Loblaws — corrected" }
        });
        assert!(validate_payload(&EventType::TransactionUpdated, &payload).is_ok());
    }

    #[test]
    fn validate_transactions_merged_minimal() {
        let payload = serde_json::json!({
            "primary_id": "01JKTXN0000000000000000000",
            "merged_ids": ["01JKTXN0000000000000000001"],
            "combined_postings": [
                { "account": "Assets:Northwind:Cash", "commodity": "CAD", "amount": "-100.00" },
                { "account": "Assets:Globepay:CAD", "commodity": "CAD", "amount": "100.00" }
            ],
            "combined_description": "Northwind → Globepay transfer"
        });
        assert!(validate_payload(&EventType::TransactionsMerged, &payload).is_ok());
    }

    #[test]
    fn validate_transactions_merged_with_balancing_posting() {
        // Hidden-fee resolution: the merged pair sums to non-zero on Unmatched
        // because Globepay took a $1.50 wire fee; user adds a balancing posting to
        // close the gap. All optional fields populated.
        let payload = serde_json::json!({
            "primary_id": "01JKTXN0000000000000000000",
            "merged_ids": ["01JKTXN0000000000000000001"],
            "combined_postings": [
                { "account": "Assets:Northwind:Cash", "commodity": "CAD", "amount": "-100.00" },
                { "account": "Assets:Globepay:CAD", "commodity": "CAD", "amount": "98.50" }
            ],
            "combined_description": "Northwind → Globepay transfer (with fee)",
            "combined_attachment": {
                "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "filename": "globepay-confirmation.pdf",
                "mime_type": "application/pdf",
                "size": 2048
            },
            "balancing_posting": {
                "account": "Expenses:Bank-Fees",
                "commodity": "CAD",
                "amount": "1.50"
            }
        });
        assert!(validate_payload(&EventType::TransactionsMerged, &payload).is_ok());
    }

    #[test]
    fn validate_budget_set() {
        let payload = serde_json::json!({
            "category": "Groceries",
            "amount": "600.00",
            "period": "monthly"
        });
        assert!(validate_payload(&EventType::BudgetSet, &payload).is_ok());
    }

    #[test]
    fn validate_budget_updated() {
        let payload = serde_json::json!({
            "category": "Groceries",
            "changes": { "amount": "650.00" }
        });
        assert!(validate_payload(&EventType::BudgetUpdated, &payload).is_ok());
    }

    #[test]
    fn validate_budget_removed() {
        let payload = serde_json::json!({ "category": "Groceries" });
        assert!(validate_payload(&EventType::BudgetRemoved, &payload).is_ok());
    }

    #[test]
    fn validate_account_added() {
        let with_display = serde_json::json!({
            "account": "Assets:Northwind:Cash",
            "commodity": "CAD",
            "display_name": "Northwind Chequing"
        });
        assert!(validate_payload(&EventType::AccountAdded, &with_display).is_ok());

        let minimal = serde_json::json!({
            "account": "Assets:Northwind:Cash",
            "commodity": "CAD"
        });
        assert!(validate_payload(&EventType::AccountAdded, &minimal).is_ok());
    }

    #[test]
    fn validate_account_reconciled() {
        let payload = serde_json::json!({
            "account": "Assets:Summit:Chequing",
            "commodity": "CAD",
            "statement_balance": "5076.10",
            "cleared_through": "2026-04-30"
        });
        assert!(validate_payload(&EventType::AccountReconciled, &payload).is_ok());
    }

    #[test]
    fn validate_recurring_transaction_lifecycle() {
        let detected = serde_json::json!({
            "pattern_id": "rec_netflix",
            "pattern": { "vendor": "Netflix", "amount": "16.99", "cadence_days": 30 }
        });
        assert!(
            validate_payload(&EventType::RecurringTransactionDetected, &detected).is_ok()
        );

        let confirmed = serde_json::json!({ "pattern_id": "rec_netflix" });
        assert!(
            validate_payload(&EventType::RecurringTransactionConfirmed, &confirmed).is_ok()
        );

        let dismissed = serde_json::json!({ "pattern_id": "rec_netflix" });
        assert!(
            validate_payload(&EventType::RecurringTransactionDismissed, &dismissed).is_ok()
        );
    }

    #[test]
    fn validate_data_wiped() {
        let payload = serde_json::json!({
            "initiated_at": "2026-04-19T12:00:00Z",
            "device_id": "device-a"
        });
        assert!(validate_payload(&EventType::DataWiped, &payload).is_ok());
    }
}
