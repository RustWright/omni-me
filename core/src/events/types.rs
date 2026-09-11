use crate::config::Feature;
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
    GenericNoteAppended,
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
    // Feedback — the one event about the *app* rather than the user's content
    FeedbackCaptured,
    // Config — the app's own settings, shared across the user's devices
    ConfigSet,
    // Record types — the shape of the user's own records, shared across devices
    RecordTypeDeclared,
    // Assistant — a conversation, carried as events so it reaches every device
    // and so the resident agent can read a question out of the database it
    // already holds open (see `docs/src/assistant.md`).
    AssistantQuestionAsked,
    AssistantAnswerGiven,
    // Assistant proposals — the write half. The assistant records an intention;
    // nothing changes until a person decides. See `docs/src/assistant.md`
    // § "It proposes; you dispose".
    AssistantProposalMade,
    AssistantProposalDecided,
    // Beliefs — what the assistant has concluded about the user, with the
    // evidence behind it. Arrive only through a proposal the user accepted.
    BeliefRecorded,
    BeliefSuperseded,
    // Autonomy — which action types the user has allowed the assistant to
    // carry out without asking. Granted by the user, never requested by the
    // assistant.
    AutonomyGranted,
    AutonomyRevoked,
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
            EventType::GenericNoteAppended => "generic_note_appended",
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
            EventType::RecurringTransactionDetected => "recurring_transaction_detected",
            EventType::RecurringTransactionConfirmed => "recurring_transaction_confirmed",
            EventType::RecurringTransactionDismissed => "recurring_transaction_dismissed",
            EventType::ExchangeRateRecorded => "exchange_rate_recorded",
            EventType::AutoImportBatchProposed => "auto_import_batch_proposed",
            EventType::AutoImportBatchCommitted => "auto_import_batch_committed",
            EventType::AutoImportBatchDismissed => "auto_import_batch_dismissed",
            EventType::DataWiped => "data_wiped",
            EventType::FeedbackCaptured => "feedback_captured",
            EventType::ConfigSet => "config_set",
            EventType::RecordTypeDeclared => "record_type_declared",
            EventType::AssistantQuestionAsked => "assistant_question_asked",
            EventType::AssistantAnswerGiven => "assistant_answer_given",
            EventType::AssistantProposalMade => "assistant_proposal_made",
            EventType::AssistantProposalDecided => "assistant_proposal_decided",
            EventType::BeliefRecorded => "belief_recorded",
            EventType::BeliefSuperseded => "belief_superseded",
            EventType::AutonomyGranted => "autonomy_granted",
            EventType::AutonomyRevoked => "autonomy_revoked",
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
            "generic_note_appended" => Ok(EventType::GenericNoteAppended),
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
            "recurring_transaction_detected" => Ok(EventType::RecurringTransactionDetected),
            "recurring_transaction_confirmed" => Ok(EventType::RecurringTransactionConfirmed),
            "recurring_transaction_dismissed" => Ok(EventType::RecurringTransactionDismissed),
            "exchange_rate_recorded" => Ok(EventType::ExchangeRateRecorded),
            "auto_import_batch_proposed" => Ok(EventType::AutoImportBatchProposed),
            "auto_import_batch_committed" => Ok(EventType::AutoImportBatchCommitted),
            "auto_import_batch_dismissed" => Ok(EventType::AutoImportBatchDismissed),
            "data_wiped" => Ok(EventType::DataWiped),
            "feedback_captured" => Ok(EventType::FeedbackCaptured),
            "config_set" => Ok(EventType::ConfigSet),
            "record_type_declared" => Ok(EventType::RecordTypeDeclared),
            "assistant_question_asked" => Ok(EventType::AssistantQuestionAsked),
            "assistant_answer_given" => Ok(EventType::AssistantAnswerGiven),
            "assistant_proposal_made" => Ok(EventType::AssistantProposalMade),
            "assistant_proposal_decided" => Ok(EventType::AssistantProposalDecided),
            "belief_recorded" => Ok(EventType::BeliefRecorded),
            "belief_superseded" => Ok(EventType::BeliefSuperseded),
            "autonomy_granted" => Ok(EventType::AutonomyGranted),
            "autonomy_revoked" => Ok(EventType::AutonomyRevoked),
            other => Err(format!("unknown event type: {other}")),
        }
    }
}

impl EventType {
    /// Every event type.
    ///
    /// Hand-written, with the same acknowledged hole as `config::ALL_KEYS`: the
    /// exhaustive matches in this file stop the build when a variant is added, and
    /// `all_event_types_are_listed` then fails until it appears here.
    pub const ALL: &'static [EventType] = &[
        EventType::JournalEntryCreated,
        EventType::JournalEntryUpdated,
        EventType::JournalEntryClosed,
        EventType::JournalEntryReopened,
        EventType::GenericNoteCreated,
        EventType::GenericNoteUpdated,
        EventType::GenericNoteRenamed,
        EventType::GenericNoteAppended,
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
        EventType::RecurringTransactionDetected,
        EventType::RecurringTransactionConfirmed,
        EventType::RecurringTransactionDismissed,
        EventType::ExchangeRateRecorded,
        EventType::AutoImportBatchProposed,
        EventType::AutoImportBatchCommitted,
        EventType::AutoImportBatchDismissed,
        EventType::DataWiped,
        EventType::FeedbackCaptured,
        EventType::ConfigSet,
        EventType::RecordTypeDeclared,
        EventType::AssistantQuestionAsked,
        EventType::AssistantAnswerGiven,
        EventType::AssistantProposalMade,
        EventType::AssistantProposalDecided,
        EventType::BeliefRecorded,
        EventType::BeliefSuperseded,
        EventType::AutonomyGranted,
        EventType::AutonomyRevoked,
    ];

    /// The features that may author this event, or `None` for an event no feature
    /// owns.
    ///
    /// This is the **write** half of the feature map, and it is a match rather
    /// than a lookup table so a new event type cannot be added without deciding
    /// which feature is allowed to emit it. Guarding here instead of at each of
    /// the ~70 commands means one site covers every write path, including ones
    /// that do not exist yet.
    ///
    /// Empty for `DataWiped` (the wipe has to work with everything off, or a
    /// disabled feature's data would be unreachable *and* unremovable),
    /// `FeedbackCaptured` (reporting a problem must never depend on the feature
    /// you are reporting about), `ConfigSet` (it is what the gating reads) and
    /// `RecordTypeDeclared` (declaring the shape of your own data must not depend
    /// on the feature that renders it being switched on, and the first-run seed is
    /// emitted at startup, before any feature has been consulted).
    ///
    /// This says nothing about **inbound** events: the sync pull path applies
    /// whatever the log contains, so every device keeps a complete log no matter
    /// which features it has switched on.
    pub fn authoring_features(&self) -> &'static [Feature] {
        match self {
            EventType::JournalEntryCreated
            | EventType::JournalEntryUpdated
            | EventType::JournalEntryClosed
            | EventType::JournalEntryReopened => &[Feature::Journal],

            EventType::GenericNoteCreated
            | EventType::GenericNoteUpdated
            | EventType::GenericNoteRenamed
            | EventType::GenericNoteAppended => &[Feature::Notes],

            // Authored against either a journal entry or a generic note, so the
            // LLM feature carries it and the host feature has to be on too — the
            // guard admits it while *any* listed feature is on, and the command
            // that reaches it already belongs to one of them.
            EventType::NoteLlmProcessed => &[Feature::Llm],

            EventType::RoutineGroupCreated
            | EventType::RoutineGroupReordered
            | EventType::RoutineGroupRemoved
            | EventType::RoutineItemAdded
            | EventType::RoutineItemModified
            | EventType::RoutineItemRemoved
            | EventType::RoutineItemCompleted
            | EventType::RoutineItemCompletionUndone
            | EventType::RoutineItemSkipped
            | EventType::RoutineItemSkipUndone => &[Feature::Routines],

            // A committed auto-import batch is the second author of a
            // transaction, which is why `budget` survives finances being off
            // while auto-import is on (see `events::registry`).
            EventType::TransactionRecorded => &[Feature::Finances, Feature::AutoImport],

            EventType::TransactionCategorized
            | EventType::TransactionTagged
            | EventType::TransactionUpdated
            | EventType::TransactionDeleted
            | EventType::TransactionCleared
            | EventType::TransactionsMerged
            | EventType::BudgetSet
            | EventType::BudgetUpdated
            | EventType::BudgetRemoved
            | EventType::AccountAdded
            | EventType::RecurringTransactionDetected
            | EventType::RecurringTransactionConfirmed
            | EventType::RecurringTransactionDismissed
            | EventType::ExchangeRateRecorded => &[Feature::Finances],

            EventType::AutoImportBatchProposed
            | EventType::AutoImportBatchCommitted
            | EventType::AutoImportBatchDismissed => &[Feature::AutoImport],

            // Both halves of a conversation, and both under the same feature —
            // which is what makes one switch stop the whole exchange. The client
            // is refused when it tries to ask; the agent, resolving the same
            // shared config, is refused when it tries to answer. Guarding only
            // the question would leave answers arriving for questions the log
            // says were never admitted.
            EventType::AssistantQuestionAsked | EventType::AssistantAnswerGiven => &[Feature::Llm],

            // Making a proposal is the assistant acting, so it is gated like the
            // rest of the conversation.
            EventType::AssistantProposalMade => &[Feature::Llm],

            // A belief exists only because the assistant concluded it, so it is
            // the assistant's feature that owns it — even though the user is the
            // one who accepted it.
            EventType::BeliefRecorded | EventType::BeliefSuperseded => &[Feature::Llm],

            // Granting is about the assistant, so it is gated with it. ⚠️ Note
            // the asymmetry with `AssistantProposalDecided`, which is ungated:
            // that one had to stay reachable so a feature switch could not
            // strand a pending item. Nothing is stranded here — with the
            // assistant off, a grant that cannot be changed also cannot be
            // used, because nothing is proposing.
            EventType::AutonomyGranted | EventType::AutonomyRevoked => &[Feature::Llm],

            EventType::DataWiped
            | EventType::FeedbackCaptured
            | EventType::ConfigSet
            | EventType::RecordTypeDeclared
            // ⚠️ **Deliberately ungated, and this differs from the auto-import
            // precedent.** `AutoImportBatchDismissed` requires its own feature,
            // so switching auto-import off strands every pending batch in the
            // inbox with no way to clear it. Deciding a proposal is the *user*
            // closing something already in front of them, not the assistant
            // acting, and a feature switch must never be able to trap an item
            // there permanently. Nothing is weakened by this: approving a
            // proposal authors the real events in the same batch, and those
            // carry their own guards — turn Notes off and the note is still
            // refused. The guard belongs on the effect, not on the bookkeeping.
            | EventType::AssistantProposalDecided => &[],
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

/// Text added to the end of a note, without restating what is already there.
///
/// ⚠️ **The only payload in this file whose fold is not a plain overwrite**, and
/// the reason it exists. [`GenericNoteUpdatedPayload`] carries the whole body, so
/// an assistant asked to add a line has to reproduce the rest of the note
/// exactly — costing tokens in proportion to the note's length and, worse, able
/// to silently reword the part it was never asked to touch. This one cannot drop
/// what it did not send.
///
/// ⛔ **Appending only.** Inserting or replacing text in the middle is a
/// different problem — it needs a position that survives concurrent edits from
/// two devices — and is deliberately not attempted here. Do not widen this
/// payload with an offset; a byte index into a body another device has already
/// rewritten points at the wrong place, and the corruption is silent.
///
/// ⚠️ Folding this is **not naturally idempotent** — concatenation applied twice
/// is wrong, where an overwrite applied twice is not. `NotesProjection` keeps the
/// applied event ids on the row to make replay safe; see `on_generic_appended`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericNoteAppendedPayload {
    pub note_id: String,
    pub added_text: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecordedPayload {
    pub txn_id: String,
    pub date: chrono::NaiveDate,
    pub description: String,
    pub postings: Vec<Posting>,
    /// Transaction-level (header) tags — ledger inline `; key: value` on the
    /// date line. Posting-level tags live on each [`Posting`]; these belong to
    /// the whole entry and land in the projection's `tags_top`. Defaulted +
    /// skipped-when-empty so existing event payloads deserialize unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub tags: Vec<Tag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<AttachmentRef>,
    /// Provenance tag for statement-imported transactions (Phase 5.5).
    /// `None` for capture/auto-import/manual entries; `Some("summit-chequing-2026-05")`
    /// for rows imported from a bank statement CSV. Used by the unified
    /// reconciliation engine (5.7) to mark the cleared side on merge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_source: Option<String>,
}

impl TransactionRecordedPayload {
    /// The single canonical constructor for a transaction payload. Every path
    /// that records a transaction — the interactive command, the in-app journal
    /// import, auto-import accept, and the headless importer — MUST build the
    /// payload through here so the field shape and its defaults live in exactly
    /// one place. Two hand-rolled builders drifting is precisely what produced
    /// the sync-stranding bug this guards against.
    ///
    /// `tags` / `attachment` / `statement_source` default to empty/None; layer
    /// them on with the `with_*` builders.
    pub fn new(
        txn_id: String,
        date: chrono::NaiveDate,
        description: String,
        postings: Vec<Posting>,
    ) -> Self {
        Self {
            txn_id,
            date,
            description,
            postings,
            tags: Vec::new(),
            attachment: None,
            statement_source: None,
        }
    }

    /// Attach transaction-level (header) tags.
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<Tag>) -> Self {
        self.tags = tags;
        self
    }

    /// Attach a content-addressed file reference.
    #[must_use]
    pub fn with_attachment(mut self, attachment: Option<AttachmentRef>) -> Self {
        self.attachment = attachment;
        self
    }

    /// Mark the statement-import provenance (Phase 5.5).
    #[must_use]
    pub fn with_statement_source(mut self, statement_source: Option<String>) -> Self {
        self.statement_source = statement_source;
        self
    }
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

/// Transaction-level tag key recording which upstream row a committed
/// auto-import draft came from, e.g. `autoimport-id:globepay-TRANSFER-123`.
///
/// Lives here rather than beside either user because both sides need it and
/// they cannot share a feature-gated module: the commit path is in the Tauri
/// client (core without `auto-import`) while the sources that read it back are
/// in the server overlay (core *with* `auto-import`). Two copies of the literal
/// would drift silently — the writer would tag one way and the dedup filter
/// would look for another, which reads exactly like the filter doing nothing.
pub const AUTOIMPORT_TAG_KEY: &str = "autoimport-id";

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

// Feedback

/// A problem report filed from inside the app at the moment of friction.
///
/// **This is the only event about the app rather than about the user's own
/// content**, and that difference is why it has no projection: nothing derives
/// state from it, and its only reader is a query that renders markdown for a
/// person. Storing it as an event anyway buys the offline queue, retry, dedup
/// and cross-device replication that the write path already provides — the
/// alternative was rebuilding all four for a handful of rows a week.
///
/// **Only `feedback_id` and `body` are required.** Every context field is
/// optional and defaults, so a screen that declines to describe itself costs
/// the report a line rather than costing the user their report. Capture that
/// can fail validation is capture that gets abandoned mid-friction, which is
/// exactly the moment it is most needed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackCapturedPayload {
    /// Stable id for this report. `aggregate_id` mirrors it, per the factory
    /// discipline the create-event constructors follow.
    pub feedback_id: String,
    /// What the user typed.
    pub body: String,

    // --- Where it happened (free: already persisted by `NavState`) ---
    /// Tab plus sub-position, e.g. `notes:edit`, `finances:ledger`,
    /// `journal:calendar`. Flat string because `NavState` itself stores the
    /// position as strings, keeping that module dependency-free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
    /// The identity the screen was addressing, when it has one: a note ULID, a
    /// journal date. Separate from `screen` so a reader can look it up directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_ref: Option<String>,
    /// What was on screen, rendered to text by the capturing page from whatever
    /// it holds in the continuity store — the unsaved editor buffer, the
    /// half-filled transaction form, the loaded ledger rows and their filter.
    ///
    /// Deliberately opaque text rather than a typed union: the shape differs per
    /// page, no code parses it, and a union would need widening every time a
    /// page learned to describe itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_data: Option<String>,

    // --- Which build (identifies the report against a release) ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Resolved sync target and sandbox flag, from `get_runtime_profile`. Both
    /// matter because a report from a throwaway data root describes different
    /// behaviour than one from the real box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(default)]
    pub non_production: bool,
    /// Absolute app-data root. Carried in addition to `non_production` because
    /// that flag only says a sandbox, while this says *which* one — and a test
    /// run is usually one of several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,

    // --- What went wrong (populated once the diagnostic buffer exists) ---
    /// Recent panics, console errors and failed command calls, newest last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_errors: Vec<String>,
    /// Recent events authored on this device, newest last, as
    /// `<timestamp> <event_type> <aggregate_id>` lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_events: Vec<String>,
}

// Config

/// A change to one configuration key's **shared** value.
///
/// `aggregate_id` is the key's wire name, so `get_by_aggregate` yields that key's
/// history for free. Per-device overrides are deliberately absent from this
/// payload: they are local by definition and must never travel over sync.
///
/// `value: None` means "clear back to the built-in default" rather than "set to
/// nothing" — the projection deletes the row, and resolution falls through to
/// [`crate::config::ConfigKey::default_value`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSetPayload {
    pub key: crate::config::ConfigKey,
    #[serde(default)]
    pub value: Option<crate::config::ConfigValue>,
}

// Record types

/// A declaration of what one record type looks like.
///
/// `aggregate_id` is the type's name, so `get_by_aggregate` yields that type's
/// declaration history for free. The whole shape is carried on every declaration
/// rather than a diff against the last one: a record type is small, and a replay
/// that has to fold partial edits to know the current shape could not answer
/// "what did this note mean at the time" without replaying the whole log first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordTypeDeclaredPayload {
    #[serde(flatten)]
    pub record_type: crate::record_type::RecordType,
}

// Assistant

/// One question, as the user asked it.
///
/// `aggregate_id` is the `thread_id`, not the message id — a conversation is the
/// aggregate, so `get_by_aggregate` yields a whole thread in order and two
/// devices asking into the same thread converge on one row. That is the
/// `config_set` discipline (key as aggregate), not the create-event one (id as
/// aggregate), because the thing with identity here is the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantQuestionAskedPayload {
    pub thread_id: String,
    /// Stable id for this message, referenced by the answer's `in_reply_to`.
    pub message_id: String,
    pub text: String,
    /// Set on a thread's first question only, so a thread list has something to
    /// show. Absent on follow-ups rather than repeated, which would let two
    /// devices disagree about the title of the same conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// True when the agent asked this on the user's behalf rather than the user
    /// typing it.
    ///
    /// ⚠️ An explicit field rather than inferring it from `device_id`. The agent
    /// is a device like any other, so "authored by the agent" is not the same
    /// claim as "nobody asked for this" — and the difference is exactly what a
    /// person needs to see before reading an answer they did not request.
    /// Defaults to false, so every question in the log before this field existed
    /// reads correctly as user-asked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scheduled: bool,
}

/// Why an answer stopped, at the write site.
///
/// ⚠️ This is **not** the type carried on the wire — the payload's `stopped` is a
/// `String`, and this enum is how authors produce it. Same asymmetry as
/// [`EventType`] against `NewEvent::event_type`, and for a sharper reason here: a
/// newer build's stop reason must not make this one reject the payload, because a
/// rejected answer leaves its question with no terminal event and the UI spinning
/// forever — the exact state the field exists to end. Readers treat an
/// unrecognised value as "finished, reason unknown", never as "not finished".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerStop {
    /// The model replied in prose. The good ending.
    Answered,
    /// It used every turn still calling verbs.
    TurnBudget,
    /// The provider failed. Says nothing about the model's judgement.
    Failed,
    /// Never sent to a model: the question was older than the answering horizon
    /// when the agent picked it up. An agent that was down for a day would
    /// otherwise wake and pay for every question the user has since re-asked.
    Stale,
}

impl AnswerStop {
    pub fn as_str(self) -> &'static str {
        match self {
            AnswerStop::Answered => "answered",
            AnswerStop::TurnBudget => "turn_budget",
            AnswerStop::Failed => "failed",
            AnswerStop::Stale => "stale",
        }
    }

    /// `None` for a value this build does not know, which a reader must treat as
    /// a terminal state rather than as an absent one.
    pub fn parse(s: &str) -> Option<AnswerStop> {
        match s {
            "answered" => Some(AnswerStop::Answered),
            "turn_budget" => Some(AnswerStop::TurnBudget),
            "failed" => Some(AnswerStop::Failed),
            "stale" => Some(AnswerStop::Stale),
            _ => None,
        }
    }
}

impl fmt::Display for AnswerStop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// What one request cost, as recorded.
///
/// Deliberately **not** `crate::llm::chat::Usage`. That type parses one provider
/// family's response block and changes when a provider reports something new;
/// this one is a permanent record and must not move underneath the log. The
/// conversion is one `From` impl, and it drops `total_tokens` because a sum of
/// three fields stored beside them is a second place to be wrong.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnswerUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    /// Separate because it is billed and rate-limited as output and was ~95% of
    /// it on the baseline model. Folded in, it would misstate both the cost and
    /// where the time went.
    #[serde(default)]
    pub reasoning_tokens: u32,
}

/// A record the assistant actually opened while answering.
///
/// Derived from the `read` calls in the run's trace, never from the model's
/// prose: these are records it demonstrably had in hand, so a citation built
/// from them cannot be invented. The client renders them as links back into
/// journal and notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordRef {
    /// The catalog kind — `journal`, `note`, `routine`.
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// The agent's reply to one question — including the replies that failed.
///
/// **One event covers every ending.** A separate failure type would double the
/// event count for no gain, and a question with no terminal event is
/// indistinguishable from one still being worked on. `text` is `None` for
/// everything except [`AnswerStop::Answered`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantAnswerGivenPayload {
    pub thread_id: String,
    pub message_id: String,
    /// The `message_id` of the question this answers.
    pub in_reply_to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// An [`AnswerStop`] rendered to its wire name. See that type for why this is
    /// a `String`.
    pub stopped: String,
    /// The provider's failure sentence, when there was one. Kept apart from
    /// `text` so a client never renders an error as if it were an answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Which model answered. Recorded per answer rather than read from config at
    /// display time, because config moves and this is the audit trail — and,
    /// once a proposal corpus exists, the eval set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: AnswerUsage,
    #[serde(default)]
    pub elapsed_ms: u64,
    /// The verbs called, in order — the trace, compressed to what a person
    /// reading their own history would want. The arguments stay in the agent's
    /// logs: they are re-derivable, they can be long, and every one of them
    /// syncs to the phone if put here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verbs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records_read: Vec<RecordRef>,
}

// Assistant proposals

/// One thing the assistant offered to do.
///
/// `aggregate_id` is the `proposal_id`: a proposal is its own aggregate, unlike a
/// message, which belongs to the thread that owns it. That is what lets the
/// approval inbox pull one proposal by identity without reading the conversation
/// it came out of — the inbox is reachable from the phone whether or not the user
/// ever opens that thread.
///
/// ⚠️ **This event is the whole proposal, including everything needed to carry it
/// out.** Same reasoning as [`AutoImportBatchProposedPayload`] keeping its draft
/// rows: a decision may be taken days later, from another device, and must not
/// depend on re-deriving anything from a model run that is long gone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantProposalMadePayload {
    pub proposal_id: String,
    /// The conversation it came out of, so the inbox can link back to the
    /// reasoning. Not the aggregate — see the type's note.
    pub thread_id: String,
    /// The answer message that carried this proposal, for the same reason.
    pub message_id: String,
    /// The action type name, e.g. `note.create`. Matched against
    /// `assistant::actions` at decision time.
    pub action: String,
    /// The validated arguments, exactly as they will be carried out.
    pub args: serde_json::Value,
    /// One sentence from the model on why it is proposing this. Shown in the
    /// inbox, because "create a note titled X" without a reason is a decision the
    /// user has to reconstruct.
    pub rationale: String,
    /// Whether the action declared itself reversible **at proposal time**.
    ///
    /// ⚠️ Snapshotted rather than looked up when rendering. The registry is code
    /// and will move; the audit trail has to say what was claimed when the user
    /// was asked, not what a later build believes. A mismatch between this and
    /// the live declaration is a real signal, and it cannot be seen if the value
    /// is read live.
    pub reversible: bool,
}

/// What the user decided about one proposal.
///
/// ⚠️ **One event covers every ending**, and the reasoning is
/// [`AssistantAnswerGivenPayload`]'s rather than auto-import's two-event shape: a
/// proposal with no terminal event is indistinguishable from one still waiting,
/// so a newer build's decision value must never make this build *reject* the
/// payload and strand the proposal as pending forever.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantProposalDecidedPayload {
    pub proposal_id: String,
    /// A [`ProposalDecision`] rendered to its wire name. A `String` on the wire
    /// for the reason above; see that type.
    pub decision: String,
    /// The ids of the events the approval authored, when it was approved.
    ///
    /// The audit link in the direction a person actually asks for it — "what did
    /// accepting this actually do?" — and the reason it is recorded rather than
    /// re-derived is that nothing else connects a note to the proposal that made
    /// it. Empty on a rejection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_event_ids: Vec<String>,
    /// Free-form, optional, and only ever the user's own words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// How a proposal ended, at the write site.
///
/// ⚠️ Not the type carried on the wire — same asymmetry as [`AnswerStop`], and
/// for the same reason. Readers treat an unrecognised value as "decided, outcome
/// unknown", never as "still pending".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalDecision {
    /// The user accepted it and the events were authored.
    Approved,
    /// The user declined. The proposal stays in the log as "seen and refused",
    /// which is the evidence an approval rate is computed from.
    Rejected,
}

impl ProposalDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            ProposalDecision::Approved => "approved",
            ProposalDecision::Rejected => "rejected",
        }
    }

    /// `None` for a value this build does not know, which a reader must treat as
    /// terminal rather than as absent.
    pub fn parse(s: &str) -> Option<ProposalDecision> {
        match s {
            "approved" => Some(ProposalDecision::Approved),
            "rejected" => Some(ProposalDecision::Rejected),
            _ => None,
        }
    }
}

impl fmt::Display for ProposalDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// Beliefs

/// One conclusion the assistant reached and the user accepted.
///
/// ⚠️ **This event can only arrive through an approved proposal.** There is no
/// other author: `belief.record` is an action, and an action only runs at
/// approval. That is what makes the belief set auditable rather than merely
/// inspectable — every entry was seen by a person before it existed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeliefRecordedPayload {
    pub belief_id: String,
    /// The conclusion, one sentence, written about the user.
    pub statement: String,
    /// `low` | `medium` | `high`, per `assistant::actions::CONFIDENCE_LEVELS`.
    ///
    /// A `String` on the wire for [`AnswerStop`]'s reason, and three words rather
    /// than a number because a model's numeric confidence is uncalibrated
    /// precision that then reads as rigour.
    pub confidence: String,
    /// When to re-examine it, as `YYYY-MM-DD`.
    ///
    /// ⚠️ Chosen deliberately over decaying the confidence with age. Decay would
    /// compute a new number from one that was never calibrated, which is false
    /// precision squared; a review date says the honest thing instead — *this
    /// should be looked at again*, and a person decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_after: Option<String>,
    /// The records the run actually opened while reaching this.
    ///
    /// ⚠️ Derived from the trace, never written by the model — see
    /// `assistant::actions::ParamSource::EvidenceFromTrace`. A belief cannot cite
    /// a record the assistant never read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<RecordRef>,
}

/// A belief that no longer holds.
///
/// Retires rather than deletes: the statement and its evidence stay in the log,
/// so "what did it used to think, and why did that stop being true" is a query
/// rather than an archaeology exercise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeliefSupersededPayload {
    pub belief_id: String,
    /// What changed, in the assistant's words, for the user reading it later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The belief that replaced it, when one did. `None` means simply retired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

// Autonomy

/// The user allowing one action type to be carried out without being asked.
///
/// ⚠️ **Authored by the user, never by the assistant.** There is no verb and no
/// action that produces this; an assistant able to propose its own promotion is
/// the thing the whole permission model exists to prevent. What it may do is
/// accumulate the approval record that makes the user's decision an informed one.
///
/// ⚠️ **Only ever granted for a reversible action.** `docs/src/assistant.md`'s
/// destination is "reversible things it may do freely; irreversible things it
/// always asks about", so a grant on an irreversible action is refused at the
/// write site rather than merely discouraged — see `promotion::grant`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyGrantedPayload {
    /// The action type name, e.g. `note.create`.
    pub action: String,
    /// Whether the action declared itself reversible **when the grant was made**.
    ///
    /// Snapshotted for [`AssistantProposalMadePayload::reversible`]'s reason: the
    /// registry is code and moves, and the audit trail has to record what was
    /// true when the user decided. A grant whose snapshot disagrees with the live
    /// declaration is a signal worth seeing rather than one to paper over.
    pub reversible: bool,
}

/// The user taking that permission back.
///
/// Every grant is revocable, and revoking is not the same as never granting: the
/// log keeps both, so "it used to be allowed to do this" stays answerable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyRevokedPayload {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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
        EventType::GenericNoteAppended => {
            serde_json::from_value::<GenericNoteAppendedPayload>(payload.clone()).map(|_| ())
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
        EventType::FeedbackCaptured => {
            serde_json::from_value::<FeedbackCapturedPayload>(payload.clone()).map(|_| ())
        }
        EventType::ConfigSet => {
            serde_json::from_value::<ConfigSetPayload>(payload.clone()).map(|_| ())
        }
        EventType::RecordTypeDeclared => {
            serde_json::from_value::<RecordTypeDeclaredPayload>(payload.clone()).map(|_| ())
        }
        EventType::AssistantQuestionAsked => {
            serde_json::from_value::<AssistantQuestionAskedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AssistantProposalMade => {
            serde_json::from_value::<AssistantProposalMadePayload>(payload.clone()).map(|_| ())
        }
        EventType::BeliefRecorded => {
            serde_json::from_value::<BeliefRecordedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AutonomyGranted => {
            serde_json::from_value::<AutonomyGrantedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AutonomyRevoked => {
            serde_json::from_value::<AutonomyRevokedPayload>(payload.clone()).map(|_| ())
        }
        EventType::BeliefSuperseded => {
            serde_json::from_value::<BeliefSupersededPayload>(payload.clone()).map(|_| ())
        }
        EventType::AssistantProposalDecided => {
            serde_json::from_value::<AssistantProposalDecidedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AssistantAnswerGiven => {
            serde_json::from_value::<AssistantAnswerGivenPayload>(payload.clone()).map(|_| ())
        }
    };

    result.map_err(|e| {
        super::store::EventError::Validation(format!("invalid payload for {event_type}: {e}"))
    })?;

    // Config gets a second gate the other payloads don't need. For every other
    // event the payload's shape IS its contract; a config value's admissible type
    // and domain instead depend on which key carries it, so
    // `{"key":"appearance.theme","value":{"kind":"bool","value":true}}` decodes
    // perfectly and still means nothing. Re-decoding here rather than threading a
    // value out of the match keeps every arm the same shape, and the payload is
    // two fields on a rare event.
    if *event_type == EventType::ConfigSet {
        let parsed: ConfigSetPayload = serde_json::from_value(payload.clone()).map_err(|e| {
            super::store::EventError::Validation(format!("invalid payload for {event_type}: {e}"))
        })?;
        if let Some(value) = &parsed.value {
            parsed
                .key
                .validate(value)
                .map_err(super::store::EventError::Validation)?;
        }
    }

    // A record type gets the same second gate, for the same reason: the payload's
    // shape says nothing about whether its property keys are usable. A key holding
    // a colon decodes perfectly and then serializes into a frontmatter line the
    // completeness scanner reads as a different property entirely. Gating here
    // catches it on the way *in*, including from sync, rather than at each writer.
    if *event_type == EventType::RecordTypeDeclared {
        let parsed: RecordTypeDeclaredPayload =
            serde_json::from_value(payload.clone()).map_err(|e| {
                super::store::EventError::Validation(format!(
                    "invalid payload for {event_type}: {e}"
                ))
            })?;
        parsed
            .record_type
            .validate()
            .map_err(super::store::EventError::Validation)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_display_roundtrip() {
        for t in EventType::ALL {
            let s = t.to_string();
            let parsed: EventType = s.parse().unwrap();
            assert_eq!(&parsed, t);
        }
    }

    /// `EventType::ALL` is hand-written, and a type missing from it is invisible
    /// to everything that iterates the space — including the feature-ownership
    /// audits in `commands::shared`. Same shape and same hole as
    /// `config::tests::all_keys_lists_every_variant`: the exhaustive match stops
    /// the build here, then the count fails until `ALL` is updated too.
    #[test]
    fn all_event_types_are_listed() {
        let mut counted = 0;
        for t in EventType::ALL {
            match t {
                EventType::JournalEntryCreated
                | EventType::JournalEntryUpdated
                | EventType::JournalEntryClosed
                | EventType::JournalEntryReopened
                | EventType::GenericNoteCreated
                | EventType::GenericNoteUpdated
                | EventType::GenericNoteRenamed
                | EventType::GenericNoteAppended
                | EventType::NoteLlmProcessed
                | EventType::RoutineGroupCreated
                | EventType::RoutineGroupReordered
                | EventType::RoutineGroupRemoved
                | EventType::RoutineItemAdded
                | EventType::RoutineItemModified
                | EventType::RoutineItemRemoved
                | EventType::RoutineItemCompleted
                | EventType::RoutineItemCompletionUndone
                | EventType::RoutineItemSkipped
                | EventType::RoutineItemSkipUndone
                | EventType::TransactionRecorded
                | EventType::TransactionCategorized
                | EventType::TransactionTagged
                | EventType::TransactionUpdated
                | EventType::TransactionDeleted
                | EventType::TransactionCleared
                | EventType::TransactionsMerged
                | EventType::BudgetSet
                | EventType::BudgetUpdated
                | EventType::BudgetRemoved
                | EventType::AccountAdded
                | EventType::RecurringTransactionDetected
                | EventType::RecurringTransactionConfirmed
                | EventType::RecurringTransactionDismissed
                | EventType::ExchangeRateRecorded
                | EventType::AutoImportBatchProposed
                | EventType::AutoImportBatchCommitted
                | EventType::AutoImportBatchDismissed
                | EventType::DataWiped
                | EventType::FeedbackCaptured
                | EventType::ConfigSet
                | EventType::RecordTypeDeclared
                | EventType::AssistantQuestionAsked
                | EventType::AssistantAnswerGiven
                | EventType::AssistantProposalMade
                | EventType::AssistantProposalDecided
                | EventType::BeliefRecorded
                | EventType::BeliefSuperseded
                | EventType::AutonomyGranted
                | EventType::AutonomyRevoked => counted += 1,
            }
        }
        assert_eq!(counted, 49, "EventType::ALL does not list every variant");

        let unique: std::collections::BTreeSet<String> =
            EventType::ALL.iter().map(|t| t.to_string()).collect();
        assert_eq!(
            unique.len(),
            EventType::ALL.len(),
            "EventType::ALL repeats a type, or two share a wire name"
        );
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
        // The pre-rename names below must no longer parse: the rename was
        // decisive, deliberately without a back-compat alias.
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
    fn validate_recurring_transaction_lifecycle() {
        let detected = serde_json::json!({
            "pattern_id": "rec_netflix",
            "pattern": { "vendor": "Netflix", "amount": "16.99", "cadence_days": 30 }
        });
        assert!(validate_payload(&EventType::RecurringTransactionDetected, &detected).is_ok());

        let confirmed = serde_json::json!({ "pattern_id": "rec_netflix" });
        assert!(validate_payload(&EventType::RecurringTransactionConfirmed, &confirmed).is_ok());

        let dismissed = serde_json::json!({ "pattern_id": "rec_netflix" });
        assert!(validate_payload(&EventType::RecurringTransactionDismissed, &dismissed).is_ok());
    }

    #[test]
    fn validate_data_wiped() {
        let payload = serde_json::json!({
            "initiated_at": "2026-04-19T12:00:00Z",
            "device_id": "device-a"
        });
        assert!(validate_payload(&EventType::DataWiped, &payload).is_ok());
    }

    /// The design guarantee: a report with nothing but an id and a sentence is
    /// valid. Capture happens mid-friction, and a validation failure there costs
    /// the report entirely — so every context field must be droppable.
    #[test]
    fn validate_feedback_captured_with_no_context() {
        let payload = serde_json::json!({
            "feedback_id": "01HF2K3M4N5P6Q7R8S9TVWXYZA",
            "body": "cursor jumped behind the keyboard"
        });
        assert!(validate_payload(&EventType::FeedbackCaptured, &payload).is_ok());
    }

    #[test]
    fn feedback_captured_payload_roundtrips_through_validation() {
        let payload = FeedbackCapturedPayload {
            feedback_id: "01HF2K3M4N5P6Q7R8S9TVWXYZA".into(),
            body: "swiping between Ledger and Analyze does nothing".into(),
            screen: Some("finances:ledger".into()),
            screen_ref: None,
            screen_data: Some("42 rows loaded, filter=uncleared".into()),
            app_version: Some("1.0.5".into()),
            platform: Some("android".into()),
            server_url: Some("http://box:3000".into()),
            non_production: false,
            data_dir: Some("/home/u/.local/share/com.omni-me.app".into()),
            recent_errors: vec!["invoke list_transactions failed: timeout".into()],
            recent_events: vec!["2026-09-05T12:00:00Z transaction_cleared t_91".into()],
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(validate_payload(&EventType::FeedbackCaptured, &json).is_ok());

        let back: FeedbackCapturedPayload = serde_json::from_value(json).unwrap();
        assert_eq!(back.screen.as_deref(), Some("finances:ledger"));
        assert_eq!(back.recent_errors.len(), 1);
    }

    /// Empty context collections are skipped on the wire rather than serialized
    /// as `[]`, keeping a bare report small — it is one row of a log that every
    /// device replicates.
    #[test]
    fn feedback_captured_omits_empty_context_fields() {
        let payload = FeedbackCapturedPayload {
            feedback_id: "01HF2K3M4N5P6Q7R8S9TVWXYZA".into(),
            body: "typo on the settings page".into(),
            ..Default::default()
        };
        let json = serde_json::to_value(&payload).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("screen"));
        assert!(!obj.contains_key("recent_errors"));
        assert!(obj.contains_key("body"));
    }

    #[test]
    fn transaction_payload_new_defaults_optional_fields() {
        let p = TransactionRecordedPayload::new(
            "t1".into(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 4).unwrap(),
            "Coffee".into(),
            vec![],
        );
        assert!(p.tags.is_empty());
        assert!(p.attachment.is_none());
        assert!(p.statement_source.is_none());
        // Round-trips through the same schema the projection validates.
        let json = serde_json::to_value(&p).unwrap();
        assert!(validate_payload(&EventType::TransactionRecorded, &json).is_ok());
    }

    #[test]
    fn transaction_payload_with_builders_layer_on() {
        let p = TransactionRecordedPayload::new(
            "t1".into(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 4).unwrap(),
            "Salary".into(),
            vec![],
        )
        .with_tags(vec![Tag::KeyValue {
            key: "ref".into(),
            value: "abc".into(),
        }])
        .with_statement_source(Some("summit-2026-01".into()));
        assert_eq!(p.tags.len(), 1);
        assert_eq!(p.statement_source.as_deref(), Some("summit-2026-01"));
        assert!(p.attachment.is_none());
    }
}
