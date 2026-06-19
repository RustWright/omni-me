//! LLM provider config route (3.8 — bring-your-own-LLM).
//!
//! `GET /llm/config` returns the current provider selection WITHOUT the secret
//! key (a `has_key` bool stands in for it). `PUT /llm/config` writes the `[llm]`
//! section into `credentials.toml`. Apply model = **restart-to-apply**: the
//! running `AppState.llm_client` was selected at boot, so a change takes effect
//! on the next server start — the LLM provider is a set-once knob, unlike the
//! auto-import sources which apply live.
//!
//! Same unauthenticated MVP posture as the rest of the server (behind Tailscale,
//! per [[project-auth-deferred]]).

use axum::{http::StatusCode, routing::get, Json, Router};
use serde::{Deserialize, Serialize};

use omni_me_core::credentials::{self, LlmProviderConfig};

use crate::AppState;

pub fn llm_routes() -> Router<AppState> {
    Router::new().route("/llm/config", get(get_llm_config).put(put_llm_config))
}

fn internal_err<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Read view — deliberately never includes the api_key, only whether one is set
/// (so the picker can show "key configured" without the secret crossing the
/// wire / landing in a client log).
#[derive(Serialize)]
struct LlmConfigView {
    provider: String,
    base_url: Option<String>,
    model: Option<String>,
    has_key: bool,
}

async fn get_llm_config() -> Result<Json<LlmConfigView>, (StatusCode, String)> {
    let path = credentials::default_path().map_err(internal_err)?;
    let creds = credentials::load(&path).map_err(internal_err)?;
    let view = match creds.llm {
        Some(c) => LlmConfigView {
            provider: c.provider,
            base_url: c.base_url,
            model: c.model,
            has_key: c.api_key.is_some_and(|k| !k.is_empty()),
        },
        // No [llm] section → the engine's default (Gemini).
        None => LlmConfigView {
            provider: "gemini".to_string(),
            base_url: None,
            model: None,
            has_key: false,
        },
    };
    Ok(Json(view))
}

/// Write payload. `api_key` is optional: when omitted/blank AND a key is already
/// stored, the existing key is preserved (the form leaves the field blank to
/// mean "unchanged"); a non-blank value replaces it.
#[derive(Deserialize)]
struct LlmConfigUpdate {
    provider: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
}

async fn put_llm_config(
    Json(update): Json<LlmConfigUpdate>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let path = credentials::default_path().map_err(internal_err)?;
    let mut creds = credentials::load(&path).map_err(internal_err)?;

    // Preserve the existing key when the form leaves api_key blank ("unchanged").
    let existing_key = creds.llm.as_ref().and_then(|c| c.api_key.clone());
    let api_key = match update.api_key {
        Some(k) if !k.is_empty() => Some(k),
        _ => existing_key,
    };

    creds.llm = Some(LlmProviderConfig {
        provider: update.provider,
        base_url: update.base_url.filter(|s| !s.is_empty()),
        model: update.model.filter(|s| !s.is_empty()),
        api_key,
    });
    credentials::save(&path, &creds).map_err(internal_err)?;
    Ok(Json(
        serde_json::json!({ "status": "saved", "applies": "next_restart" }),
    ))
}
