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
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use omni_me_core::auto_import_scheduler::{
    ReauthOutcome, SourceStatus, classify_source_health, SourceHealth,
};

use crate::AppState;

pub fn auto_import_routes() -> Router<AppState> {
    Router::new()
        .route("/auto_import/status", get(status_handler))
        .route("/auto_import/tick", post(tick_handler))
        .route("/auto_import/reauth", post(reauth_handler))
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
