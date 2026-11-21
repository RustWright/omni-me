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
    // Housekeeping on a conversation: hide it, or remove it from every view.
    AssistantThreadArchived,
    AssistantThreadDeleted,
    // Beliefs — what the assistant has concluded about the user, with the
    // evidence behind it. Arrive only through a proposal the user accepted.
    BeliefRecorded,
    BeliefSuperseded,
    // Autonomy — which action types the user has allowed the assistant to
    // carry out without asking. Granted by the user, never requested by the
    // assistant.
    AutonomyGranted,
    AutonomyRevoked,
    // Document archive — the file itself, and what was read out of it. Two
    // events rather than one because ingest has to succeed when structuring
    // cannot (a scan nothing can parse is still a document worth keeping), and
    // because a correction is necessarily a second event in an append-only log.
    DocumentArchived,
    DocumentFieldsExtracted,
    DocumentTextTranscribed,
    /// A document removed on purpose, bytes and all.
    ///
    /// A tombstone, not a deletion. The archive event stays in the log — it has
    /// to, the log is append-only and a projection rebuild replays it — so the row
    /// comes back on every rebuild and this is what makes it come back *purged*
    /// rather than pointing at bytes that are gone.
    DocumentPurged,
    /// A retention rule for one document tag. See `crate::retention`.
    DocumentRetentionSet,
    // Tasks: native tasks and projects, and hub indexes pushed by session hooks.
    TaskCreated,
    TaskModified,
    TaskStatusSet,
    TaskTimerStarted,
    TaskTimerPaused,
    TaskTimerStopped,
    TaskProjectCreated,
    TaskProjectClosed,
    HubIndexReceived,
    DeadlineFeedReceived,
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
            EventType::AssistantThreadArchived => "assistant_thread_archived",
            EventType::AssistantThreadDeleted => "assistant_thread_deleted",
            EventType::BeliefRecorded => "belief_recorded",
            EventType::BeliefSuperseded => "belief_superseded",
            EventType::AutonomyGranted => "autonomy_granted",
            EventType::AutonomyRevoked => "autonomy_revoked",
            EventType::DocumentArchived => "document_archived",
            EventType::DocumentFieldsExtracted => "document_fields_extracted",
            EventType::DocumentTextTranscribed => "document_text_transcribed",
            EventType::DocumentPurged => "document_purged",
            EventType::DocumentRetentionSet => "document_retention_set",
            EventType::TaskCreated => "task_created",
            EventType::TaskModified => "task_modified",
            EventType::TaskStatusSet => "task_status_set",
            EventType::TaskTimerStarted => "task_timer_started",
            EventType::TaskTimerPaused => "task_timer_paused",
            EventType::TaskTimerStopped => "task_timer_stopped",
            EventType::TaskProjectCreated => "task_project_created",
            EventType::TaskProjectClosed => "task_project_closed",
            EventType::HubIndexReceived => "hub_index_received",
            EventType::DeadlineFeedReceived => "deadline_feed_received",
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
            "assistant_thread_archived" => Ok(EventType::AssistantThreadArchived),
            "assistant_thread_deleted" => Ok(EventType::AssistantThreadDeleted),
            "belief_recorded" => Ok(EventType::BeliefRecorded),
            "belief_superseded" => Ok(EventType::BeliefSuperseded),
            "autonomy_granted" => Ok(EventType::AutonomyGranted),
            "autonomy_revoked" => Ok(EventType::AutonomyRevoked),
            "document_archived" => Ok(EventType::DocumentArchived),
            "document_fields_extracted" => Ok(EventType::DocumentFieldsExtracted),
            "document_text_transcribed" => Ok(EventType::DocumentTextTranscribed),
            "document_purged" => Ok(EventType::DocumentPurged),
            "document_retention_set" => Ok(EventType::DocumentRetentionSet),
            "task_created" => Ok(EventType::TaskCreated),
            "task_modified" => Ok(EventType::TaskModified),
            "task_status_set" => Ok(EventType::TaskStatusSet),
            "task_timer_started" => Ok(EventType::TaskTimerStarted),
            "task_timer_paused" => Ok(EventType::TaskTimerPaused),
            "task_timer_stopped" => Ok(EventType::TaskTimerStopped),
            "task_project_created" => Ok(EventType::TaskProjectCreated),
            "task_project_closed" => Ok(EventType::TaskProjectClosed),
            "hub_index_received" => Ok(EventType::HubIndexReceived),
            "deadline_feed_received" => Ok(EventType::DeadlineFeedReceived),
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
        EventType::AssistantThreadArchived,
        EventType::AssistantThreadDeleted,
        EventType::BeliefRecorded,
        EventType::BeliefSuperseded,
        EventType::AutonomyGranted,
        EventType::AutonomyRevoked,
        EventType::DocumentArchived,
        EventType::DocumentFieldsExtracted,
        EventType::DocumentTextTranscribed,
        EventType::DocumentPurged,
        EventType::DocumentRetentionSet,
        EventType::TaskCreated,
        EventType::TaskModified,
        EventType::TaskStatusSet,
        EventType::TaskTimerStarted,
        EventType::TaskTimerPaused,
        EventType::TaskTimerStopped,
        EventType::TaskProjectCreated,
        EventType::TaskProjectClosed,
        EventType::HubIndexReceived,
        EventType::DeadlineFeedReceived,
    ];

    /// Every event type a set of features owns, for a scoped wipe.
    ///
    /// Derived from [`EventType::authoring_features`], never a second list that could go stale.
    /// `TransactionRecorded` belongs to finances and auto-import, so wiping either claims it.
    /// The five unowned types survive every wipe, keeping the config and the wipe's own record.
    /// `docs/src/features.md` § "Wiping one feature's data".
    pub fn owned_by(features: &[Feature]) -> Vec<&'static EventType> {
        EventType::ALL
            .iter()
            .filter(|t| {
                t.authoring_features()
                    .iter()
                    .any(|owner| features.contains(owner))
            })
            .collect()
    }

    /// The features that may author this event, or `None` for an event no feature
    /// owns.
    ///
    /// A match, so no event type exists without deciding who may emit it; one site guards every
    /// write path. Empty for the five that must work with features off (`DataWiped`,
    /// `FeedbackCaptured`, `ConfigSet`, `RecordTypeDeclared`, `AssistantProposalDecided`).
    /// Outbound only: pulls apply everything.
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

            // Tidying the conversation list; with the assistant off there is no list.
            EventType::AssistantThreadArchived | EventType::AssistantThreadDeleted => {
                &[Feature::Llm]
            }

            // A belief exists only because the assistant concluded it, so it is
            // the assistant's feature that owns it — even though the user is the
            // one who accepted it.
            EventType::BeliefRecorded | EventType::BeliefSuperseded => &[Feature::Llm],

            // Granting is about the assistant, so it is gated with it. Note
            // the asymmetry with `AssistantProposalDecided`, which is ungated:
            // that one had to stay reachable so a feature switch could not
            // strand a pending item. Nothing is stranded here — with the
            // assistant off, a grant that cannot be changed also cannot be
            // used, because nothing is proposing.
            EventType::AutonomyGranted | EventType::AutonomyRevoked => &[Feature::Llm],

            EventType::DocumentArchived
            | EventType::DocumentFieldsExtracted
            | EventType::DocumentTextTranscribed
            | EventType::DocumentPurged
            | EventType::DocumentRetentionSet => &[Feature::Documents],

            EventType::TaskCreated
            | EventType::TaskModified
            | EventType::TaskStatusSet
            | EventType::TaskTimerStarted
            | EventType::TaskTimerPaused
            | EventType::TaskTimerStopped
            | EventType::TaskProjectCreated
            | EventType::TaskProjectClosed
            | EventType::HubIndexReceived
            | EventType::DeadlineFeedReceived => &[Feature::Tasks],

            EventType::DataWiped
            | EventType::FeedbackCaptured
            | EventType::ConfigSet
            | EventType::RecordTypeDeclared
            // Ungated on purpose: deciding is the user closing an item, which a switch must never
            // trap (auto-import-off strands pending batches). The approved events carry their own
            // guards; the guard belongs on the effect, not the bookkeeping.
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
/// Unlike [`GenericNoteUpdatedPayload`]'s whole body, it cannot reword what it did not send.
/// Append only: never add an offset, since a byte index into a body another device rewrote
/// corrupts silently. Not idempotent; see `NotesProjection::on_generic_appended`.
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
    /// The archive document the same capture filed, which links a transaction to it.
    /// Absent on attachments stored before captures were archived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
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

/// Separator between tags when a whole set is stored in one string.
///
/// `,` because `:` is already spent on [`Tag`]'s key/value split. [`Tag::normalize`]
/// refuses a tag containing it, so a set cannot round-trip into more tags than it
/// went in with.
pub const TAG_SEPARATOR: char = ',';

impl Tag {
    /// Parse into the form a tag is stored and compared in: trimmed, lowercased.
    ///
    /// Stricter than [`FromStr`], since comparisons are exact and a case or padding variant is a
    /// second tag. `FromStr` stays lenient: it parses old log events that cannot be rewritten.
    pub fn normalize(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim().to_lowercase();
        if trimmed.is_empty() {
            return Err("empty tag".into());
        }
        if trimmed.contains(TAG_SEPARATOR) {
            return Err(format!("a tag cannot contain '{TAG_SEPARATOR}'"));
        }
        match trimmed.split_once(':') {
            Some((k, v)) if k.is_empty() || v.is_empty() => {
                Err(format!("'{trimmed}' needs both halves of key:value"))
            }
            Some((k, v)) => Ok(Tag::KeyValue {
                key: k.to_string(),
                value: v.to_string(),
            }),
            None => Ok(Tag::Bare(trimmed)),
        }
    }
}

/// Normalize a caller's tags into the set to store: ordered and deduplicated.
///
/// Ordered so equal sets store equally; deduped, since arrays enforce no uniqueness. A bad tag
/// rejects the whole set, never dropped silently.
pub fn normalize_tag_set<I, S>(raw: I) -> Result<Vec<Tag>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut tags: Vec<Tag> = raw
        .into_iter()
        .map(|s| Tag::normalize(s.as_ref()))
        .collect::<Result<_, _>>()?;
    // Keyed on `Display` because `Tag` derives neither `Ord` nor `Hash`, and the
    // stored form is what has to be unique.
    tags.sort_by_key(|t| t.to_string());
    tags.dedup_by_key(|t| t.to_string());
    Ok(tags)
}

/// Join a tag set into the single value one `DocumentField` carries.
pub fn encode_tag_set(tags: &[Tag]) -> String {
    tags.iter()
        .map(Tag::to_string)
        .collect::<Vec<_>>()
        .join(&TAG_SEPARATOR.to_string())
}

/// Split a stored value back into tags.
///
/// Lenient on purpose, and it is the read half of a pair whose write half is
/// strict: this parses what is already in the log, including values written by a
/// build that normalized differently or not at all. An unparseable segment is
/// skipped rather than failing the fold — one bad tag must not cost a document
/// its row.
pub fn decode_tag_set(value: &str) -> Vec<Tag> {
    value
        .split(TAG_SEPARATOR)
        .filter_map(|s| {
            let s = s.trim();
            (!s.is_empty()).then(|| s.parse().ok()).flatten()
        })
        .collect()
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
    /// The one constructor every path recording a transaction must use, since drifting hand-built
    /// payloads once stranded synced data. Optional fields come from the `with_*` builders.
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
/// Here because the writer (client, no `auto-import`) and the reader (server, with it) share no
/// gated module; two copies would drift and the dedup filter would silently do nothing.
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
    /// Rows the reviewer changed before committing. The `Proposed` event keeps
    /// the model's version, so original and correction are both on record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrections: Vec<DraftCorrection>,
    /// Rows the reviewer entered by hand, typically for a receipt the reader
    /// got nothing from. `index` is the position within this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<DraftCorrection>,
}

/// One proposed row as the reviewer committed it, replacing the draft at
/// `index` in `Proposed.draft_postings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftCorrection {
    pub index: usize,
    pub date: chrono::NaiveDate,
    pub description: String,
    pub postings: Vec<Posting>,
    /// Transaction tags, normalized; the committed transaction carries them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
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
    /// The features a server wipe took, spelled as their config key's suffix.
    /// Empty for a device's local wipe of everything, which peers ignore.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
}

impl DataWipedPayload {
    /// The named features this build knows. An unknown name is skipped, so a
    /// newer server's feature cannot make an older device purge anything else.
    pub fn known_features(&self) -> Vec<crate::config::Feature> {
        crate::config::ALL_FEATURES
            .iter()
            .copied()
            .filter(|f| {
                let key = f.key().to_string();
                let name = key.strip_prefix("feature.").unwrap_or(&key);
                self.features.iter().any(|n| n.trim() == name)
            })
            .collect()
    }
}

// Feedback

/// A problem report filed from inside the app at the moment of friction.
///
/// About the app, not the user's content, so no projection; an event for the free offline queue
/// and replication. Only `feedback_id` and `body` are required, so capture never fails
/// mid-friction.
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

/// A change to one configuration key's shared value.
///
/// `aggregate_id` is the key's wire name, giving its history. Device overrides never travel.
/// `value: None` clears to [`crate::config::ConfigKey::default_value`], not "set to nothing".
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
    /// Explicit, not inferred from `device_id`: authored by the agent is not "nobody asked".
    /// Defaults false, so older questions read as user-asked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scheduled: bool,
    /// Which scheduled prompt raised it; each prompt's due-check and housekeeping read only their
    /// own. Absent on a scheduled question from before prompts were named: see [`Self::prompt`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_name: Option<String>,
}

/// The name a scheduled question carries when it predates named prompts: the check-in was the
/// only one.
pub const LEGACY_SCHEDULED_PROMPT: &str = "check_in";

impl AssistantQuestionAskedPayload {
    /// The scheduled prompt this question belongs to, or `None` for one the user asked.
    pub fn prompt(&self) -> Option<&str> {
        match (&self.prompt_name, self.scheduled) {
            (Some(name), _) => Some(name.as_str()),
            (None, true) => Some(LEGACY_SCHEDULED_PROMPT),
            (None, false) => None,
        }
    }
}

/// Why an answer stopped, at the write site.
///
/// Writes `stopped`, which travels as a `String`, so a newer build's reason never makes this one
/// reject the answer and spin forever. Readers treat an unknown value as finished.
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
/// Deliberately not `crate::llm::chat::Usage`. That type parses one provider
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
/// One event covers every ending. A separate failure type would double the
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
/// Its own aggregate (`proposal_id`), so the inbox reads it without the conversation. Carries
/// everything needed to act, like [`AutoImportBatchProposedPayload`]: a decision days later must
/// not depend on a model run long gone.
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
    /// Whether the action declared itself reversible at proposal time.
    ///
    /// Snapshotted rather than looked up when rendering. The registry is code
    /// and will move; the audit trail has to say what was claimed when the user
    /// was asked, not what a later build believes. A mismatch between this and
    /// the live declaration is a real signal, and it cannot be seen if the value
    /// is read live.
    pub reversible: bool,
}

/// A conversation hidden from the list, or brought back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantThreadArchivedPayload {
    pub thread_id: String,
    /// `false` brings it back. Last write wins, by event timestamp.
    pub archived: bool,
}

/// A conversation removed from every view, permanently. The log keeps its
/// messages; proposals made in it stay decidable in the inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantThreadDeletedPayload {
    pub thread_id: String,
}

/// What the user decided about one proposal.
///
/// One event covers every ending, and the reasoning is
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
/// Not the type carried on the wire — same asymmetry as [`AnswerStop`], and
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
/// This event can only arrive through an approved proposal. There is no
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
    /// Chosen deliberately over decaying the confidence with age. Decay would
    /// compute a new number from one that was never calibrated, which is false
    /// precision squared; a review date says the honest thing instead — *this
    /// should be looked at again*, and a person decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_after: Option<String>,
    /// The records the run actually opened while reaching this.
    ///
    /// Derived from the trace, never written by the model — see
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
/// Authored by the user only: no verb produces it, since an assistant proposing its own promotion
/// is what the permission model prevents. Reversible actions only; `promotion::grant` refuses
/// the rest at the write site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyGrantedPayload {
    /// The action type name, e.g. `note.create`.
    pub action: String,
    /// Whether the action declared itself reversible when the grant was made.
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

// Document archive

/// A file entering the archive: the bytes, and the text they were found to hold.
///
/// The text travels in the event because blobs do not sync: a device that never opened a
/// document lacks its bytes, and the archive would be searchable only where it was cached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentArchivedPayload {
    /// Identity of the archive entry.
    ///
    /// Deliberately not `sha256`. The same bytes can legitimately be archived
    /// twice — the statement that arrived by email and the one scanned from
    /// paper are one file and two events, with different sources and dates — and
    /// keying on content would silently collapse them into whichever landed last.
    pub document_id: String,
    /// Content address of the bytes in the blob store, as `AttachmentRef` uses.
    pub sha256: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    /// RFC3339, from the archiving device's clock.
    pub archived_at: String,
    /// How it arrived: `scan` | `upload` | `email` | `bulk`.
    pub source: String,
    /// The document's text, when anything could read one out of it.
    ///
    /// `None` is an ordinary outcome, not a failure — an image with no legible
    /// text is still archived, and stays findable by filename and by its fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Where [`Self::text`] came from: `extracted` (a text layer the file
    /// actually carried) | `transcribed` (a model read it off an image) | `none`.
    ///
    /// Not decoration. Transcribed text is a model's reading and can be wrong
    /// in ways extracted text cannot, so search over it is correspondingly less
    /// trustworthy — and a reader who cannot tell the two apart will trust both
    /// equally.
    pub text_source: String,
    /// The document this one arrived *inside*, when it arrived inside anything.
    ///
    /// An attachment points to its email, so a draft shows beside its message; [`Self::source`]
    /// names only the channel. Optional forever, for older events. A link, not ownership: never
    /// cascade a delete, as the same bytes may be filed again from another channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_document_id: Option<String>,
}

/// One extracted value, with an honest account of where it came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentField {
    pub key: String,
    pub value: String,
    /// `parser:<id>` | `model:<name>@<ver>` | `human`.
    ///
    /// A string rather than an enum because the model and parser identifiers are
    /// open sets that move with the build, and an enum would force a payload
    /// change every time either did.
    pub source: String,
    /// Whether anything actually checked this value against something.
    ///
    /// True only against a real oracle, such as a statement's own figures; model output never is.
    /// Never add a confidence score beside it: a number implies a calibration nothing provides.
    pub verified: bool,
}

impl DocumentField {
    /// How far this field's origin outranks another's when both claim one key.
    ///
    /// Stops a re-extraction undoing hand corrections, which latest-wins would: a human beats
    /// anything, a checked parser beats a model, and equal ranks fall back to arrival.
    pub fn rank(source: &str) -> u8 {
        if source == "human" {
            2
        } else if source.starts_with("parser:") {
            1
        } else {
            0
        }
    }
}

/// What was read out of a document — re-emittable, and expected to be re-emitted.
///
/// Separate from [`DocumentArchivedPayload`] because the two answer different
/// questions at different times. Ingest knows the bytes; it may know nothing
/// else, and a document nothing can structure still belongs in the archive. Later
/// a parser, a model, or the user supplies fields — each emitting one of these,
/// each folding by key under [`DocumentField::rank`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentFieldsExtractedPayload {
    pub document_id: String,
    /// RFC3339, from the extracting device's clock.
    pub extracted_at: String,
    /// Every extracted value, folded by key.
    ///
    /// `kind`, `title` and `document_date` are keys here, hoisted to columns by the projection but
    /// ranked like any key; their own fields would be a second provenance mechanism.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<DocumentField>,
}

/// A model's reading of a document that carried no text layer of its own.
///
/// Its own event: [`DocumentArchivedPayload`] is written once at ingest, and a fields key would
/// re-serialize the body on every extraction and mix two provenance vocabularies. Re-emittable,
/// as a better model is just another one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTextTranscribedPayload {
    /// The archive entry this reading belongs to — `DocumentArchivedPayload::document_id`.
    pub document_id: String,
    /// What the model read off the document.
    ///
    /// Always `TextSource::Transcribed` at the projection, never `extracted`:
    /// this event exists precisely because nothing deterministic could read the
    /// file. The two must stay distinguishable downstream — search over a
    /// model's reading is less trustworthy than search over what a file states.
    pub text: String,
    /// `<name>@<ver>`, matching the `model:` half of [`DocumentField::source`].
    ///
    /// No confidence score beside it, for `DocumentField::verified`'s reason:
    /// nothing here checks a transcription against an oracle, and a number would
    /// imply a calibration that does not exist.
    pub model: String,
    /// RFC3339, from the transcribing device's clock.
    pub transcribed_at: String,
    /// `extracted` when the text came off the file's own text layer after ingest
    /// (an encrypted PDF re-read once a password exists, `model` = `pdftotext`).
    /// Absent means a model's reading, which is every event before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_source: Option<String>,
}

/// A document purged on purpose: its row tombstoned and its bytes reclaimed.
///
/// A tombstone: the archive event replays on rebuild, so without it the row would return with its
/// bytes gone; readers must tell purged from broken. No `sha256` on purpose: blobs are shared,
/// and deleting one is safe only after counting every referrer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPurgedPayload {
    /// The archive entry being purged — `DocumentArchivedPayload::document_id`.
    pub document_id: String,
    /// RFC3339, from the purging device's clock.
    pub purged_at: String,
    /// Why it went, in the user's terms — the tag or sender group it was purged
    /// under. Recorded because a purge is irreversible and the log is the only
    /// account of it that survives; a bare tombstone cannot answer "why is this
    /// gone".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// How long documents carrying one tag are kept.
///
/// `aggregate_id` is the tag, giving its rule history (why per tag: [`crate::retention`]).
/// `keep_days: None` clears the rule, the same as never set; there is no separate "never".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRetentionSetPayload {
    /// The tag this governs, already normalized by [`normalize_tag_set`]'s rules.
    pub tag: String,
    /// Days to keep a document carrying this tag, counted from `archived_at`.
    /// `None` clears the rule.
    #[serde(default)]
    pub keep_days: Option<u32>,
}

// Tasks: rulings 90-93 and 121-131 (RULINGS.md); the shared rules are in `crate::tasks`.

/// A native task. `aggregate_id` is `task_id`, a ULID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreatedPayload {
    pub task_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// His estimate in minutes, made at planning: ruling 93 (RULINGS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate_min: Option<u32>,
    /// A real external deadline only, as `tasks::parse_due` reads it: ruling 130 (RULINGS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    /// The day he intends to do it. Never rolled forward silently: ruling 130 (RULINGS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled: Option<chrono::NaiveDate>,
    /// A `tasks::RepeatRule`; each occurrence is its own task. Ruling 131 (RULINGS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<String>,
    /// The first occurrence's id, on every later occurrence of a repeating task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    /// The hub id this was taken over from; the hub item is then ignored. Ruling 121 (RULINGS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_id: Option<String>,
    /// The feed deadline this task carries; the feed stays the source of its due date.
    /// Ruling 122 (RULINGS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A change to a native task: only the fields given change, and `clear` empties fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskModifiedPayload {
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate_min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clear: Vec<crate::tasks::TaskField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusSetPayload {
    pub task_id: String,
    pub status: crate::tasks::TaskStatus,
    /// Minutes it took, asked at completion when no timer ran: ruling 93 (RULINGS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_min: Option<u32>,
}

/// A timer start, pause or stop; the time is the event's timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTimerPayload {
    pub task_id: String,
}

/// A native project. `aggregate_id` is `project_id`, a ULID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProjectCreatedPayload {
    pub project_id: String,
    pub name: String,
    /// The course code, for a project made from the LEARN feed: ruling 123 (RULINGS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
    /// When it should close; a course closes when its term ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_on: Option<chrono::NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProjectClosedPayload {
    pub project_id: String,
}

/// A project's `.omni/tasks.toml` as pushed, parsed. `aggregate_id` is the project slug.
pub type HubIndexReceivedPayload = crate::tasks::hub::HubIndex;

/// A read of a deadline feed (LEARN), parsed. `aggregate_id` is the feed's source name.
pub type DeadlineFeedReceivedPayload = crate::tasks::deadlines::DeadlineFeed;

/// Keys the projection hoists into columns. See [`DocumentFieldsExtractedPayload::fields`].
pub const DOCUMENT_KIND_KEY: &str = "kind";
pub const DOCUMENT_TITLE_KEY: &str = "title";
pub const DOCUMENT_DATE_KEY: &str = "document_date";
/// The whole tag set, joined by [`TAG_SEPARATOR`]. One key rather than one key
/// per tag because the field fold can overwrite a key but never remove one, so
/// per-tag keys would leave a model's wrong tag impossible to take off.
pub const DOCUMENT_TAGS_KEY: &str = "tags";

/// The retention category, one of `retention_class::CLASSES`. Its own key, not a
/// tag: a charge found by the receipt importer writes it at parser rank, which
/// must outrank the reader's guess without touching the reader's tags.
pub const DOCUMENT_RETENTION_KEY: &str = "retention";

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
        EventType::DocumentRetentionSet => {
            serde_json::from_value::<DocumentRetentionSetPayload>(payload.clone()).map(|_| ())
        }
        EventType::TaskCreated => {
            serde_json::from_value::<TaskCreatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TaskModified => {
            serde_json::from_value::<TaskModifiedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TaskStatusSet => {
            serde_json::from_value::<TaskStatusSetPayload>(payload.clone()).map(|_| ())
        }
        EventType::TaskTimerStarted => {
            serde_json::from_value::<TaskTimerPayload>(payload.clone()).map(|_| ())
        }
        EventType::TaskTimerPaused => {
            serde_json::from_value::<TaskTimerPayload>(payload.clone()).map(|_| ())
        }
        EventType::TaskTimerStopped => {
            serde_json::from_value::<TaskTimerPayload>(payload.clone()).map(|_| ())
        }
        EventType::TaskProjectCreated => {
            serde_json::from_value::<TaskProjectCreatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::TaskProjectClosed => {
            serde_json::from_value::<TaskProjectClosedPayload>(payload.clone()).map(|_| ())
        }
        EventType::HubIndexReceived => {
            serde_json::from_value::<HubIndexReceivedPayload>(payload.clone()).map(|_| ())
        }
        EventType::DeadlineFeedReceived => {
            serde_json::from_value::<DeadlineFeedReceivedPayload>(payload.clone()).map(|_| ())
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
        EventType::AssistantThreadArchived => {
            serde_json::from_value::<AssistantThreadArchivedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AssistantThreadDeleted => {
            serde_json::from_value::<AssistantThreadDeletedPayload>(payload.clone()).map(|_| ())
        }
        EventType::AssistantAnswerGiven => {
            serde_json::from_value::<AssistantAnswerGivenPayload>(payload.clone()).map(|_| ())
        }
        EventType::DocumentArchived => {
            serde_json::from_value::<DocumentArchivedPayload>(payload.clone()).map(|_| ())
        }
        EventType::DocumentFieldsExtracted => {
            serde_json::from_value::<DocumentFieldsExtractedPayload>(payload.clone()).map(|_| ())
        }
        EventType::DocumentTextTranscribed => {
            serde_json::from_value::<DocumentTextTranscribedPayload>(payload.clone()).map(|_| ())
        }
        EventType::DocumentPurged => {
            serde_json::from_value::<DocumentPurgedPayload>(payload.clone()).map(|_| ())
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

    // A due date or repeat rule decodes as any string; checked here so a bad one from sync
    // never reaches the planner.
    let task_formats = match event_type {
        EventType::TaskCreated => serde_json::from_value::<TaskCreatedPayload>(payload.clone())
            .ok()
            .map(|p| (p.due, p.repeat)),
        EventType::TaskModified => serde_json::from_value::<TaskModifiedPayload>(payload.clone())
            .ok()
            .map(|p| (p.due, p.repeat)),
        _ => None,
    };
    if let Some((due, repeat)) = task_formats {
        if let Some(due) = due {
            crate::tasks::valid_due(&due).map_err(super::store::EventError::Validation)?;
        }
        if let Some(repeat) = repeat {
            crate::tasks::valid_repeat(&repeat).map_err(super::store::EventError::Validation)?;
        }
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
                | EventType::AssistantThreadArchived
                | EventType::AssistantThreadDeleted
                | EventType::BeliefRecorded
                | EventType::BeliefSuperseded
                | EventType::AutonomyGranted
                | EventType::AutonomyRevoked
                | EventType::DocumentArchived
                | EventType::DocumentFieldsExtracted
                | EventType::DocumentTextTranscribed
                | EventType::DocumentPurged
                | EventType::DocumentRetentionSet
                | EventType::TaskCreated
                | EventType::TaskModified
                | EventType::TaskStatusSet
                | EventType::TaskTimerStarted
                | EventType::TaskTimerPaused
                | EventType::TaskTimerStopped
                | EventType::TaskProjectCreated
                | EventType::TaskProjectClosed
                | EventType::HubIndexReceived
                | EventType::DeadlineFeedReceived => counted += 1,
            }
        }
        assert_eq!(counted, 66, "EventType::ALL does not list every variant");

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
            corrections: vec![],
            added: vec![],
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
            corrections: vec![],
            added: vec![],
        };
        let json = serde_json::to_value(&payload).unwrap();
        validate_payload(&EventType::AutoImportBatchCommitted, &json).unwrap();
        // fx_rate + fx_commodity should not serialize when None.
        let json_str = serde_json::to_string(&payload).unwrap();
        assert!(!json_str.contains("fx_rate"));
        assert!(!json_str.contains("fx_commodity"));
        assert!(!json_str.contains("corrections"));
    }

    #[test]
    fn auto_import_batch_committed_payload_carries_corrections() {
        let json = serde_json::json!({
            "batch_id": "01HJ...",
            "accepted_indices": [0, 1],
            "corrections": [{
                "index": 1,
                "date": "2026-09-14",
                "description": "Pharmacy",
                "postings": [
                    { "account": "Expenses:Health", "commodity": "CAD", "amount": "12.40" },
                    { "account": "Liabilities:Visa", "commodity": "CAD", "amount": "-12.40" },
                ],
            }],
        });
        validate_payload(&EventType::AutoImportBatchCommitted, &json).unwrap();
        let back: AutoImportBatchCommittedPayload = serde_json::from_value(json).unwrap();
        assert_eq!(back.corrections.len(), 1);
        assert_eq!(back.corrections[0].index, 1);
        assert_eq!(back.corrections[0].postings[0].account, "Expenses:Health");
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
            "description": "Greenfield grocery run",
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
                "filename": "greenfield-2026-05-16.jpg",
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
            "changes": { "description": "Greenfield — corrected" }
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

    /// The four values `FromStr` accepts and the stored form must not.
    ///
    /// Each is a tag that would compare unequal to the one the user meant, and
    /// the symptom is always the same: a filter returning nothing for a document
    /// that visibly carries the tag.
    #[test]
    fn normalize_rejects_what_from_str_waves_through() {
        // Padding and case: accepted by `FromStr` verbatim, folded here.
        assert_eq!(
            Tag::normalize(" Receipt ").unwrap(),
            Tag::Bare("receipt".into())
        );
        assert_eq!(
            Tag::normalize("Institution:RBC").unwrap(),
            Tag::KeyValue {
                key: "institution".into(),
                value: "rbc".into()
            }
        );
        // A half-empty key:value. `FromStr` builds `KeyValue { key: "", .. }`.
        assert!(Tag::normalize(":x").is_err(), "an empty key is not a tag");
        assert!(Tag::normalize("x:").is_err(), "an empty value is not a tag");
        // The set separator. Left in, one tag would round-trip into two.
        assert!(
            Tag::normalize("a,b").is_err(),
            "a tag cannot carry the separator"
        );
        assert!(Tag::normalize("   ").is_err(), "whitespace is not a tag");
    }

    /// `a:b:c` keeps `FromStr`'s first-colon split — the value is allowed to
    /// carry colons, so only the leading key is taken.
    #[test]
    fn normalize_splits_on_the_first_colon_only() {
        assert_eq!(
            Tag::normalize("ref:a:b").unwrap(),
            Tag::KeyValue {
                key: "ref".into(),
                value: "a:b".into()
            }
        );
    }

    #[test]
    fn a_tag_set_is_ordered_and_deduplicated() {
        let set = normalize_tag_set(["Receipt", "groceries", "receipt", " GROCERIES "]).unwrap();
        assert_eq!(encode_tag_set(&set), "groceries,receipt");
    }

    /// One bad tag fails the write. Dropping it would report success for a tag
    /// the user typed and can never find.
    #[test]
    fn a_bad_tag_fails_the_whole_set() {
        assert!(normalize_tag_set(["receipt", "a,b"]).is_err());
    }

    #[test]
    fn a_tag_set_round_trips_through_its_stored_form() {
        let set = normalize_tag_set(["receipt", "institution:rbc"]).unwrap();
        let encoded = encode_tag_set(&set);
        assert_eq!(encoded, "institution:rbc,receipt");
        assert_eq!(decode_tag_set(&encoded), set);
    }

    /// The read half is lenient where the write half is strict: it parses values
    /// already in the log, and an empty segment is not a tag.
    #[test]
    fn decoding_skips_empty_segments_rather_than_inventing_tags() {
        assert_eq!(decode_tag_set(""), vec![]);
        assert_eq!(decode_tag_set(",,"), vec![]);
        assert_eq!(
            decode_tag_set("receipt,,groceries"),
            vec![Tag::Bare("receipt".into()), Tag::Bare("groceries".into())]
        );
    }
}
