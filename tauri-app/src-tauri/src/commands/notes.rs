use chrono::Utc;
use tauri::State;

use omni_me_core::db::queries::{self, NoteRow};
use omni_me_core::events::{EventStore, EventType, NewEvent};

use crate::AppState;

#[tauri::command(rename_all = "snake_case")]
pub async fn create_note(
    state: State<'_, AppState>,
    raw_text: String,
    date: String,
) -> Result<NoteRow, String> {
    tracing::info!(date = %date, len = raw_text.len(), "create_note");
    let note_id = ulid::Ulid::new().to_string();

    let event = NewEvent {
        id: None,
        event_type: EventType::NoteCreated.to_string(),
        aggregate_id: note_id.clone(),
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload: serde_json::json!({
            "raw_text": raw_text,
            "date": date,
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

    queries::get_note(&state.db, &note_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Note created but not found in projection".to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_notes(state: State<'_, AppState>) -> Result<Vec<NoteRow>, String> {
    tracing::debug!("list_notes");
    queries::list_notes(&state.db, 100, 0).await.map_err(|e| {
        tracing::warn!(error = %e, "list_notes failed");
        e.to_string()
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_note(state: State<'_, AppState>, id: String) -> Result<NoteRow, String> {
    tracing::debug!(id = %id, "get_note");
    queries::get_note(&state.db, &id)
        .await
        .map_err(|e| {
            tracing::warn!(id = %id, error = %e, "get_note failed");
            e.to_string()
        })?
        .ok_or_else(|| format!("Note '{id}' not found"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_note(
    state: State<'_, AppState>,
    id: String,
    raw_text: String,
) -> Result<(), String> {
    tracing::info!(id = %id, len = raw_text.len(), "update_note");
    let event = NewEvent {
        id: None,
        event_type: EventType::NoteUpdated.to_string(),
        aggregate_id: id.clone(),
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload: serde_json::json!({
            "note_id": id,
            "raw_text": raw_text,
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
pub async fn search_notes(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<NoteRow>, String> {
    tracing::debug!(query = %query, "search_notes");
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    queries::search_notes(&state.db, &query).await.map_err(|e| {
        tracing::warn!(query = %query, error = %e, "search_notes failed");
        e.to_string()
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn process_note_llm(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(note_id = %note_id, "process_note_llm");
    let note = queries::get_note(&state.db, &note_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Note '{note_id}' not found"))?;

    // Send to server for LLM processing
    let server_url = state.server_url.read().await.clone();
    let resp = state
        .http
        .post(format!("{}/notes/{}/process", server_url, note_id))
        .json(&serde_json::json!({
            "raw_text": note.raw_text,
            "device_id": state.device_id,
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to reach server: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Server error {status}: {body}"));
    }

    let mut result: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse server response: {e}"))?;

    // Sync the note_llm_processed event back to local DB so projections update.
    // LLM processing succeeded server-side — the event exists on the server.
    // This sync pulls it locally so projections populate tags/summary.
    // Sync failures are returned as warnings, not errors — the LLM result is preserved.
    let sync_client = omni_me_core::sync::SyncClient::new(server_url, state.device_id.clone());

    if let Err(warning) = sync_back_after_llm(&sync_client, &state).await {
        if let serde_json::Value::Object(ref mut map) = result {
            map.insert("warnings".to_string(), serde_json::json!([warning]));
        }
    }

    Ok(result)
}

/// Sync events from the server after LLM processing so local projections update.
/// Returns Ok(()) on success, or an informative error if sync fails.
async fn sync_back_after_llm(
    sync_client: &omni_me_core::sync::SyncClient,
    state: &AppState,
) -> Result<(), String> {
    let result = sync_client.sync(&state.db).await.map_err(|e| {
        tracing::warn!(error = %e, "sync back after llm failed");
        format!(
            "Sync after llm processing failed, retry syncing manually - original error: {}",
            e.to_string()
        )
    })?;

    // Apply pulled events through projections so they become visible in the UI
    if !result.pulled_events.is_empty() {
        tracing::info!(
            pulled = result.pulled,
            "applying pulled events to projections"
        );
        state
            .projections
            .apply_events(&result.pulled_events)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "projection apply after sync failed");
                format!(
                    "Projection apply after sync failed, retry syncing manually - original error: {}",
                    e.to_string())
            })?;
    }

    tracing::info!(
        pulled = result.pulled,
        pushed = result.pushed,
        "sync complete"
    );

    Ok(())
}
