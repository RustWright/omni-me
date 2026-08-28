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

    let server_token = state.server_token.read().await.clone();
    let client = SyncClient::new(server_url, state.device_id.clone()).with_token(&server_token);

    let result = client.sync(&state.db).await.map_err(|e| {
        tracing::warn!(error = %e, "sync failed");
        e.to_string()
    })?;

    // Apply pulled events through projections so they become visible in the UI.
    // Best-effort: a single bad remote event is logged and skipped, never aborts
    // the batch (the events are durably stored, so a rebuild always recovers).
    if !result.pulled_events.is_empty() {
        tracing::info!(
            pulled = result.pulled,
            "applying pulled events to projections"
        );
        let failed = state
            .projections
            .apply_events_resilient(&result.pulled_events)
            .await;
        if failed > 0 {
            tracing::warn!(
                failed,
                pulled = result.pulled,
                "some pulled events failed to project"
            );
        }
    }

    tracing::info!(
        pulled = result.pulled,
        pushed = result.pushed,
        "sync complete"
    );

    Ok(SyncCommandResult {
        pulled: result.pulled,
        pushed: result.pushed,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_sync_info(state: State<'_, AppState>) -> Result<SyncInfo, String> {
    let server_url = state.server_url.read().await.clone();
    let server_token = state.server_token.read().await.clone();
    Ok(SyncInfo {
        server_url,
        device_id: state.device_id.clone(),
        // Whether a token is set, never the token. Same rule the LLM key
        // already follows (`has_key`): Settings needs to render state, not
        // re-display a secret.
        has_server_token: !server_token.trim().is_empty(),
    })
}

/// Set (or clear) the bearer token this device sends to the box.
///
/// Takes effect immediately for command-driven calls, which read the token per
/// request through `AppState::box_request`. The background sync schedulers hold
/// a `SyncClient` built at startup, so they pick it up on the next launch —
/// exactly the same restart requirement `server_url` already has, and the
/// Settings copy says so.
#[tauri::command(rename_all = "snake_case")]
pub async fn update_server_token(
    state: State<'_, AppState>,
    server_token: String,
) -> Result<(), String> {
    let server_token = server_token.trim().to_string();
    // Logged as a boolean. A token in the log is a token in every pasted
    // bug report.
    tracing::info!(cleared = server_token.is_empty(), "update_server_token");
    let path = state.app_data_dir.join(crate::SERVER_TOKEN_FILE);
    std::fs::write(&path, &server_token).map_err(|e| {
        tracing::warn!(error = %e, "failed to persist server_token");
        e.to_string()
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    *state.server_token.write().await = server_token;
    Ok(())
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
    pub has_server_token: bool,
}
