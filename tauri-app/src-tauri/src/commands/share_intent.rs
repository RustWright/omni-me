//! On Android, `MainActivity.kt` writes a shared file and a metadata sidecar to the app data
//! dir; `take_pending_share_intent`, called on mount, returns the pair and deletes both.
//! On desktop it always returns `None`.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct PendingShareIntent {
    pub mime: String,
    pub filename: String,
    pub size: u64,
    pub bytes: Vec<u8>,
}

const BYTES_FILE: &str = "share_intent.bin";
const META_FILE: &str = "share_intent.json";

#[tauri::command(rename_all = "snake_case")]
pub async fn take_pending_share_intent(
    app: AppHandle,
    _state: State<'_, AppState>,
) -> Result<Option<PendingShareIntent>, String> {
    // On Android this is `activity.dataDir`, not `filesDir` (`dataDir/files`). Kotlin's
    // `bridgeDir()` must return the same directory: change one side and you must change the
    // other, or shares land where this never looks, silently.
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    let bytes_path = dir.join(BYTES_FILE);
    let meta_path = dir.join(META_FILE);

    if !tokio::fs::try_exists(&bytes_path)
        .await
        .map_err(|e| format!("bytes exists: {e}"))?
    {
        return Ok(None);
    }

    let meta_raw = tokio::fs::read_to_string(&meta_path)
        .await
        .map_err(|e| format!("meta read: {e}"))?;
    let meta: serde_json::Value =
        serde_json::from_str(&meta_raw).map_err(|e| format!("meta parse: {e}"))?;
    let mime = meta
        .get("mime")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream")
        .to_string();
    let filename = meta
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("shared")
        .to_string();

    let bytes = tokio::fs::read(&bytes_path)
        .await
        .map_err(|e| format!("bytes read: {e}"))?;
    let size = bytes.len() as u64;

    // Best-effort cleanup — if these fail (e.g. concurrent writer), worst
    // case the frontend sees the same intent twice; the user can dismiss.
    let _ = tokio::fs::remove_file(&bytes_path).await;
    let _ = tokio::fs::remove_file(&meta_path).await;

    Ok(Some(PendingShareIntent {
        mime,
        filename,
        size,
        bytes,
    }))
}
