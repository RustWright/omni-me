//! Workspace-continuity persistence (task 1.8a).
//!
//! Persists the frontend's in-memory continuity store (unsaved editor / capture
//! / transaction-list state) to a flat JSON file in `app_data_dir`, so content
//! the user left mid-edit survives an app-kill — the gap task 1.6 deliberately
//! left to disk persistence rather than a root save daemon. The backend treats
//! the blob opaquely; its shape is owned by the frontend
//! (`continuity::PersistedWorkspace`).

use tauri::State;

use crate::AppState;

#[tauri::command(rename_all = "snake_case")]
pub async fn get_workspace(state: State<'_, AppState>) -> Result<String, String> {
    let path = state.app_data_dir.join(crate::WORKSPACE_FILE);
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(contents),
        // No file yet = nothing persisted; hand back an empty blob.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn save_workspace(state: State<'_, AppState>, json: String) -> Result<(), String> {
    let path = state.app_data_dir.join(crate::WORKSPACE_FILE);
    std::fs::write(&path, &json).map_err(|e| e.to_string())
}
