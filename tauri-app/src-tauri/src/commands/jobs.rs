//! The job status view: a proxy to the server's `/jobs` routes.
//!
//! Passed through as JSON. The contract's types hold `&'static str` declarations,
//! so the frontend decodes its own owned mirror (`frontend/src/types.rs`).

use tauri::State;

use super::shared::get_json;
use crate::AppState;

#[tauri::command(rename_all = "snake_case")]
pub async fn list_jobs(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    get_json(&state, "/jobs", "jobs").await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_job_runs(
    state: State<'_, AppState>,
    name: String,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    let limit = limit.unwrap_or(20);
    let name = super::shared::encode(&name);
    get_json(&state, &format!("/jobs/{name}/runs?limit={limit}"), "jobs").await
}
