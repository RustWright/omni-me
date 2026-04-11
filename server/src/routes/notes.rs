use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use serde::{Deserialize, Serialize};

use omni_me_core::events::SurrealEventStore;
use omni_me_core::llm;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ProcessNoteRequest {
    pub raw_text: String,
    pub device_id: String,
}

#[derive(Debug, Serialize)]
pub struct ProcessNoteResponse {
    pub tags: Vec<String>,
    pub tasks: Vec<TaskResponse>,
    pub dates: Vec<DateResponse>,
    pub expenses: Vec<ExpenseResponse>,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub description: String,
    pub priority: String,
}

#[derive(Debug, Serialize)]
pub struct DateResponse {
    pub date: String,
    pub context: String,
}

#[derive(Debug, Serialize)]
pub struct ExpenseResponse {
    pub amount: f64,
    pub currency: String,
    pub description: String,
}

pub fn notes_routes() -> Router<AppState> {
    Router::new().route("/notes/{note_id}/process", post(process_note_handler))
}

async fn process_note_handler(
    State(state): State<AppState>,
    Path(note_id): Path<String>,
    Json(body): Json<ProcessNoteRequest>,
) -> Result<Json<ProcessNoteResponse>, String> {
    let api_key = std::env::var("GEMINI_API_KEY").map_err(|_| {
        "GEMINI_API_KEY not set on server".to_string()
    })?;

    let llm_client = llm::GeminiClient::new(api_key);
    let event_store = SurrealEventStore::new((*state.db).clone());

    let result = llm::process_note(
        &note_id,
        &body.raw_text,
        &body.device_id,
        &llm_client,
        &event_store,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(Json(ProcessNoteResponse {
        tags: result.tags,
        tasks: result.tasks.into_iter().map(|t| TaskResponse {
            description: t.description,
            priority: t.priority,
        }).collect(),
        dates: result.dates.into_iter().map(|d| DateResponse {
            date: d.date,
            context: d.context,
        }).collect(),
        expenses: result.expenses.into_iter().map(|e| ExpenseResponse {
            amount: e.amount,
            currency: e.currency,
            description: e.description,
        }).collect(),
        summary: result.summary,
    }))
}
