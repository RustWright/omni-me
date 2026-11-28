//! The plan card and the read-only view (rulings 110, 117). What they show is worked out in
//! `omni_me_core::planning::view`; these only read it.

use tauri::{AppHandle, Manager, State};

use omni_me_core::config::Feature;
use omni_me_core::events::TimerStep;
use omni_me_core::planning::reminders::{self, StartedBlock};
use omni_me_core::planning::view::{self, PlanCard, PlanView};

use super::shared::require_feature;
use crate::AppState;

/// The card for one `plan.set` proposal. `None` when the id is not a plan, or its detail has
/// not synced yet: the chat then shows the ordinary proposal card.
#[tauri::command(rename_all = "snake_case")]
pub async fn plan_card(
    state: State<'_, AppState>,
    proposal_id: String,
) -> Result<Option<PlanCard>, String> {
    require_feature(&state, Feature::Planning)?;
    let config = state.config.read().await.clone();
    view::card(&state.db, &config, &proposal_id, chrono::Utc::now()).await
}

/// Must match `BLOCK_START_FILE` in `MainActivity.kt`, in the same directory as the share files.
const BLOCK_START_FILE: &str = "block_start";

/// A Start tap on a block reminder, which `MainActivity.kt` leaves in the app data dir whether
/// the tap launched the app or reached it running. Records the start and returns the block;
/// `None` when no tap is waiting. The file goes first, so a refused start is not retried.
#[tauri::command(rename_all = "snake_case")]
pub async fn take_block_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<StartedBlock>, String> {
    let path = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?
        .join(BLOCK_START_FILE);
    let action = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("block start read: {e}")),
    };
    let _ = tokio::fs::remove_file(&path).await;
    let token = crate::block_reminders::token(&state.app_data_dir);
    let Some(start) = reminders::parse_start_action(action.trim(), &token) else {
        // Never log the action: a forged one is noise, and a real one carries the token.
        tracing::warn!("block start: not this device's start action");
        return Ok(None);
    };
    require_feature(&state, Feature::Planning)?;
    let config = state.config.read().await.clone();
    let started = reminders::start_block(&state.db, &state.writer, &config, start).await;
    crate::block_reminders::nudge();
    started.map(Some)
}

/// Start, Pause or Stop on a block in the Plan view (ruling 93). `block_start` is the block's
/// start in unix seconds, as the view hands it out; `step` is `start`, `pause` or `stop`.
#[tauri::command(rename_all = "snake_case")]
pub async fn block_timer(
    state: State<'_, AppState>,
    block_start: i64,
    step: String,
) -> Result<StartedBlock, String> {
    require_feature(&state, Feature::Planning)?;
    let step = match step.as_str() {
        "start" => TimerStep::Start,
        "pause" => TimerStep::Pause,
        "stop" => TimerStep::Stop,
        other => return Err(format!("unknown timer step {other:?}")),
    };
    let start = chrono::DateTime::from_timestamp(block_start, 0)
        .ok_or_else(|| format!("block start {block_start} is out of range"))?;
    let config = state.config.read().await.clone();
    let done = reminders::time_block(&state.db, &state.writer, &config, start, step).await;
    // A running timer schedules its nudge and a pause or stop cancels it.
    crate::block_reminders::nudge();
    done
}

/// What is left of today and the rest of the week.
#[tauri::command(rename_all = "snake_case")]
pub async fn plan_view(state: State<'_, AppState>) -> Result<PlanView, String> {
    require_feature(&state, Feature::Planning)?;
    let config = state.config.read().await.clone();
    view::view(&state.db, &config, chrono::Utc::now()).await
}
