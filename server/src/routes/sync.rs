use axum::{
    Json,
    Router,
    extract::State,
    routing::post,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use omni_me_core::events::{Event, EventStore, NewEvent, SurrealEventStore};

use crate::AppState;

/// Request body for POST /sync/push
#[derive(Debug, Deserialize)]
pub struct PushRequest {
    #[allow(dead_code)]
    pub device_id: String,
    pub events: Vec<NewEvent>,
}

/// Response for POST /sync/push
#[derive(Debug, Serialize)]
pub struct PushResponse {
    pub count: usize,
}

/// Request body for POST /sync/pull
#[derive(Debug, Deserialize)]
pub struct PullRequest {
    pub device_id: String,
    pub since: DateTime<Utc>,
}

/// Response for POST /sync/pull
#[derive(Debug, Serialize)]
pub struct PullResponse {
    pub events: Vec<Event>,
    pub sync_timestamp: DateTime<Utc>,
}

/// Build the sync router (nested under /sync).
pub fn sync_routes() -> Router<AppState> {
    Router::new()
        .route("/sync/push", post(push_handler))
        .route("/sync/pull", post(pull_handler))
}

async fn push_handler(
    State(state): State<AppState>,
    Json(body): Json<PushRequest>,
) -> Json<PushResponse> {
    let store = SurrealEventStore::new((*state.db).clone());
    let mut count = 0;

    for event in body.events {
        match store.append(event).await {
            Ok(_) => count += 1,
            Err(e) => {
                tracing::warn!("failed to append event during push: {e}");
            }
        }
    }

    Json(PushResponse { count })
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
