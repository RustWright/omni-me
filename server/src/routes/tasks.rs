//! The task hub's server half: session hooks push each changed `.omni/tasks.toml` here, and
//! SessionStart reads back which hub ids were taken over. Rulings 91 and 121 (RULINGS.md).
//! The hub never reads git; the indexes arrive as file contents.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use omni_me_core::events::EventWriter;
use omni_me_core::tasks::queries::{self, IngestError, TakenOver};

use crate::AppState;

pub fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/tasks/hub_index", post(push_index))
        .route("/tasks/taken_over", get(taken_over))
}

/// What the pushing hook prints: whether anything changed and what the intake could not read.
#[derive(Debug, Serialize)]
pub struct PushReport {
    pub project: String,
    /// False when the file matched the last one recorded.
    pub recorded: bool,
    pub tasks: usize,
    pub needs_you: usize,
    /// Values kept but not understood; never dropped silently.
    pub problems: Vec<String>,
}

/// The body is the index file's text, as `curl --data-binary @.omni/tasks.toml` sends it.
async fn push_index(
    State(state): State<AppState>,
    body: String,
) -> Result<Json<PushReport>, (StatusCode, String)> {
    let writer = EventWriter::new(
        state.store.clone(),
        state.projections.clone(),
        omni_me_core::config::ALL_FEATURES.iter().copied().collect(),
        state.device_id.clone(),
    );
    let (index, recorded) =
        queries::ingest(&state.db, &writer, &body)
            .await
            .map_err(|e| match e {
                IngestError::Parse(_) => (StatusCode::BAD_REQUEST, e.to_string()),
                IngestError::Store(_) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            })?;
    Ok(Json(PushReport {
        needs_you: index.tasks.iter().filter(|t| t.needs_you()).count(),
        tasks: index.tasks.len(),
        project: index.project,
        recorded,
        problems: index.problems,
    }))
}

#[derive(Debug, Deserialize)]
pub struct TakenOverQuery {
    /// A project slug; only its ids come back.
    pub project: Option<String>,
}

async fn taken_over(
    State(state): State<AppState>,
    Query(q): Query<TakenOverQuery>,
) -> Result<Json<Vec<TakenOver>>, (StatusCode, String)> {
    queries::taken_over(&state.db, q.project.as_deref())
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
