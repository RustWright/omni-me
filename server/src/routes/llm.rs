//! `GET /llm/config`: the current `[llm]` selection, with `has_key` in place of the key.
//! No write route on purpose: the endpoint is deployment config, restart-to-apply, and set
//! per role; edit `credentials.toml` on the host.

use axum::{Json, Router, http::StatusCode, routing::get};
use serde::Serialize;

use omni_me_core::credentials;

use crate::AppState;

pub fn llm_routes() -> Router<AppState> {
    Router::new().route("/llm/config", get(get_llm_config))
}

fn internal_err<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Read view — deliberately never includes the api_key, only whether one is set
/// (so the picker can show "key configured" without the secret crossing the
/// wire / landing in a client log).
#[derive(Serialize)]
struct LlmConfigView {
    base_url: Option<String>,
    model: Option<String>,
    has_key: bool,
    /// 3.8a: also route the document extractor through this endpoint's vision API.
    vision: bool,
}

async fn get_llm_config() -> Result<Json<LlmConfigView>, (StatusCode, String)> {
    let path = credentials::default_path().map_err(internal_err)?;
    let creds = credentials::load(&path).map_err(internal_err)?;
    let view = match creds.llm {
        Some(c) => LlmConfigView {
            base_url: c.base_url,
            model: c.model,
            has_key: c.api_key.is_some_and(|k| !k.is_empty()),
            vision: c.vision,
        },
        // No [llm] section → nothing configured. The form shows empty fields
        // rather than a provider the engine can no longer build.
        None => LlmConfigView {
            base_url: None,
            model: None,
            has_key: false,
            vision: false,
        },
    };
    Ok(Json(view))
}
