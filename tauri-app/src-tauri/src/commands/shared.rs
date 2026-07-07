use chrono::Utc;

use omni_me_core::events::{Event, EventStore, EventType, NewEvent};

use crate::AppState;

/// Append a pre-built event envelope, fold it through the projection runner, and
/// nudge the push debouncer. The grammar-bearing *create* commands build their
/// envelope through the canonical `NewEvent::{transaction_recorded,
/// journal_created, generic_note_created}` factories (so the record key can't
/// drift from the payload id) and then call this shared tail. Returns the stored
/// event so a caller can read back its generated id.
pub(crate) async fn append_new_and_apply(
    state: &AppState,
    event: NewEvent,
) -> Result<Event, String> {
    let stored = state
        .event_store
        .append(event)
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(std::slice::from_ref(&stored))
        .await
        .map_err(|e| e.to_string())?;

    // Auto-sync (push half): nudge the debounced pusher so this edit propagates
    // without a manual Sync. Previously nothing woke the pusher — the SyncBuffer
    // it subscribes to was never fed — so local edits sat until the user pressed
    // Sync. `trigger()` is a non-blocking notify; the debouncer coalesces a burst
    // of edits into one push after its quiet window. Inbound events arrive via the
    // separate pull scheduler (`sync::PullScheduler`).
    state.push_debouncer.trigger();

    Ok(stored)
}

/// Append a single event and immediately fold it through the projection runner.
/// Used by every non-create command in `notes`/`routines`/`budget` (update /
/// delete / tag / close — simple `{id, changes}` shapes, not grammar-bearing).
/// Create events go through the `NewEvent::*` factories + `append_new_and_apply`.
/// The batched import path in `commands::import` uses `append_batch` directly
/// and intentionally does not go through this helper.
pub(crate) async fn append_and_apply(
    state: &AppState,
    event_type: EventType,
    aggregate_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let event = NewEvent {
        id: None,
        event_type: event_type.to_string(),
        aggregate_id,
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload,
    };
    append_new_and_apply(state, event).await?;
    Ok(())
}
