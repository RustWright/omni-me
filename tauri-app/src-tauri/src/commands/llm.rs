//! LLM provider config commands (3.8 bring-your-own-LLM) — thin HTTP proxies to
//! `/llm/config` on the sync server. The LLM runs server-side (per
//! [[feedback-llm-server-side]]), so the client only relays the provider
//! selection. The api_key is written + held server-side and never round-trips
//! back through the client: `get_llm_config` returns a `has_key` bool, not the
//! key itself.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

/// Read view of the server's `[llm]` config — the secret key is represented only
/// by `has_key`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfigView {
    pub provider: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub has_key: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_llm_config(state: State<'_, AppState>) -> Result<LlmConfigView, String> {
    let server_url = state.server_url.read().await.clone();
    let url = format!("{}/llm/config", server_url.trim_end_matches('/'));
    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("llm config fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("llm config: server returned {}", resp.status()));
    }
    resp.json::<LlmConfigView>()
        .await
        .map_err(|e| format!("llm config decode: {e}"))
}

/// `PUT /llm/config`. The frontend assembles the body
/// (`{provider, base_url?, model?, api_key?}`) as untyped JSON — leaving
/// `api_key` blank/absent preserves the stored key server-side.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_llm_config(
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let server_url = state.server_url.read().await.clone();
    let url = format!("{}/llm/config", server_url.trim_end_matches('/'));
    let resp = state
        .http
        .put(&url)
        .json(&config)
        .send()
        .await
        .map_err(|e| format!("set llm config: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("set llm config body: {e}"))?;
    if !status.is_success() {
        return Err(format!("server returned {status}: {body}"));
    }
    serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|e| format!("set llm config decode: {e}"))
}
