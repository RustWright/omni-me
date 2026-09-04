//! Auto-import observability + manual-trigger routes.
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
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};

use omni_me_core::auto_import::config::{self, SourceDef};
use omni_me_core::auto_import::paused;
use omni_me_core::auto_import_scheduler::{
    ReauthOutcome, SourceHealth, SourceStatus, classify_source_health,
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
        // Runtime off-switch (#367). Pause live-aborts a source's scheduler task
        // (keeping its config) and *persists* the pause so it survives a restart;
        // resume re-spawns it and clears the persisted flag. Works for compiled
        // overlay bank sources too — they key on their registry name, no
        // `sources.toml` entry required.
        .route(
            "/auto_import/sources/{name}/pause",
            post(pause_source_handler),
        )
        .route(
            "/auto_import/sources/{name}/resume",
            post(resume_source_handler),
        )
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

/// The manual-tick response is the whole [`ImportSummary`], not a single count.
///
/// A manual tick is what a user runs to answer "did that work?", so collapsing
/// the disposition to `events_appended` answered it wrongly whenever rows were
/// dropped: the endpoint returned `{"events_appended": 0}` for both a healthy
/// up-to-date source and one discarding every row it fetched.
async fn tick_handler(
    State(state): State<AppState>,
    Query(q): Query<TickQuery>,
) -> Result<Json<omni_me_core::auto_import_scheduler::ImportSummary>, (StatusCode, String)> {
    match state.auto_import_registry.trigger_manual(&q.source).await {
        Ok(summary) => Ok(Json(summary)),
        Err(omni_me_core::auto_import_scheduler::ImportError::NotConfigured(msg)) => {
            Err((StatusCode::NOT_FOUND, msg))
        }
        Err(e) => Err(upstream_err("tick", &q.source, e)),
    }
}

/// Log the real failure, return a generic one.
///
/// `ImportError`'s Display carries whatever the failing layer produced —
/// including a subprocess helper's raw **stderr** (python tracebacks and all)
/// and `io::Error` strings with absolute box paths. Handing that to the caller
/// is free reconnaissance: the box's directory layout, which helpers exist, and
/// what they are written in. The operator still gets the full detail, in the
/// place operators look.
fn upstream_err(
    op: &str,
    source: &str,
    e: omni_me_core::auto_import_scheduler::ImportError,
) -> (StatusCode, String) {
    tracing::warn!(op, source, error = %e, "auto-import request failed");
    (
        StatusCode::BAD_GATEWAY,
        format!("auto-import {op} failed for '{source}' — see server logs"),
    )
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
        Err(e) => Err(upstream_err("reauth", &req.source, e)),
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

/// Same reasoning as [`upstream_err`]: config-layer failures are `io::Error`
/// strings naming absolute paths on the box. Logged in full, returned generic.
fn internal_err<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    tracing::warn!(error = %e, "auto-import config operation failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "auto-import configuration error — see server logs".to_string(),
    )
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
    // (Re)configuring a source implies it should run per the new config —
    // `spawn_one` below un-pauses it in the live registry, so drop any stale
    // persisted pause to match (otherwise the next boot would re-pause it).
    clear_persisted_pause(&def.name);

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
    Ok(Json(
        serde_json::json!({ "status": "saved", "applies": "live" }),
    ))
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
    // Drop any persisted pause for this name so a later same-name source isn't
    // surprise-paused at the next boot. Best-effort: a removed source is already
    // gone, so a paused-file hiccup shouldn't fail the remove.
    clear_persisted_pause(&name);
    Ok(Json(
        serde_json::json!({ "status": "removed", "applies": "live" }),
    ))
}

// =============================================================================
// Runtime off-switch (#367) — pause / resume, persisted across restarts
// =============================================================================

/// Best-effort removal of a name from the persisted paused set. Used on the
/// config add/remove paths, where (re)configuring a source implies it should run
/// — we never want a stale paused entry to switch it back off at the next boot.
/// Logged, not surfaced: the primary action (add/remove) already succeeded.
fn clear_persisted_pause(name: &str) {
    match paused::default_path() {
        Ok(p) => {
            if let Err(e) = paused::set_paused(&p, name, false) {
                tracing::warn!(source = %name, error = %e, "failed to clear persisted pause");
            }
        }
        Err(e) => tracing::warn!(error = %e, "paused-sources path lookup failed"),
    }
}

/// `POST /auto_import/sources/{name}/pause` — live-abort the source's scheduler
/// task (keeping its config) and persist the pause. 404 if no source by that
/// name is running/registered. The persist is a hard requirement, not
/// best-effort: a pause that silently didn't survive a restart is exactly the
/// runaway-source failure mode #367 exists to prevent, so a persistence failure
/// is surfaced as a 500 (the in-memory abort is harmless on its own).
async fn pause_source_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !state.auto_import_registry.pause(&name).await {
        return Err((
            StatusCode::NOT_FOUND,
            format!("no running source named '{name}'"),
        ));
    }
    let path = paused::default_path().map_err(internal_err)?;
    paused::set_paused(&path, &name, true).map_err(internal_err)?;
    Ok(Json(
        serde_json::json!({ "status": "paused", "applies": "live" }),
    ))
}

/// `POST /auto_import/sources/{name}/resume` — re-spawn a paused source's
/// scheduler loop (an immediate fresh pull) and clear the persisted pause. 404
/// if no source by that name is running/registered.
async fn resume_source_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !state.auto_import_registry.resume(&name).await {
        return Err((
            StatusCode::NOT_FOUND,
            format!("no running source named '{name}'"),
        ));
    }
    let path = paused::default_path().map_err(internal_err)?;
    paused::set_paused(&path, &name, false).map_err(internal_err)?;
    Ok(Json(
        serde_json::json!({ "status": "resumed", "applies": "live" }),
    ))
}
