use serde::{Deserialize, Serialize};
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
            EventType::DataWiped,
        ];

        for t in &types {
            let s = t.to_string();
            let parsed: EventType = s.parse().unwrap();
            assert_eq!(&parsed, t);
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
    fn validate_data_wiped() {
        let payload = serde_json::json!({
            "initiated_at": "2026-04-19T12:00:00Z",
            "device_id": "device-a"
        });
        assert!(validate_payload(&EventType::DataWiped, &payload).is_ok());
    }
}
