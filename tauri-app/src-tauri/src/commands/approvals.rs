//! What is waiting on the user, for the nav badges and the assistant's reminder.
//!
//! One command, two renderers. ⛔ The counting lives in
//! `omni_me_core::approvals` rather than here, so the badge on a tab and the
//! sentence the assistant screen shows can never be two different numbers.
//!
//! ⛔ **Not feature-gated as a whole.** Every other command guards on the one
//! feature it serves; this one spans them, and the gating it needs is already
//! inside `summary` — a switched-off feature contributes nothing, so the command
//! degrades to a shorter list rather than a refusal. Refusing outright would take
//! the *other* features' badges down with it.

use tauri::State;

use omni_me_core::approvals::{self, PendingApprovals};

use crate::AppState;

/// Every review surface with something waiting on it.
///
/// ⚠️ Reads the **boot feature snapshot** off the writer rather than the live
/// config, which is what keeps a badge from appearing on a tab this launch is not
/// rendering. See `approvals::summary`.
#[tauri::command(rename_all = "snake_case")]
pub async fn pending_approvals(
    state: State<'_, AppState>,
) -> Result<Vec<PendingApprovals>, String> {
    approvals::summary(&state.db, state.writer.features())
        .await
        .map_err(|e| e.to_string())
}
