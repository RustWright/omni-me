//! LLM endpoint config route — **read-only**.
//!
//! `GET /llm/config` reports the current `[llm]` selection WITHOUT the secret
//! key (a `has_key` bool stands in for it), so a client can answer "is a model
//! wired, and which one".
//!
//! ⚠️ **There is deliberately no write route.** A `PUT` existed and wrote the
//! `[llm]` section — including an API key — into `credentials.toml` over an
//! endpoint with the same unauthenticated posture as the rest of the server
//! (behind Tailscale, per [[project-auth-deferred]]). It was removed with the
//! settings form that called it: which endpoint the engine talks to is
//! deployment configuration, it is restart-to-apply rather than live, and model
//! selection is becoming per-role and benchmark-driven, so a single
//! user-editable `model` field would soon have no well-defined meaning. Edit
//! `credentials.toml` on the host instead.

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
