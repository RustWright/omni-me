//! Persists the frontend's continuity store (unsaved editor, capture and list state) as JSON in
//! `app_data_dir`, so mid-edit content survives an app kill. Opaque here; the frontend owns the
//! shape (`continuity::PersistedWorkspace`).

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
