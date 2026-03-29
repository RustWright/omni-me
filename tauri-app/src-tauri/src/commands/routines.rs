use chrono::Utc;
use tauri::State;

use omni_me_core::db::queries::{self, CompletionRow, RoutineGroupRow, RoutineItemRow};
use omni_me_core::events::{EventStore, EventType, NewEvent};

use crate::AppState;

#[tauri::command(rename_all = "snake_case")]
pub async fn create_routine_group(
    state: State<'_, AppState>,
    name: String,
    frequency: String,
    time_of_day: String,
) -> Result<RoutineGroupRow, String> {
    tracing::info!(name = %name, frequency = %frequency, time_of_day = %time_of_day, "create_routine_group");
    let group_id = ulid::Ulid::new().to_string();

    let event = NewEvent {
        id: None,
        event_type: EventType::RoutineGroupCreated.to_string(),
        aggregate_id: group_id.clone(),
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload: serde_json::json!({
            "name": name,
            "frequency": frequency,
            "time_of_day": time_of_day,
        }),
    };

    let event = state
        .event_store
        .append(event)
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(&[event])
        .await
        .map_err(|e| e.to_string())?;

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
pub async fn add_routine_item(
    state: State<'_, AppState>,
    group_id: String,
    name: String,
    duration_min: u32,
    order: u32,
) -> Result<RoutineItemRow, String> {
    tracing::info!(group_id = %group_id, name = %name, "add_routine_item");
    let item_id = ulid::Ulid::new().to_string();

    let event = NewEvent {
        id: None,
        event_type: EventType::RoutineItemAdded.to_string(),
        aggregate_id: item_id.clone(),
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload: serde_json::json!({
            "group_id": group_id,
            "name": name,
            "estimated_duration_min": duration_min,
            "order": order,
        }),
    };

    let event = state
        .event_store
        .append(event)
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(&[event])
        .await
        .map_err(|e| e.to_string())?;

    // Query the item back from projection
    let items = queries::list_routine_items(&state.db, &group_id)
        .await
        .map_err(|e| e.to_string())?;

    items
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
pub async fn complete_routine_item(
    state: State<'_, AppState>,
    item_id: String,
    group_id: String,
    date: String,
) -> Result<(), String> {
    tracing::info!(item_id = %item_id, group_id = %group_id, date = %date, "complete_routine_item");
    let event = NewEvent {
        id: None,
        event_type: EventType::RoutineItemCompleted.to_string(),
        aggregate_id: ulid::Ulid::new().to_string(),
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload: serde_json::json!({
            "item_id": item_id,
            "group_id": group_id,
            "date": date,
            "completed_at": Utc::now().to_rfc3339(),
        }),
    };

    let event = state
        .event_store
        .append(event)
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(&[event])
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn skip_routine_item(
    state: State<'_, AppState>,
    item_id: String,
    group_id: String,
    date: String,
    reason: Option<String>,
) -> Result<(), String> {
    tracing::info!(item_id = %item_id, group_id = %group_id, date = %date, "skip_routine_item");
    let event = NewEvent {
        id: None,
        event_type: EventType::RoutineItemSkipped.to_string(),
        aggregate_id: ulid::Ulid::new().to_string(),
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload: serde_json::json!({
            "item_id": item_id,
            "group_id": group_id,
            "date": date,
            "reason": reason,
        }),
    };

    let event = state
        .event_store
        .append(event)
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(&[event])
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn modify_routine_group(
    state: State<'_, AppState>,
    group_id: String,
    changes: serde_json::Value,
    justification: Option<String>,
) -> Result<(), String> {
    tracing::info!(group_id = %group_id, "modify_routine_group");
    let event = NewEvent {
        id: None,
        event_type: EventType::RoutineGroupModified.to_string(),
        aggregate_id: group_id.clone(),
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload: serde_json::json!({
            "group_id": group_id,
            "changes": changes,
            "justification": justification,
        }),
    };

    let event = state
        .event_store
        .append(event)
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(&[event])
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

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
