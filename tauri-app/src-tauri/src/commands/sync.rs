use tauri::State;

use omni_me_core::sync::SyncClient;

use crate::AppState;

#[tauri::command(rename_all = "snake_case")]
pub async fn trigger_sync(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    tracing::info!(
        server_url = %state.server_url,
        device_id = %state.device_id,
        "trigger_sync"
    );

    let client = SyncClient::new(state.server_url.clone(), state.device_id.clone());

    let result = client.sync(&state.db).await.map_err(|e| {
        tracing::warn!(error = %e, "sync failed");
        e.to_string()
    })?;

    tracing::info!(pulled = result.pulled, pushed = result.pushed, "sync complete");

    Ok(SyncStatus {
        pulled: result.pulled,
        pushed: result.pushed,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_sync_info(state: State<'_, AppState>) -> Result<SyncInfo, String> {
    Ok(SyncInfo {
        server_url: state.server_url.clone(),
        device_id: state.device_id.clone(),
    })
}

#[derive(serde::Serialize)]
pub struct SyncStatus {
    pub pulled: usize,
    pub pushed: usize,
}

#[derive(serde::Serialize)]
pub struct SyncInfo {
    pub server_url: String,
    pub device_id: String,
}
