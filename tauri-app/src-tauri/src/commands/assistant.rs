//! Asking the assistant a question, and reading what it said back.
//!
//! **The client never calls a model.** It appends an `AssistantQuestionAsked`
//! event and stops; the resident agent picks the question up through sync,
//! answers it, and appends the answer, which arrives here the same way. That is
//! why there is no timeout, no retry and no API key on this path — the question
//! is durable the moment it is appended, and an answer written while the phone
//! was in a tunnel is waiting when it comes back.
//!
//! It also sidesteps a constraint that has no workaround: `surrealkv` holds an
//! OS-level exclusive lock on its directory, so nothing outside the agent's own
//! process can open the database a running agent is holding. An event is the one
//! channel that already crosses that boundary.
//!
//! The read commands are thin by design — the policy (what counts as pending,
//! how a thread is ordered) lives in `omni_me_core::assistant::conversation`, so
//! the agent's sweep and this screen cannot drift apart on the same question.

use tauri::State;

use omni_me_core::assistant::conversation::{ThreadSummary, ThreadView};
use omni_me_core::assistant::{list_threads, read_thread};
use omni_me_core::config::Feature;
use omni_me_core::events::{AssistantQuestionAskedPayload, NewEvent};

use super::shared::{append_new_and_apply, require_feature};
use crate::AppState;

/// What a freshly authored question tells the caller.
#[derive(serde::Serialize)]
pub struct AskedView {
    /// The thread the question landed on — echoed back because a new thread's id
    /// is minted here, and the UI needs it to open the thread it just started.
    pub thread_id: String,
    /// The question's own id. The answer's `in_reply_to` will carry it, which is
    /// how the UI knows which pending question just resolved.
    pub message_id: String,
}

/// How long a title may run. A title is derived from the question's opening
/// words, and a thread list is unreadable if one entry is a paragraph.
const MAX_TITLE_CHARS: usize = 80;

/// Append one question.
///
/// `thread_id` continues an existing conversation; `None` starts one. Starting a
/// thread is therefore an ordinary consequence of asking without naming one —
/// there is no separate "create thread" command, because an empty thread is a
/// row nothing can ever answer.
#[tauri::command(rename_all = "snake_case")]
pub async fn ask_assistant(
    state: State<'_, AppState>,
    text: String,
    thread_id: Option<String>,
) -> Result<AskedView, String> {
    // Checked here as well as at append time: this is what makes the feature
    // *inert* rather than merely hidden, and it is the guard that holds when the
    // command is reached by a stale frontend or a share intent.
    require_feature(&state, Feature::Llm)?;

    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("a question needs some words".into());
    }

    let is_new_thread = thread_id.is_none();
    let thread_id = thread_id.unwrap_or_else(|| ulid::Ulid::new().to_string());
    let message_id = ulid::Ulid::new().to_string();

    // Only a thread's first question carries a title. A later one would rename
    // the conversation under the user mid-scroll, and the projection takes the
    // first non-empty title it sees regardless.
    let title = is_new_thread.then(|| title_from(&text));

    let payload = AssistantQuestionAskedPayload {
        thread_id: thread_id.clone(),
        message_id: message_id.clone(),
        text,
        title,
    };

    tracing::info!(
        thread_id = %thread_id,
        message_id = %message_id,
        new_thread = is_new_thread,
        "ask_assistant"
    );

    let event = NewEvent::assistant_question_asked(state.device_id.clone(), &payload)
        .map_err(|e| format!("could not encode the question: {e}"))?;
    append_new_and_apply(&state, event).await?;

    Ok(AskedView {
        thread_id,
        message_id,
    })
}

/// Every conversation, most recently active first.
///
/// ⚠️ The caller must refetch this **on tab entry**, not once at boot: a thread
/// answered on another device arrives through a later sync, and a fetch-once
/// provider would never show it.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_assistant_threads(
    state: State<'_, AppState>,
) -> Result<Vec<ThreadSummary>, String> {
    require_feature(&state, Feature::Llm)?;
    list_threads(&state.db).await.map_err(|e| e.to_string())
}

/// One conversation, oldest message first, with its pending question resolved.
#[tauri::command(rename_all = "snake_case")]
pub async fn read_assistant_thread(
    state: State<'_, AppState>,
    thread_id: String,
) -> Result<ThreadView, String> {
    require_feature(&state, Feature::Llm)?;
    read_thread(&state.db, &thread_id)
        .await
        .map_err(|e| e.to_string())
}

/// A thread's display name, taken from the question that started it.
///
/// Truncation happens on a **character** boundary, not a byte one: the questions
/// this app takes are prose, and slicing a multi-byte character in half would
/// panic on the very input the feature exists to accept.
fn title_from(question: &str) -> String {
    let mut title: String = question.chars().take(MAX_TITLE_CHARS).collect();
    if question.chars().count() > MAX_TITLE_CHARS {
        title.push('…');
    }
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_question_is_its_own_title() {
        assert_eq!(
            title_from("what did I write about rent?"),
            "what did I write about rent?"
        );
    }

    #[test]
    fn a_long_question_is_cut_on_a_character_boundary() {
        // Multi-byte throughout: a byte-wise slice would panic here.
        let question = "é".repeat(MAX_TITLE_CHARS + 20);
        let title = title_from(&question);
        assert_eq!(
            title.chars().count(),
            MAX_TITLE_CHARS + 1,
            "the cut plus the ellipsis"
        );
        assert!(title.ends_with('…'));
    }

    #[test]
    fn a_question_exactly_at_the_limit_is_not_marked_truncated() {
        let question = "a".repeat(MAX_TITLE_CHARS);
        assert_eq!(title_from(&question), question);
    }
}
