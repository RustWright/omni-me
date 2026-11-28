//! The People tab and the person view (rulings 119, 120). What they show is worked out in
//! `omni_me_core::people::view`; these read it, and carry Undo and "Wrong?".

use tauri::State;

use omni_me_core::config::Feature;
use omni_me_core::people::questions::{self, Choice, Outcome, Questions, Reply};
use omni_me_core::people::view::{self, PeopleList, PersonView, Undo};

use super::shared::require_feature;
use crate::AppState;

/// Everyone, most recent first, or the matches for `query`. No model call (ruling 119).
#[tauri::command(rename_all = "snake_case")]
pub async fn people_list(state: State<'_, AppState>, query: String) -> Result<PeopleList, String> {
    require_feature(&state, Feature::People)?;
    view::list(&state.db, &query).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn person_view(
    state: State<'_, AppState>,
    person_id: String,
) -> Result<Option<PersonView>, String> {
    require_feature(&state, Feature::People)?;
    view::person(&state.db, &person_id).await
}

/// Undo or "Wrong?" on one line of "How this record was built".
#[tauri::command(rename_all = "snake_case")]
pub async fn person_undo(state: State<'_, AppState>, undo: Undo) -> Result<(), String> {
    require_feature(&state, Feature::People)?;
    view::undo(&state.writer, &undo).await
}

/// Today's and yesterday's people questions (rulings 104, 118). No model call.
#[tauri::command(rename_all = "snake_case")]
pub async fn people_questions(state: State<'_, AppState>) -> Result<Questions, String> {
    require_feature(&state, Feature::People)?;
    let config = state.config.read().await.clone();
    questions::questions(&state.db, &config, chrono::Utc::now()).await
}

/// One tap on a question card, Undo included.
#[tauri::command(rename_all = "snake_case")]
pub async fn people_reply(state: State<'_, AppState>, reply: Reply) -> Result<Outcome, String> {
    require_feature(&state, Feature::People)?;
    let config = state.config.read().await.clone();
    questions::reply(
        &state.db,
        &state.writer,
        &config,
        chrono::Utc::now(),
        &reply,
    )
    .await
}

/// "Someone I know…": everyone, closest spelling first.
#[tauri::command(rename_all = "snake_case")]
pub async fn people_closest(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<Choice>, String> {
    require_feature(&state, Feature::People)?;
    questions::closest(&state.db, &query).await
}
