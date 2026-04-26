use chrono::Utc;

use omni_me_core::events::{EventStore, EventType, NewEvent};

use crate::AppState;

/// Append a single event and immediately fold it through the projection
/// runner. Used by every single-event command in `notes` and `routines`.
/// The batched import path in `commands::import` uses `append_batch` directly
/// and intentionally does not go through this helper.
pub(super) async fn append_and_apply(
    state: &AppState,
    event_type: EventType,
    aggregate_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let event = state
        .event_store
        .append(NewEvent {
            id: None,
            event_type: event_type.to_string(),
            aggregate_id,
            timestamp: Utc::now(),
            device_id: state.device_id.clone(),
            payload,
        })
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(&[event])
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
