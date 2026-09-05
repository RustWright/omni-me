//! In-app problem reporting.
//!
//! One command appends one `FeedbackCaptured` event. There is no projection, no
//! table and no review screen: feedback is written on a device that may be
//! offline and read later in bulk from a terminal, which is the access pattern
//! an append-only log already serves. Riding the existing write path buys the
//! offline queue, retry, dedup and cross-device replication that carry every
//! other event, none of which had to be rebuilt here.
//!
//! **The split of responsibilities is deliberate.** The frontend supplies only
//! what it alone can know — the sentence, which screen was open, and what was on
//! it. Everything identifying the *installation* (device, build, sync target,
//! sandbox flag) is filled in here, where it cannot be stale or forged by a
//! frontend that has been open across an update.

use tauri::State;

use omni_me_core::events::{FeedbackCapturedPayload, NewEvent};

use super::shared::append_new_and_apply;
use crate::AppState;

/// Which build a report came from. Split out from `get_runtime_profile` because
/// that one is on the render path of a banner drawn on every screen and must
/// stay free of anything it doesn't need.
#[derive(serde::Serialize)]
pub struct AppContextView {
    pub app_version: String,
    pub platform: String,
    pub device_id: String,
    pub server_url: String,
    pub non_production: bool,
}

/// Build + installation identity, for display in the capture modal so the user
/// can see what a report will carry before sending it.
///
/// The modal shows this rather than hiding it. A report is a message about the
/// user's own machine and may quote their unsaved journal text, so what goes in
/// it is not something to decide on their behalf.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_app_context(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AppContextView, String> {
    Ok(AppContextView {
        app_version: app.package_info().version.to_string(),
        platform: platform_name().to_string(),
        device_id: state.device_id.clone(),
        server_url: state.server_url.read().await.clone(),
        non_production: state.non_production,
    })
}

/// Append one problem report.
///
/// Returns the report's id so the UI can confirm with something concrete rather
/// than a bare "sent".
///
/// `body` is the only field that must be non-empty. Every context argument is
/// optional and every one of them is droppable in the modal — a report filed
/// mid-friction must never fail because a screen declined to describe itself,
/// or because the user chose not to attach their draft.
#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub async fn submit_feedback(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    body: String,
    screen: Option<String>,
    screen_ref: Option<String>,
    screen_data: Option<String>,
    recent_errors: Vec<String>,
) -> Result<String, String> {
    let body = body.trim().to_string();
    if body.is_empty() {
        return Err("a report needs a description".into());
    }

    let feedback_id = ulid::Ulid::new().to_string();
    tracing::info!(feedback_id = %feedback_id, screen = ?screen, "submit_feedback");

    let payload = FeedbackCapturedPayload {
        feedback_id: feedback_id.clone(),
        body,
        screen,
        screen_ref,
        screen_data,
        app_version: Some(app.package_info().version.to_string()),
        platform: Some(platform_name().to_string()),
        server_url: Some(state.server_url.read().await.clone()),
        non_production: state.non_production,
        recent_errors,
        // Filled once `get_recent_events` exists; the field is already on the
        // payload so adding it later is not a wire-format change.
        recent_events: Vec::new(),
    };

    let event = NewEvent::feedback_captured(state.device_id.clone(), &payload)
        .map_err(|e| format!("could not encode report: {e}"))?;
    append_new_and_apply(&state, event).await?;

    Ok(feedback_id)
}

/// Compile-time platform tag. `std::env::consts::OS` reports the *host* triple,
/// which for an Android build is what we want, but naming it here keeps the tag
/// stable if that ever stops being true.
fn platform_name() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}
