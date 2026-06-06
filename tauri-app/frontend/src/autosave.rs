//! Auto-save resilience (task 1.7).
//!
//! A shared save-state model + a retry/backoff policy for the journal and notes
//! editors. The editors differ in *what* they save (different backend calls),
//! but the *resilience* around a save — how progress is surfaced and how a
//! transient failure is handled — is identical, so it lives here once.

use std::future::Future;

use dioxus::prelude::*;

use crate::timer::sleep_ms;

/// Glanceable auto-save state for the editor's status indicator.
///
/// - `Saved`: the live buffer matches what's persisted.
/// - `Saving`: a save is in flight.
/// - `Unsaved`: edits exist that aren't persisted yet (debounce pending).
/// - `Failed`: the last save exhausted its retries.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SaveState {
    Saved,
    Saving,
    Unsaved,
    Failed,
}

/// Given a zero-based retry `attempt` (0 = the wait before the *first* retry,
/// i.e. after the initial save already failed once), return:
///   - `Some(ms)` to wait `ms` milliseconds and retry, or
///   - `None` to stop retrying and surface `SaveState::Failed`.
///
/// This is auto-save's patience policy for a flaky save (a network blip or a
/// server restart mid-edit): it trades off "recover silently from a brief
/// outage" against "don't leave the user thinking they're saved when they're
/// not." Delays are i32 ms to match `sleep_ms`.
pub fn backoff_delay(attempt: u32) -> Option<i32> {
    (attempt < 4).then(|| 500i32 * 2i32.pow(attempt))
}

/// Run `attempt_save` with retries governed by [`backoff_delay`]. Returns the
/// save's value on the first success, or the last error once the policy gives up.
///
/// Generic over the success value `T` so callers that need it back (e.g. a
/// journal *create* returns the new entry) aren't forced to discard it. The
/// caller still owns *whether* to start a save (debounce + the generation-counter
/// cancel in the editors); this owns *persistence* once a save is underway.
pub async fn save_with_retry<T, F, Fut>(mut attempt_save: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut attempt: u32 = 0;
    loop {
        match attempt_save().await {
            Ok(v) => return Ok(v),
            Err(e) => match backoff_delay(attempt) {
                Some(ms) => {
                    sleep_ms(ms).await;
                    attempt += 1;
                }
                None => return Err(e),
            },
        }
    }
}

/// Glanceable save-state pill, shared by the journal and notes editors. The
/// editors derive `SaveState` from their own signals (saving flag / last-failed
/// flag / content-vs-persisted) and hand it here purely for display.
#[component]
pub fn SaveIndicator(state: SaveState) -> Element {
    let (label, classes) = match state {
        SaveState::Saved => (
            "Saved",
            "bg-obsidian-text-muted/10 text-obsidian-text-muted border-white/10",
        ),
        SaveState::Saving => (
            "Saving…",
            "bg-obsidian-accent/10 text-obsidian-accent border-obsidian-accent/20",
        ),
        SaveState::Unsaved => (
            "Unsaved",
            "bg-amber-500/10 text-amber-400 border-amber-500/30",
        ),
        SaveState::Failed => ("Save failed", "bg-red-900/20 text-red-400 border-red-900/50"),
    };
    rsx! {
        span {
            class: "px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border {classes}",
            "{label}"
        }
    }
}
