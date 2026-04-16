use axum::{
    Json,
    Router,
    extract::State,
    routing::post,
};
use chrono::Utc;

use axum::http::StatusCode;
use omni_me_core::events::{EventStore, EventType, SurrealEventStore, validate_payload};
use omni_me_core::sync::{PullRequest, PullResponse, PushRequest, PushResponse};

use crate::AppState;

/// Build the sync router (nested under /sync).
pub fn sync_routes() -> Router<AppState> {
    Router::new()
        .route("/sync/push", post(push_handler))
        .route("/sync/pull", post(pull_handler))
}

const MAX_EVENTS_PER_PUSH: usize = 100;

async fn push_handler(
    State(state): State<AppState>,
    Json(body): Json<PushRequest>,
) -> Result<Json<PushResponse>, (StatusCode, String)> {
    if body.events.len() > MAX_EVENTS_PER_PUSH {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("too many events: {} (max {})", body.events.len(), MAX_EVENTS_PER_PUSH),
        ));
    }

    // Validate all events before appending any
    for (i, event) in body.events.iter().enumerate() {
        let event_type: EventType = event.event_type.parse().map_err(|e: String| {
            (StatusCode::BAD_REQUEST, format!("event[{i}]: {e}"))
        })?;
        validate_payload(&event_type, &event.payload).map_err(|e| {
            (StatusCode::BAD_REQUEST, format!("event[{i}]: {e}"))
        })?;
    }

    let store = SurrealEventStore::new((*state.db).clone());
    let count = body.events.len();

    store.append_batch(body.events).await.map_err(|e| {
        tracing::warn!("failed to append events during push: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to store events: {e}"))
    })?;

    Ok(Json(PushResponse { count }))
}

async fn pull_handler(
    State(state): State<AppState>,
    Json(body): Json<PullRequest>,
) -> Json<PullResponse> {
    let store = SurrealEventStore::new((*state.db).clone());

    let events = store
        .get_since(body.since, Some(&body.device_id))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("failed to get events during pull: {e}");
            vec![]
        });

    let sync_timestamp = Utc::now();

    Json(PullResponse {
        events,
        sync_timestamp,
    })
}
