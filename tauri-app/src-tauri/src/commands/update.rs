//! In-app updater (Android OTA half).
//!
//! Tauri's updater plugin does not support mobile, so Android updates are
//! hand-rolled: the box serves a manifest + the signed APK under `/updates`
//! (see the public server's `UPDATES_DIR` route), and these commands let the app
//! check it, download the APK into the cache dir (covered by the FileProvider's
//! `cache-path`), verify its sha256, and hand the path to the Kotlin
//! `InstallBridge`, which fires the system package-installer intent.
//!
//! Desktop uses the Tauri updater plugin instead (see `lib.rs`); these commands
//! compile everywhere but are only invoked on Android.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use crate::AppState;

/// The manifest the box publishes at `/updates/android/latest.json`.
#[derive(Debug, Deserialize)]
struct AndroidManifest {
    version: String,
    url: String,
    sha256: String,
    #[serde(default)]
    notes: String,
}

/// Result of an update check, surfaced to the Settings "Updates" section.
#[derive(Debug, Serialize)]
pub struct UpdateCheck {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    /// Download URL for the APK (from the manifest; host is the box over the tailnet).
    pub url: String,
    pub sha256: String,
    pub notes: String,
}

/// True when `latest` is a strictly higher semver than `current`. Falls back to
/// a plain inequality if either string isn't valid semver (offer rather than
/// silently skip).
fn is_newer(latest: &str, current: &str) -> bool {
    match (
        semver::Version::parse(latest),
        semver::Version::parse(current),
    ) {
        (Ok(l), Ok(c)) => l > c,
        _ => latest != current,
    }
}

/// Fetch the Android update manifest from the configured server and compare it
/// to the running app version.
#[tauri::command(rename_all = "snake_case")]
pub async fn check_for_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdateCheck, String> {
    let server_url = state.server_url.read().await.clone();
    let manifest_url = format!(
        "{}/updates/android/latest.json",
        server_url.trim_end_matches('/')
    );

    let manifest: AndroidManifest = state
        .http
        .get(&manifest_url)
        .send()
        .await
        .map_err(|e| format!("update check failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("update check failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("bad update manifest: {e}"))?;

    let current = app.package_info().version.to_string();
    Ok(UpdateCheck {
        available: is_newer(&manifest.version, &current),
        current_version: current,
        latest_version: manifest.version,
        url: manifest.url,
        sha256: manifest.sha256,
        notes: manifest.notes,
    })
}

/// Download the APK to the app cache dir, verify its sha256, and return the
/// absolute path. The frontend then hands that path to the Kotlin
/// `window.AndroidInstaller.installApk(path)` bridge. Verifying before install is
/// the integrity guard (the transport is already tailnet-private).
#[tauri::command(rename_all = "snake_case")]
pub async fn download_android_update(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    sha256: String,
) -> Result<String, String> {
    // The FileProvider's `cache-path "."` covers the app cache dir, so a content
    // URI for a file here can be granted to the installer with no manifest change.
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("no cache dir: {e}"))?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("mkdir cache failed: {e}"))?;
    let apk_path = cache_dir.join("omni-me-update.apk");

    let bytes = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("download read failed: {e}"))?;

    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if !actual.eq_ignore_ascii_case(&sha256) {
        return Err(format!("checksum mismatch: expected {sha256}, got {actual}"));
    }

    std::fs::write(&apk_path, &bytes).map_err(|e| format!("write apk failed: {e}"))?;
    Ok(apk_path.to_string_lossy().to_string())
}

/// Ask the Android side to install the (already downloaded + verified) APK at
/// `apk_path`. We write a one-line request side-file into the app's local data
/// dir — the same `filesDir` MainActivity polls — and the Kotlin poller reads
/// the path, fires the system package-installer intent (via the FileProvider),
/// and deletes the request. This reuses the proven side-file Rust↔Kotlin channel
/// (`take_pending_share_intent`) rather than a JS interface, whose injected
/// object would only appear after a page reload in our SPA. No-op on desktop
/// (nothing polls the file there).
#[tauri::command(rename_all = "snake_case")]
pub async fn request_android_install(app: AppHandle, apk_path: String) -> Result<(), String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    tokio::fs::write(dir.join(INSTALL_REQUEST_FILE), apk_path)
        .await
        .map_err(|e| format!("write install request: {e}"))?;
    Ok(())
}

/// Side-file MainActivity polls for an APK install request (must match the
/// Kotlin `INSTALL_REQUEST_FILE`).
const INSTALL_REQUEST_FILE: &str = "install_request";

/// Which update flow the frontend should drive: `"android"` (custom OTA via the
/// commands above) or `"desktop"` (the Tauri updater plugin commands below). The
/// frontend is one wasm build for both, so it asks at runtime.
#[tauri::command(rename_all = "snake_case")]
pub fn app_platform() -> String {
    #[cfg(desktop)]
    {
        "desktop".to_string()
    }
    #[cfg(not(desktop))]
    {
        "android".to_string()
    }
}

/// Desktop: check the Tauri updater endpoint (configured at build time via the
/// private CI's `--config` injection — pubkey + box endpoint). `url`/`sha256` are
/// unused on desktop (the plugin handles download + signature verification).
#[tauri::command(rename_all = "snake_case")]
pub async fn check_desktop_update(app: AppHandle) -> Result<UpdateCheck, String> {
    #[cfg(desktop)]
    {
        use tauri_plugin_updater::UpdaterExt;
        let current = app.package_info().version.to_string();
        let updater = app.updater().map_err(|e| e.to_string())?;
        match updater.check().await {
            Ok(Some(update)) => Ok(UpdateCheck {
                available: true,
                current_version: current,
                latest_version: update.version.clone(),
                url: String::new(),
                sha256: String::new(),
                notes: update.body.clone().unwrap_or_default(),
            }),
            Ok(None) => Ok(UpdateCheck {
                available: false,
                latest_version: current.clone(),
                current_version: current,
                url: String::new(),
                sha256: String::new(),
                notes: String::new(),
            }),
            Err(e) => Err(format!("update check failed: {e}")),
        }
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Err("check_desktop_update is desktop-only".to_string())
    }
}

/// Desktop: download + install the available update (signature-verified by the
/// plugin), then relaunch into the new version.
#[tauri::command(rename_all = "snake_case")]
pub async fn install_desktop_update(app: AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    {
        use tauri_plugin_updater::UpdaterExt;
        let updater = app.updater().map_err(|e| e.to_string())?;
        let update = updater
            .check()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no update available".to_string())?;
        update
            .download_and_install(|_chunk, _total| {}, || {})
            .await
            .map_err(|e| format!("install failed: {e}"))?;
        app.restart()
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Err("install_desktop_update is desktop-only".to_string())
    }
}
