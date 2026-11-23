//! The plan card and the read-only view (rulings 110, 117). What they show is worked out in
//! `omni_me_core::planning::view`; these only read it.

use tauri::State;

use omni_me_core::config::Feature;
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

/// What is left of today and the rest of the week.
#[tauri::command(rename_all = "snake_case")]
pub async fn plan_view(state: State<'_, AppState>) -> Result<PlanView, String> {
    require_feature(&state, Feature::Planning)?;
    let config = state.config.read().await.clone();
    view::view(&state.db, &config, chrono::Utc::now()).await
}
