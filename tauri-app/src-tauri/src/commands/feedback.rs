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

use omni_me_core::events::{EventStore, FeedbackCapturedPayload, NewEvent};

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
    /// Which app-data root this run is on. `non_production` says a sandbox;
    /// this says which one.
    pub data_dir: String,
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
        data_dir: state.app_data_dir.to_string_lossy().to_string(),
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

    // `diagnostics` already bounds this on the way out, so in practice the
    // truncation never fires. It is here because the frontend's cap is a
    // *convention* and this is the boundary where an event becomes permanent:
    // the log is append-only and replicates to every device, so an oversized
    // report is not something a later fix can take back.
    let mut recent_errors = recent_errors;
    recent_errors.truncate(MAX_RECENT_ERRORS);

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
        data_dir: Some(state.app_data_dir.to_string_lossy().to_string()),
        recent_errors,
        recent_events: recent_event_lines(&state, RECENT_EVENT_LIMIT).await,
    };

    let event = NewEvent::feedback_captured(state.device_id.clone(), &payload)
        .map_err(|e| format!("could not encode report: {e}"))?;
    append_new_and_apply(&state, event).await?;

    Ok(feedback_id)
}

/// Hard ceiling on attached error lines. Matches `diagnostics::CAPACITY` on the
/// frontend, which is the only caller — this is the backstop, not the policy.
const MAX_RECENT_ERRORS: usize = 50;

/// How many recent events a report carries. Enough to show the sequence that
/// led somewhere — opening a note, an autosave, a sync — without turning the
/// report into a log dump nobody reads to the end.
const RECENT_EVENT_LIMIT: u32 = 20;

/// The last few events authored on this device, oldest first, as
/// `<timestamp> <event_type> <aggregate_id>` lines.
///
/// **Built here rather than in the frontend, deliberately.** This is the same
/// split the module header draws for device and build identity: anything
/// describing the *installation* is filled in on this side, where a stale or
/// forged frontend cannot reach it. It also keeps the event list off the IPC
/// wire entirely — the modal never needs to see it.
///
/// **Payloads are never included.** Event payloads carry journal prose and
/// transaction amounts; the type and aggregate id are what let a reader
/// reconstruct a sequence, and a report is not a data export.
///
/// A store error yields an empty list, matching `queries::list_feedback`'s
/// skip-don't-fail discipline. A report must never fail because its optional
/// context could not be gathered — the sentence the user typed is the part that
/// matters, and it is already in hand by the time this runs.
async fn recent_event_lines(state: &AppState, limit: u32) -> Vec<String> {
    let events = match state
        .event_store
        .get_recent_by_device(&state.device_id, limit)
        .await
    {
        Ok(events) => events,
        Err(e) => {
            tracing::warn!(error = %e, "could not read recent events for report");
            return Vec::new();
        }
    };

    // The store returns newest-first so the limit keeps the events nearest the
    // failure; the payload documents newest-last, which is how a sequence reads.
    events
        .into_iter()
        .rev()
        .map(|e| {
            format!(
                "{} {} {}",
                e.timestamp.to_rfc3339(),
                e.event_type,
                e.aggregate_id
            )
        })
        .collect()
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
