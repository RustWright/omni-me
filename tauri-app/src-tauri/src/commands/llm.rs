//! LLM endpoint config command — a thin **read-only** HTTP proxy to
//! `/llm/config` on the sync server. The LLM runs server-side (per
//! [[feedback-llm-server-side]]), so the client only reports what is wired. The
//! api_key is held server-side and never round-trips through the client:
//! `get_llm_config` returns a `has_key` bool, not the key itself.
//!
//! Writing was removed along with the settings form — see the server route.

use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::config::Feature;

use crate::commands::shared::require_feature;

use crate::AppState;

/// Read view of the server's `[llm]` config — the secret key is represented only
/// by `has_key`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfigView {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub has_key: bool,
    /// Whether the endpoint is also opted in to reading documents.
    ///
    /// ⚠️ This was **missing** from this struct, so it never round-tripped: the
    /// server reported it, this view silently dropped it, and the client always
    /// saw `false`. The old settings checkbox therefore rendered unchecked no
    /// matter how the server was configured.
    #[serde(default)]
    pub vision: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_llm_config(state: State<'_, AppState>) -> Result<LlmConfigView, String> {
    require_feature(&state, Feature::Llm)?;

    let resp = state
        .box_request(reqwest::Method::GET, "/llm/config")
        .await
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
