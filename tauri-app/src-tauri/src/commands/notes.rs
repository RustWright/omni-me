use chrono::Utc;
use tauri::State;

use omni_me_core::db::queries::{self, GenericNoteRow, JournalEntryRow};
use omni_me_core::events::{EventStore, EventType, NewEvent};

use crate::AppState;

// -----------------------------------------------------------------------------
// Journal entries (date-keyed, one-per-day)
// -----------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn create_journal_entry(
    state: State<'_, AppState>,
    date: String,
    raw_text: String,
    legacy_properties: Option<serde_json::Value>,
) -> Result<JournalEntryRow, String> {
    tracing::info!(date = %date, len = raw_text.len(), "create_journal_entry");
    let journal_id = ulid::Ulid::new().to_string();

    let mut payload = serde_json::json!({
        "journal_id": journal_id,
        "date": date,
        "raw_text": raw_text,
    });
    if let Some(props) = legacy_properties {
        payload["legacy_properties"] = props;
    }

    append_and_apply(
        &state,
        EventType::JournalEntryCreated,
        journal_id.clone(),
        payload,
    )
    .await?;

    queries::get_journal_by_date(&state.db, &date)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Journal entry created but not found in projection".to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_journal_entry(
    state: State<'_, AppState>,
    journal_id: String,
    raw_text: String,
) -> Result<(), String> {
    tracing::info!(journal_id = %journal_id, len = raw_text.len(), "update_journal_entry");
    let payload = serde_json::json!({
        "journal_id": journal_id,
        "raw_text": raw_text,
    });
    append_and_apply(
        &state,
        EventType::JournalEntryUpdated,
        journal_id,
        payload,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn close_journal_entry(
    state: State<'_, AppState>,
    journal_id: String,
    trigger: String,
) -> Result<(), String> {
    tracing::info!(journal_id = %journal_id, trigger = %trigger, "close_journal_entry");
    let trigger = match trigger.as_str() {
        "manual" | "auto" => trigger,
        _ => return Err(format!("invalid trigger: {trigger} (expected manual|auto)")),
    };
    let payload = serde_json::json!({ "journal_id": journal_id, "trigger": trigger });
    append_and_apply(&state, EventType::JournalEntryClosed, journal_id, payload).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn reopen_journal_entry(
    state: State<'_, AppState>,
    journal_id: String,
) -> Result<(), String> {
    tracing::info!(journal_id = %journal_id, "reopen_journal_entry");
    let payload = serde_json::json!({ "journal_id": journal_id });
    append_and_apply(
        &state,
        EventType::JournalEntryReopened,
        journal_id,
        payload,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_journal_by_date(
    state: State<'_, AppState>,
    date: String,
) -> Result<Option<JournalEntryRow>, String> {
    queries::get_journal_by_date(&state.db, &date)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_journal_entries(
    state: State<'_, AppState>,
) -> Result<Vec<JournalEntryRow>, String> {
    queries::list_journal_entries(&state.db, 100, 0)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_journal_dates(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<Vec<String>, String> {
    queries::list_journal_dates(&state.db, &from_date, &to_date)
        .await
        .map_err(|e| e.to_string())
}

// -----------------------------------------------------------------------------
// Generic notes (id-keyed, user-titled, free-form)
// -----------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn create_generic_note(
    state: State<'_, AppState>,
    title: String,
    raw_text: String,
    legacy_properties: Option<serde_json::Value>,
) -> Result<GenericNoteRow, String> {
    tracing::info!(title = %title, len = raw_text.len(), "create_generic_note");
    let note_id = ulid::Ulid::new().to_string();

    let mut payload = serde_json::json!({
        "note_id": note_id,
        "title": title,
        "raw_text": raw_text,
    });
    if let Some(props) = legacy_properties {
        payload["legacy_properties"] = props;
    }

    append_and_apply(
        &state,
        EventType::GenericNoteCreated,
        note_id.clone(),
        payload,
    )
    .await?;

    queries::get_generic_note(&state.db, &note_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Note created but not found in projection".to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_generic_note(
    state: State<'_, AppState>,
    note_id: String,
    raw_text: String,
) -> Result<(), String> {
    tracing::info!(note_id = %note_id, len = raw_text.len(), "update_generic_note");
    let payload = serde_json::json!({
        "note_id": note_id,
        "raw_text": raw_text,
    });
    append_and_apply(&state, EventType::GenericNoteUpdated, note_id, payload).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn rename_generic_note(
    state: State<'_, AppState>,
    note_id: String,
    title: String,
) -> Result<(), String> {
    tracing::info!(note_id = %note_id, title = %title, "rename_generic_note");
    let payload = serde_json::json!({ "note_id": note_id, "title": title });
    append_and_apply(&state, EventType::GenericNoteRenamed, note_id, payload).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_generic_note(
    state: State<'_, AppState>,
    id: String,
) -> Result<GenericNoteRow, String> {
    queries::get_generic_note(&state.db, &id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Note '{id}' not found"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_generic_notes(
    state: State<'_, AppState>,
) -> Result<Vec<GenericNoteRow>, String> {
    queries::list_generic_notes(&state.db, 100, 0)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn search_generic_notes(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<GenericNoteRow>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    queries::search_generic_notes(&state.db, &query)
        .await
        .map_err(|e| e.to_string())
}

// -----------------------------------------------------------------------------
// LLM processing (routes via aggregate_id — works for either journal or generic)
// -----------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn process_note_llm(
    state: State<'_, AppState>,
    aggregate_id: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(aggregate_id = %aggregate_id, "process_note_llm");

    let raw_text = resolve_raw_text(&state, &aggregate_id)
        .await?
        .ok_or_else(|| format!("Aggregate '{aggregate_id}' not found"))?;

    let server_url = state.server_url.read().await.clone();
    let resp = state
        .http
        .post(format!("{}/notes/{}/process", server_url, aggregate_id))
        .json(&serde_json::json!({
            "raw_text": raw_text,
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

    let sync_client =
        omni_me_core::sync::SyncClient::new(server_url, state.device_id.clone());

    if let Err(warning) = sync_back_after_llm(&sync_client, &state).await {
        if let serde_json::Value::Object(ref mut map) = result {
            map.insert("warnings".to_string(), serde_json::json!([warning]));
        }
    }

    Ok(result)
}

async fn resolve_raw_text(
    state: &AppState,
    aggregate_id: &str,
) -> Result<Option<String>, String> {
    if let Some(journal) = queries::get_journal_by_id(&state.db, aggregate_id)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(Some(journal.raw_text));
    }
    if let Some(note) = queries::get_generic_note(&state.db, aggregate_id)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(Some(note.raw_text));
    }
    Ok(None)
}

async fn sync_back_after_llm(
    sync_client: &omni_me_core::sync::SyncClient,
    state: &AppState,
) -> Result<(), String> {
    let result = sync_client.sync(&state.db).await.map_err(|e| {
        tracing::warn!(error = %e, "sync back after llm failed");
        format!(
            "Sync after llm processing failed, retry syncing manually - original error: {}",
            e
        )
    })?;

    if !result.pulled_events.is_empty() {
        tracing::info!(pulled = result.pulled, "applying pulled events to projections");
        state
            .projections
            .apply_events(&result.pulled_events)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "projection apply after sync failed");
                format!(
                    "Projection apply after sync failed, retry syncing manually - original error: {}",
                    e
                )
            })?;
    }

    tracing::info!(pulled = result.pulled, pushed = result.pushed, "sync complete");
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
