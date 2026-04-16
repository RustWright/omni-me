use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use serde::Deserialize;

use omni_me_core::events::SurrealEventStore;
use omni_me_core::llm;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ProcessNoteRequest {
    pub raw_text: String,
    pub device_id: String,
}

pub fn notes_routes() -> Router<AppState> {
    Router::new().route("/notes/{note_id}/process", post(process_note_handler))
}

async fn process_note_handler(
    State(state): State<AppState>,
    Path(note_id): Path<String>,
    Json(body): Json<ProcessNoteRequest>,
) -> Result<Json<llm::NoteProcessingResult>, String> {
    let event_store = SurrealEventStore::new((*state.db).clone());

    let result = llm::process_note(
        &note_id,
        &body.raw_text,
        &body.device_id,
        state.llm_client.as_ref(),
        &event_store,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(Json(result))
}
