//! What is waiting on the user, for the nav badges and the assistant's reminder.
//!
//! Counting lives in `omni_me_core::approvals`, so badge and sentence never disagree.
//! Not feature-gated as a whole: `summary` drops switched-off features, and refusing outright
//! would take the other features' badges down too.

use tauri::State;

use omni_me_core::approvals::{self, PendingApprovals};

use crate::AppState;

/// Every review surface with something waiting on it.
///
/// Reads the boot feature snapshot off the writer rather than the live
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
