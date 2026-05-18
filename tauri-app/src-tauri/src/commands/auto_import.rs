//! Auto-import observability commands (Phase 3.9).
//!
//! These are pure HTTP proxies to `/auto_import/{status,tick}` on the sync
//! server — auto-import lives server-side (per [[feedback-llm-server-side]]),
//! so the Tauri client has no in-process registry to query.
//!
//! Errors from the server are returned as-is so the Settings panel can
//! surface them inline ("server returned 502: wise upstream error").

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoImportSourceView {
    pub name: String,
    pub last_tick_at: Option<String>,
    pub last_outcome: serde_json::Value,
    pub interval_secs: u64,
    pub health: String,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_auto_import_sources(
    state: State<'_, AppState>,
) -> Result<Vec<AutoImportSourceView>, String> {
    let server_url = state.server_url.read().await.clone();
    let url = format!(
        "{}/auto_import/status",
        server_url.trim_end_matches('/')
    );
    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("auto-import status fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "auto-import status: server returned {}",
            resp.status()
        ));
    }
    resp.json::<Vec<AutoImportSourceView>>()
        .await
        .map_err(|e| format!("auto-import status decode: {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickResponse {
    pub events_appended: usize,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn trigger_auto_import_tick(
    state: State<'_, AppState>,
    source: String,
) -> Result<TickResponse, String> {
    let server_url = state.server_url.read().await.clone();
    let url = format!("{}/auto_import/tick", server_url.trim_end_matches('/'));
    let resp = state
        .http
        .post(&url)
        .query(&[("source", source.as_str())])
        .send()
        .await
        .map_err(|e| format!("auto-import tick: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("auto-import tick body: {e}"))?;
    if !status.is_success() {
        return Err(format!("server returned {status}: {body}"));
    }
    serde_json::from_str::<TickResponse>(&body)
        .map_err(|e| format!("auto-import tick decode: {e}"))
}
