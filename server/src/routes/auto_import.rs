//! Auto-import observability + manual-trigger routes (Phase 3.9).
//!
//! `GET /auto_import/status` returns the full registry snapshot — used by the
//! Settings panel to render per-source health badges.
//!
//! `POST /auto_import/tick` triggers an out-of-band tick for one source —
//! used by the Settings "Fetch now" button.
//!
//! Both routes are unauthenticated, matching the rest of the server's MVP
//! posture (per [[project-auth-deferred]] — sync endpoints have the same
//! shape and the server only runs behind Tailscale).

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};

use omni_me_core::auto_import::config::{self, SourceDef};
use omni_me_core::auto_import_scheduler::{
    ReauthOutcome, SourceStatus, classify_source_health, SourceHealth,
};

use crate::AppState;

pub fn auto_import_routes() -> Router<AppState> {
    Router::new()
        .route("/auto_import/status", get(status_handler))
        .route("/auto_import/tick", post(tick_handler))
        .route("/auto_import/reauth", post(reauth_handler))
        // Source-definition CRUD (3.7 + live fast-follow). These persist to the
        // server-side `sources.toml` AND apply live: add/edit (re)builds + spawns
        // the source into the running registry, remove aborts its task — no
        // restart required.
        .route(
            "/auto_import/sources",
            get(list_sources_handler).post(add_source_handler),
        )
        .route("/auto_import/sources/{name}", delete(remove_source_handler))
}

#[derive(Serialize)]
struct SourceStatusView {
    #[serde(flatten)]
    status: SourceStatus,
    /// `classify_source_health` result computed server-side so all clients
    /// see the same colour without re-deriving the policy.
    health: SourceHealth,
}

async fn status_handler(State(state): State<AppState>) -> Json<Vec<SourceStatusView>> {
    let snapshot = state.auto_import_registry.snapshot().await;
    let now = chrono::Utc::now();
    let mut views: Vec<SourceStatusView> = snapshot
        .into_iter()
        .map(|s| {
            let health = classify_source_health(&s, now);
            SourceStatusView { status: s, health }
        })
        .collect();
    views.sort_by(|a, b| a.status.name.cmp(&b.status.name));
    Json(views)
}

#[derive(Deserialize)]
struct TickQuery {
    source: String,
}

#[derive(Serialize)]
struct TickResponseOk {
    events_appended: usize,
}

async fn tick_handler(
    State(state): State<AppState>,
    Query(q): Query<TickQuery>,
) -> Result<Json<TickResponseOk>, (StatusCode, String)> {
    match state.auto_import_registry.trigger_manual(&q.source).await {
        Ok(summary) => Ok(Json(TickResponseOk {
            events_appended: summary.events_appended,
        })),
        Err(omni_me_core::auto_import_scheduler::ImportError::NotConfigured(msg)) => {
            Err((StatusCode::NOT_FOUND, msg))
        }
        Err(e) => Err((StatusCode::BAD_GATEWAY, e.to_string())),
    }
}

impl IntoResponse for TickResponseOk {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}

/// `POST /auto_import/reauth` — drive interactive re-auth for one source. The
/// OTP lives in the JSON **body** (not the query string) so it never lands in
/// access logs. The response is the `ReauthOutcome` verbatim
/// (`{"status":"active"|"invalid_otp"|"not_supported"|"error",…}`); only an
/// unknown source name is a transport error (404).
#[derive(Deserialize)]
struct ReauthRequest {
    source: String,
    otp: String,
}

async fn reauth_handler(
    State(state): State<AppState>,
    Json(req): Json<ReauthRequest>,
) -> Result<Json<ReauthOutcome>, (StatusCode, String)> {
    match state
        .auto_import_registry
        .reauth(&req.source, &req.otp)
        .await
    {
        Ok(outcome) => Ok(Json(outcome)),
        Err(omni_me_core::auto_import_scheduler::ImportError::NotConfigured(msg)) => {
            Err((StatusCode::NOT_FOUND, msg))
        }
        Err(e) => Err((StatusCode::BAD_GATEWAY, e.to_string())),
    }
}

// =============================================================================
// Source-definition CRUD (3.7) — persist + apply live
// =============================================================================
//
// These persist to the server-side `sources.toml` AND mutate the running
// registry: add/edit builds the source and (re)spawns it, remove aborts its
// task — the change takes effect immediately, no restart. Single-user-behind-
// Tailscale posture means the load-modify-save is unguarded against concurrent
// writers (acceptable per [[project-auth-deferred]]); `config::save` is itself
// atomic (temp + rename).

fn internal_err<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// `GET /auto_import/sources` — the *configured* source definitions (distinct
/// from `/status`'s *running* snapshot). Drives the Settings management list.
async fn list_sources_handler() -> Result<Json<Vec<SourceDef>>, (StatusCode, String)> {
    let path = config::default_path().map_err(internal_err)?;
    let cfg = config::load(&path).map_err(internal_err)?;
    Ok(Json(cfg.sources))
}

/// `POST /auto_import/sources` — add or replace a definition (keyed by name).
/// Rejected with 400 if the definition is invalid (missing required fields /
/// unknown type) so a bad config never reaches the file.
async fn add_source_handler(
    State(state): State<AppState>,
    Json(def): Json<SourceDef>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    config::validate(&def).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let path = config::default_path().map_err(internal_err)?;
    let mut cfg = config::load(&path).map_err(internal_err)?;
    // Upsert by name (edit = replace) in the on-disk definitions.
    cfg.sources.retain(|s| s.name != def.name);
    cfg.sources.push(def.clone());
    config::save(&path, &cfg).map_err(internal_err)?;

    // Apply live: build the source from the def and (re)spawn it straight into
    // the running registry — `spawn_one` aborts+replaces any prior instance of
    // the same name, so an edit takes effect without a restart. A *disabled* def
    // builds to `None`; we just tear down any running instance.
    match config::build_one(&def, &state.store, &state.projections, &state.device_id) {
        Some(source) => {
            state
                .auto_import_registry
                .spawn_one(source, state.default_interval)
                .await;
        }
        None => {
            state.auto_import_registry.remove(&def.name).await;
        }
    }
    Ok(Json(serde_json::json!({ "status": "saved", "applies": "live" })))
}

/// `DELETE /auto_import/sources/{name}` — remove a definition. 404 if no such
/// name is configured.
async fn remove_source_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let path = config::default_path().map_err(internal_err)?;
    let mut cfg = config::load(&path).map_err(internal_err)?;
    let before = cfg.sources.len();
    cfg.sources.retain(|s| s.name != name);
    if cfg.sources.len() == before {
        return Err((StatusCode::NOT_FOUND, format!("no source named '{name}'")));
    }
    config::save(&path, &cfg).map_err(internal_err)?;
    // Tear the running task down live too (no-op if it wasn't spawned).
    state.auto_import_registry.remove(&name).await;
    Ok(Json(serde_json::json!({ "status": "removed", "applies": "live" })))
}
