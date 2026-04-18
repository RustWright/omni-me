use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// All known event types in the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType {
    NoteCreated,
    NoteUpdated,
    NoteLlmProcessed,
    RoutineGroupCreated,
    RoutineItemAdded,
    RoutineItemCompleted,
    RoutineItemSkipped,
    RoutineGroupModified,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            EventType::NoteCreated => "note_created",
            EventType::NoteUpdated => "note_updated",
            EventType::NoteLlmProcessed => "note_llm_processed",
            EventType::RoutineGroupCreated => "routine_group_created",
            EventType::RoutineItemAdded => "routine_item_added",
            EventType::RoutineItemCompleted => "routine_item_completed",
            EventType::RoutineItemSkipped => "routine_item_skipped",
            EventType::RoutineGroupModified => "routine_group_modified",
        };
        write!(f, "{s}")
    }
}

impl FromStr for EventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "note_created" => Ok(EventType::NoteCreated),
            "note_updated" => Ok(EventType::NoteUpdated),
            "note_llm_processed" => Ok(EventType::NoteLlmProcessed),
            "routine_group_created" => Ok(EventType::RoutineGroupCreated),
            "routine_item_added" => Ok(EventType::RoutineItemAdded),
            "routine_item_completed" => Ok(EventType::RoutineItemCompleted),
            "routine_item_skipped" => Ok(EventType::RoutineItemSkipped),
            "routine_group_modified" => Ok(EventType::RoutineGroupModified),
            other => Err(format!("unknown event type: {other}")),
        }
    }
}

// --- Typed payload structs ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteCreatedPayload {
    pub raw_text: String,
    pub date: chrono::NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteUpdatedPayload {
    pub note_id: String,
    pub raw_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteLlmProcessedPayload {
    pub note_id: String,
    pub prompt_version: String,
    pub model: String,
    pub derived: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineGroupCreatedPayload {
    pub name: String,
    pub frequency: String,
    pub time_of_day: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemAddedPayload {
    pub group_id: String,
    pub name: String,
    pub estimated_duration_min: u32,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemCompletedPayload {
    pub item_id: String,
    pub group_id: String,
    pub date: chrono::NaiveDate,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineItemSkippedPayload {
    pub item_id: String,
    pub group_id: String,
    pub date: chrono::NaiveDate,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineGroupModifiedPayload {
    pub group_id: String,
    pub changes: serde_json::Value,
    pub justification: Option<String>,
}

/// Validate that a payload JSON value matches the expected shape for the given event type.
pub fn validate_payload(
    event_type: &EventType,
    payload: &serde_json::Value,
) -> Result<(), super::store::EventError> {
    let result = match event_type {
        EventType::NoteCreated => {
            serde_json::from_value::<NoteCreatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::NoteUpdated => {
            serde_json::from_value::<NoteUpdatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::NoteLlmProcessed => {
            serde_json::from_value::<NoteLlmProcessedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineGroupCreated => {
            serde_json::from_value::<RoutineGroupCreatedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemAdded => {
            serde_json::from_value::<RoutineItemAddedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemCompleted => {
            serde_json::from_value::<RoutineItemCompletedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineItemSkipped => {
            serde_json::from_value::<RoutineItemSkippedPayload>(payload.clone()).map(|_| ())
        }
        EventType::RoutineGroupModified => {
            serde_json::from_value::<RoutineGroupModifiedPayload>(payload.clone()).map(|_| ())
        }
    };

    result.map_err(|e| {
        super::store::EventError::Validation(format!(
            "invalid payload for {event_type}: {e}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_display_roundtrip() {
        let types = [
            EventType::NoteCreated,
            EventType::NoteUpdated,
            EventType::NoteLlmProcessed,
            EventType::RoutineGroupCreated,
            EventType::RoutineItemAdded,
            EventType::RoutineItemCompleted,
            EventType::RoutineItemSkipped,
            EventType::RoutineGroupModified,
        ];

        for t in &types {
            let s = t.to_string();
            let parsed: EventType = s.parse().unwrap();
            assert_eq!(&parsed, t);
        }
    }

    #[test]
    fn unknown_event_type_errors() {
        let result = "unknown_type".parse::<EventType>();
        assert!(result.is_err());
    }

    #[test]
    fn validate_note_created_ok() {
        let payload = serde_json::json!({
            "raw_text": "Hello world",
            "date": "2026-03-27"
        });
        assert!(validate_payload(&EventType::NoteCreated, &payload).is_ok());
    }

    #[test]
    fn validate_note_created_missing_field() {
        let payload = serde_json::json!({
            "raw_text": "Hello world"
        });
        assert!(validate_payload(&EventType::NoteCreated, &payload).is_err());
    }

    #[test]
    fn validate_routine_item_completed() {
        let payload = serde_json::json!({
            "item_id": "item-1",
            "group_id": "group-1",
            "date": "2026-03-27",
            "completed_at": "2026-03-27T10:00:00Z"
        });
        assert!(validate_payload(&EventType::RoutineItemCompleted, &payload).is_ok());
    }

    /// Locks in the payload schema for every event type the existing tests
    /// didn't cover. A failure here means someone changed a payload struct
    /// (added a required field, renamed a field) without updating callers.
    #[test]
    fn validate_ok_for_remaining_event_types() {
        let cases: Vec<(EventType, serde_json::Value)> = vec![
            (
                EventType::NoteUpdated,
                serde_json::json!({
                    "note_id": "note-1",
                    "raw_text": "updated text"
                }),
            ),
            (
                EventType::NoteLlmProcessed,
                serde_json::json!({
                    "note_id": "note-1",
                    "prompt_version": "v1",
                    "model": "gemini-flash",
                    "derived": { "tags": ["focus"] }
                }),
            ),
            (
                EventType::RoutineGroupCreated,
                serde_json::json!({
                    "name": "Morning",
                    "frequency": "daily",
                    "time_of_day": "morning"
                }),
            ),
            (
                EventType::RoutineItemAdded,
                serde_json::json!({
                    "group_id": "group-1",
                    "name": "Stretch",
                    "estimated_duration_min": 5,
                    "order": 0
                }),
            ),
        ];

        for (event_type, payload) in &cases {
            validate_payload(event_type, payload).unwrap_or_else(|e| {
                panic!("expected valid payload for {event_type}, got: {e:?}")
            });
        }
    }

    /// `RoutineItemSkipped.reason` is typed as `Option<String>` — the sync
    /// gate must accept skip events both with and without a reason. Locks in
    /// the "reason is optional" schema choice.
    #[test]
    fn validate_routine_item_skipped_reason_is_optional() {
        let with_reason = serde_json::json!({
            "item_id": "item-1",
            "group_id": "group-1",
            "date": "2026-03-27",
            "reason": "traveling"
        });
        assert!(validate_payload(&EventType::RoutineItemSkipped, &with_reason).is_ok());

        let without_reason = serde_json::json!({
            "item_id": "item-1",
            "group_id": "group-1",
            "date": "2026-03-27"
        });
        assert!(
            validate_payload(&EventType::RoutineItemSkipped, &without_reason).is_ok(),
            "reason must stay optional — Option<String> defaults to None when missing"
        );
    }

    /// `RoutineGroupModified.justification` is `Option<String>` — modify events
    /// must accept both user-supplied justifications and silent modifications
    /// (e.g., migrations). Locks in the "justification is optional" schema.
    #[test]
    fn validate_routine_group_modified_justification_is_optional() {
        let with_justification = serde_json::json!({
            "group_id": "group-1",
            "changes": { "name": "Evening" },
            "justification": "renamed for clarity"
        });
        assert!(
            validate_payload(&EventType::RoutineGroupModified, &with_justification).is_ok()
        );

        let without_justification = serde_json::json!({
            "group_id": "group-1",
            "changes": { "name": "Evening" }
        });
        assert!(
            validate_payload(&EventType::RoutineGroupModified, &without_justification).is_ok(),
            "justification must stay optional — Option<String> defaults to None when missing"
        );
    }
}
