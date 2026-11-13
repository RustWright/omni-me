//! The job status view: a proxy to the server's `/jobs` routes.
//!
//! Passed through as JSON. The contract's types hold `&'static str` declarations,
//! so the frontend decodes its own owned mirror (`frontend/src/types.rs`).

use tauri::State;

use crate::AppState;

async fn get_json(state: &AppState, path: &str) -> Result<serde_json::Value, String> {
    let resp = state
        .box_request(reqwest::Method::GET, path)
        .await
        .send()
        .await
        .map_err(|e| format!("jobs fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("jobs: server returned {}", resp.status()));
    }
    resp.json().await.map_err(|e| format!("jobs decode: {e}"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_jobs(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    get_json(&state, "/jobs").await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_job_runs(
    state: State<'_, AppState>,
    name: String,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    let limit = limit.unwrap_or(20);
    get_json(&state, &format!("/jobs/{name}/runs?limit={limit}")).await
}
