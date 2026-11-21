//! In-app updater (Android OTA half).
//!
//! Hand-rolled, since Tauri's updater does not support mobile: check the box's `/updates`
//! manifest, download the APK to the cache dir, verify its sha256, and hand it to the Kotlin
//! `InstallBridge`. Desktop uses the updater plugin (`lib.rs`).

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

/// True when `latest` is a strictly higher semver than `current`.
///
/// Fails closed: an unparseable version means no update. Falling back to "different" would
/// include "older", making the updater a downgrade channel to a validly signed old release.
fn is_newer(latest: &str, current: &str) -> bool {
    match (
        semver::Version::parse(latest),
        semver::Version::parse(current),
    ) {
        (Ok(l), Ok(c)) => l > c,
        _ => false,
    }
}

/// Fetch the Android update manifest from the configured server and compare it
/// to the running app version.
#[tauri::command(rename_all = "snake_case")]
pub async fn check_for_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdateCheck, String> {
    let manifest: AndroidManifest = state
        .box_request(reqwest::Method::GET, "/updates/android/latest.json")
        .await
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
    let apk_path = cache_dir.join(DOWNLOADED_APK_FILE);

    // The manifest supplies both the `url` and the `sha256`, so verifying one
    // against the other proves only "you received what the manifest named" —
    // it says nothing about who wrote the manifest. Constraining the origin is
    // what makes the check mean something: whoever can write the manifest can
    // still choose which artifact *on the box* you install, but can no longer
    // point the installer at a host of their choosing.
    let path = same_origin_path(&url, &state.server_url.read().await.clone())?;

    let bytes = state
        .box_request(reqwest::Method::GET, &path)
        .await
        .timeout(omni_me_core::http::DOWNLOAD_TIMEOUT)
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
        return Err(format!(
            "checksum mismatch: expected {sha256}, got {actual}"
        ));
    }

    std::fs::write(&apk_path, &bytes).map_err(|e| format!("write apk failed: {e}"))?;
    Ok(apk_path.to_string_lossy().to_string())
}

/// Ask Android to install the downloaded, verified APK, via a request side-file MainActivity
/// polls (the share-intent channel; a JS interface would need a page reload). No-op on desktop.
/// `apk_path` is ignored, kept for the frontend call: the path is derived here, because a
/// supplied one let the webview install any APK it could name.
#[tauri::command(rename_all = "snake_case")]
pub async fn request_android_install(app: AppHandle, apk_path: String) -> Result<(), String> {
    let _ = apk_path;
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("no cache dir: {e}"))?;
    let verified_apk = cache_dir.join(DOWNLOADED_APK_FILE);
    if !verified_apk.exists() {
        return Err("no verified update has been downloaded".to_string());
    }

    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    tokio::fs::write(
        dir.join(INSTALL_REQUEST_FILE),
        verified_apk.to_string_lossy().as_ref(),
    )
    .await
    .map_err(|e| format!("write install request: {e}"))?;
    Ok(())
}

/// Reduce a manifest-supplied absolute URL to a path on the configured box,
/// refusing anything whose scheme, host or port differs.
///
/// Compared component-wise rather than by string prefix: a `starts_with` test
/// on the base URL would accept `http://box.example.com.attacker.test/...`,
/// which is the classic way an origin check that "looks right" fails open.
fn same_origin_path(url: &str, server_url: &str) -> Result<String, String> {
    let target = tauri::Url::parse(url).map_err(|e| format!("bad update url: {e}"))?;
    let base = tauri::Url::parse(server_url).map_err(|e| format!("bad server url: {e}"))?;

    let same = target.scheme() == base.scheme()
        && target.host_str() == base.host_str()
        && target.port_or_known_default() == base.port_or_known_default();
    if !same {
        return Err(format!(
            "refusing update from {} — the configured server is {}",
            target.host_str().unwrap_or("<no host>"),
            base.host_str().unwrap_or("<no host>"),
        ));
    }

    let mut path = target.path().to_string();
    if let Some(q) = target.query() {
        path.push('?');
        path.push_str(q);
    }
    Ok(path)
}

/// Side-file MainActivity polls for an APK install request (must match the
/// Kotlin `INSTALL_REQUEST_FILE`).
const INSTALL_REQUEST_FILE: &str = "install_request";

/// The one APK path this app will ever install — written by
/// [`download_android_update`] after its checksum check and read back by
/// [`request_android_install`]. A constant, so no caller can name a different
/// file.
const DOWNLOADED_APK_FILE: &str = "omni-me-update.apk";

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the fail-closed change: an unparseable version must
    /// not be offered, because "different from current" includes "older".
    #[test]
    fn an_unparseable_version_is_never_newer() {
        assert!(
            !is_newer("not-a-version", "1.2.3"),
            "a broken manifest must not produce an update offer",
        );
        assert!(
            !is_newer("0.9.0-nightly+weird+", "1.2.3"),
            "nor may it become a downgrade channel",
        );
        assert!(!is_newer("1.2.3", "also-not-a-version"));
    }

    #[test]
    fn ordinary_semver_comparison_still_works() {
        assert!(is_newer("1.2.4", "1.2.3"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.2.3", "1.2.3"));
        assert!(!is_newer("1.2.2", "1.2.3"), "older is not newer");
    }

    #[test]
    fn a_same_origin_url_reduces_to_its_path() {
        let path = same_origin_path(
            "http://100.64.1.2:3000/updates/android/omni-me.apk",
            "http://100.64.1.2:3000",
        )
        .expect("same origin must be accepted");
        assert_eq!(path, "/updates/android/omni-me.apk");
    }

    #[test]
    fn a_different_host_is_refused() {
        let err = same_origin_path("http://attacker.test/evil.apk", "http://100.64.1.2:3000")
            .expect_err("a foreign host must be refused");
        assert!(err.contains("refusing update"), "got: {err}");
    }

    /// The classic way an origin check fails open: a `starts_with` test on the
    /// base URL accepts any host that merely *begins* with the real one.
    #[test]
    fn a_host_that_only_prefixes_the_real_one_is_refused() {
        assert!(
            same_origin_path(
                "http://100.64.1.2.attacker.test/evil.apk",
                "http://100.64.1.2:3000",
            )
            .is_err(),
            "host-prefix trickery must not pass the origin check",
        );
    }

    #[test]
    fn a_different_port_is_refused() {
        assert!(
            same_origin_path("http://100.64.1.2:9999/x.apk", "http://100.64.1.2:3000").is_err(),
            "a different port is a different origin",
        );
    }

    #[test]
    fn a_scheme_downgrade_is_refused() {
        assert!(
            same_origin_path("http://box.example/x.apk", "https://box.example").is_err(),
            "http must not satisfy an https-configured server",
        );
    }
}
