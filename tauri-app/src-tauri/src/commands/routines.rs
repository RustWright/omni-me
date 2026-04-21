use chrono::Utc;
use tauri::State;

use omni_me_core::db::queries::{self, CompletionRow, RoutineGroupRow, RoutineItemRow};
use omni_me_core::events::{EventStore, EventType, NewEvent};

use crate::AppState;

// -----------------------------------------------------------------------------
// Group CRUD
// -----------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn create_routine_group(
    state: State<'_, AppState>,
    name: String,
    frequency: String,
    order: u32,
) -> Result<RoutineGroupRow, String> {
    tracing::info!(name = %name, frequency = %frequency, order, "create_routine_group");

    // Validate frequency up-front — sync accepts it regardless, but we want
    // the UI to get a fast, structured error for bad input.
    frequency
        .parse::<omni_me_core::routines::Frequency>()
        .map_err(|e| format!("invalid frequency '{frequency}': {e}"))?;

    let group_id = ulid::Ulid::new().to_string();
    let payload = serde_json::json!({
        "name": name,
        "frequency": frequency,
        "order": order,
    });
    append_and_apply(
        &state,
        EventType::RoutineGroupCreated,
        group_id.clone(),
        payload,
    )
    .await?;

    queries::get_routine_group(&state.db, &group_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Group created but not found in projection".to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_routine_groups(
    state: State<'_, AppState>,
) -> Result<Vec<RoutineGroupRow>, String> {
    queries::list_routine_groups(&state.db)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn reorder_routine_groups(
    state: State<'_, AppState>,
    orderings: Vec<serde_json::Value>,
) -> Result<(), String> {
    tracing::info!(count = orderings.len(), "reorder_routine_groups");
    let payload = serde_json::json!({ "orderings": orderings });
    // Aggregate id here is synthetic — reorder touches many groups; use a ULID
    // so the event has its own identity and sync idempotency holds.
    let aggregate_id = ulid::Ulid::new().to_string();
    append_and_apply(
        &state,
        EventType::RoutineGroupReordered,
        aggregate_id,
        payload,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn remove_routine_group(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<(), String> {
    tracing::info!(group_id = %group_id, "remove_routine_group");
    let payload = serde_json::json!({ "group_id": group_id });
    append_and_apply(
        &state,
        EventType::RoutineGroupRemoved,
        group_id,
        payload,
    )
    .await
}

// -----------------------------------------------------------------------------
// Item CRUD
// -----------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn add_routine_item(
    state: State<'_, AppState>,
    group_id: String,
    name: String,
    duration_min: u32,
    order: u32,
) -> Result<RoutineItemRow, String> {
    tracing::info!(group_id = %group_id, name = %name, "add_routine_item");
    let item_id = ulid::Ulid::new().to_string();
    let payload = serde_json::json!({
        "group_id": group_id,
        "name": name,
        "estimated_duration_min": duration_min,
        "order": order,
    });
    append_and_apply(
        &state,
        EventType::RoutineItemAdded,
        item_id.clone(),
        payload,
    )
    .await?;

    queries::list_routine_items(&state.db, &group_id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|i| i.id == item_id)
        .ok_or_else(|| "Item created but not found in projection".to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_routine_items(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<Vec<RoutineItemRow>, String> {
    queries::list_routine_items(&state.db, &group_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn modify_routine_item(
    state: State<'_, AppState>,
    item_id: String,
    changes: serde_json::Value,
) -> Result<(), String> {
    tracing::info!(item_id = %item_id, "modify_routine_item");
    let payload = serde_json::json!({ "item_id": item_id, "changes": changes });
    append_and_apply(&state, EventType::RoutineItemModified, item_id, payload).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn remove_routine_item(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<(), String> {
    tracing::info!(item_id = %item_id, "remove_routine_item");
    let payload = serde_json::json!({ "item_id": item_id });
    append_and_apply(&state, EventType::RoutineItemRemoved, item_id, payload).await
}

// -----------------------------------------------------------------------------
// Completion events (and their undos)
// -----------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn complete_routine_item(
    state: State<'_, AppState>,
    item_id: String,
    group_id: String,
    date: String,
) -> Result<(), String> {
    tracing::info!(item_id = %item_id, date = %date, "complete_routine_item");
    let payload = serde_json::json!({
        "item_id": item_id,
        "group_id": group_id,
        "date": date,
        "completed_at": Utc::now().to_rfc3339(),
    });
    let aggregate_id = ulid::Ulid::new().to_string();
    append_and_apply(
        &state,
        EventType::RoutineItemCompleted,
        aggregate_id,
        payload,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn undo_completion(
    state: State<'_, AppState>,
    item_id: String,
    date: String,
) -> Result<(), String> {
    tracing::info!(item_id = %item_id, date = %date, "undo_completion");
    let payload = serde_json::json!({ "item_id": item_id, "date": date });
    let aggregate_id = ulid::Ulid::new().to_string();
    append_and_apply(
        &state,
        EventType::RoutineItemCompletionUndone,
        aggregate_id,
        payload,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn skip_routine_item(
    state: State<'_, AppState>,
    item_id: String,
    group_id: String,
    date: String,
    reason: Option<String>,
) -> Result<(), String> {
    tracing::info!(item_id = %item_id, date = %date, "skip_routine_item");
    let payload = serde_json::json!({
        "item_id": item_id,
        "group_id": group_id,
        "date": date,
        "reason": reason,
    });
    let aggregate_id = ulid::Ulid::new().to_string();
    append_and_apply(
        &state,
        EventType::RoutineItemSkipped,
        aggregate_id,
        payload,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn undo_skip(
    state: State<'_, AppState>,
    item_id: String,
    date: String,
) -> Result<(), String> {
    tracing::info!(item_id = %item_id, date = %date, "undo_skip");
    let payload = serde_json::json!({ "item_id": item_id, "date": date });
    let aggregate_id = ulid::Ulid::new().to_string();
    append_and_apply(
        &state,
        EventType::RoutineItemSkipUndone,
        aggregate_id,
        payload,
    )
    .await
}

// -----------------------------------------------------------------------------
// Queries
// -----------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn get_completions_for_date(
    state: State<'_, AppState>,
    group_id: String,
    date: String,
) -> Result<Vec<CompletionRow>, String> {
    queries::get_completions_for_date(&state.db, &group_id, &date)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_routine_history(
    state: State<'_, AppState>,
    group_id: String,
    days: u32,
) -> Result<Vec<CompletionRow>, String> {
    queries::get_completion_history(&state.db, &group_id, days)
        .await
        .map_err(|e| e.to_string())
}

// -----------------------------------------------------------------------------
// Destructive: emit DataWiped + clear events + rebuild projections.
// -----------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn wipe_all_data(state: State<'_, AppState>) -> Result<(), String> {
    tracing::warn!("wipe_all_data invoked");

    // Local-only wipe (Option A): purge this device's event log + projections.
    // Peers are unaffected. `DataWiped` remains an audit-only event with no
    // projection handler — it records that the wipe happened on this device.
    state
        .event_store
        .purge_all()
        .await
        .map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "initiated_at": Utc::now().to_rfc3339(),
        "device_id": state.device_id,
    });
    state
        .event_store
        .append(NewEvent {
            id: None,
            event_type: EventType::DataWiped.to_string(),
            aggregate_id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            device_id: state.device_id.clone(),
            payload,
        })
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .rebuild()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// -----------------------------------------------------------------------------
// Shared helper: append + apply through projections.
// -----------------------------------------------------------------------------

async fn append_and_apply(
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
