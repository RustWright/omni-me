use tauri::State;

use omni_me_core::sync::{SyncClient, SyncStatusSnapshot};

use crate::AppState;

#[tauri::command(rename_all = "snake_case")]
pub async fn trigger_sync(state: State<'_, AppState>) -> Result<SyncCommandResult, String> {
    let server_url = state.server_url.read().await.clone();
    tracing::info!(
        server_url = %server_url,
        device_id = %state.device_id,
        "trigger_sync"
    );

    let client = SyncClient::new(server_url, state.device_id.clone());

    let result = client.sync(&state.db).await.map_err(|e| {
        tracing::warn!(error = %e, "sync failed");
        e.to_string()
    })?;

    // Apply pulled events through projections so they become visible in the UI
    if !result.pulled_events.is_empty() {
        tracing::info!(pulled = result.pulled, "applying pulled events to projections");
        state.projections.apply_events(&result.pulled_events).await.map_err(|e| {
            tracing::warn!(error = %e, "projection apply after sync failed");
            e.to_string()
        })?;
    }

    tracing::info!(pulled = result.pulled, pushed = result.pushed, "sync complete");

    Ok(SyncCommandResult {
        pulled: result.pulled,
        pushed: result.pushed,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_sync_info(state: State<'_, AppState>) -> Result<SyncInfo, String> {
    let server_url = state.server_url.read().await.clone();
    Ok(SyncInfo {
        server_url,
        device_id: state.device_id.clone(),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_server_url(
    state: State<'_, AppState>,
    server_url: String,
) -> Result<(), String> {
    tracing::info!(new_url = %server_url, "update_server_url");
    let _ = tauri::Url::parse(&server_url).map_err(|e| format!("invalid URL: {e}"))?;
    let path = state.app_data_dir.join(crate::SERVER_URL_FILE);
    std::fs::write(&path, &server_url).map_err(|e| {
        tracing::warn!(error = %e, "failed to persist server_url");
        e.to_string()
    })?;
    *state.server_url.write().await = server_url;
    Ok(())
}

/// Return the current aggregated sync status — one of `idle | syncing |
/// retrying | error` (kebab-case) plus retry attempt + last error.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatusSnapshot, String> {
    Ok(state.status_reporter.snapshot().await)
}

#[derive(serde::Serialize)]
pub struct SyncCommandResult {
    pub pulled: usize,
    pub pushed: usize,
}

#[derive(serde::Serialize)]
pub struct SyncInfo {
    pub server_url: String,
    pub device_id: String,
}
