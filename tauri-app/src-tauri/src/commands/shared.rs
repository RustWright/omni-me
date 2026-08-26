use chrono::Utc;

use omni_me_core::events::{Event, EventStore, EventType, NewEvent};

use crate::AppState;

/// Append a pre-built event envelope, fold it through the projection runner, and
/// nudge the push debouncer. The grammar-bearing *create* commands build their
/// envelope through the canonical `NewEvent::{transaction_recorded,
/// journal_created, generic_note_created}` factories (so the record key can't
/// drift from the payload id) and then call this shared tail. Returns the stored
/// event so a caller can read back its generated id.
pub(crate) async fn append_new_and_apply(
    state: &AppState,
    event: NewEvent,
) -> Result<Event, String> {
    let stored = state
        .event_store
        .append(event)
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(std::slice::from_ref(&stored))
        .await
        .map_err(|e| e.to_string())?;

    // Auto-sync (push half): nudge the debounced pusher so this edit propagates
    // without a manual Sync. `trigger()` is a non-blocking notify; the debouncer
    // coalesces a burst of edits into one push after its quiet window. Inbound
    // events arrive via the separate pull scheduler (`sync::PullScheduler`).
    state.push_debouncer.trigger();

    Ok(stored)
}

/// Append a single event and immediately fold it through the projection runner.
/// Used by every non-create command in `notes`/`routines`/`budget` (update /
/// delete / tag / close — simple `{id, changes}` shapes, not grammar-bearing).
/// Create events go through the `NewEvent::*` factories + `append_new_and_apply`.
pub(crate) async fn append_and_apply(
    state: &AppState,
    event_type: EventType,
    aggregate_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let event = NewEvent {
        id: None,
        event_type: event_type.to_string(),
        aggregate_id,
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload,
    };
    append_new_and_apply(state, event).await?;
    Ok(())
}

/// Append a batch of events, fold them through the projection runner, and nudge
/// the pusher — the batch twin of [`append_new_and_apply`].
///
/// **Why this exists as a helper rather than three calls at each site.** The
/// `SyncBuffer` this app was designed around was never fed, so nothing woke the
/// pusher; the fix was to nudge it from the shared append tail. That works for
/// the 39 command call sites that go through these helpers — but it turned
/// "every append nudges the pusher" into a rule you have to remember, and five
/// sites had already forgotten it: the Obsidian batch import, the hledger
/// journal import, the recurring scanner, and both wipe-path appends.
///
/// That is not a slow-sync bug, it is a no-sync bug. `pusher::run_loop` blocks
/// on `trigger.notified()` with **no interval fallback**, so an un-nudged bulk
/// import of 10k events pushed *nothing* until some unrelated edit, a manual
/// Sync, or the retry engine happened to fire. Pairing the two operations in
/// one function is what stops a sixth instance;
/// `commands::shared::tests::no_command_appends_events_directly` enforces it.
pub(crate) async fn append_batch_and_apply(
    state: &AppState,
    events: Vec<NewEvent>,
) -> Result<Vec<Event>, String> {
    let appended = state
        .event_store
        .append_batch(events)
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(&appended)
        .await
        .map_err(|e| e.to_string())?;

    if !appended.is_empty() {
        state.push_debouncer.trigger();
    }

    Ok(appended)
}

#[cfg(test)]
mod tests {
    /// No command may reach for `state.event_store.append*` directly.
    ///
    /// Appending an event and nudging the push debouncer have to happen
    /// together: `pusher::run_loop` blocks on `trigger.notified()` with **no
    /// interval fallback**, so an append that skips the nudge doesn't sync
    /// slowly, it doesn't sync at all — until an unrelated edit, a manual Sync,
    /// or the retry engine happens to fire.
    ///
    /// This started as a rule people remembered, and six sites had already
    /// forgotten it: the Obsidian batch import, the hledger journal import, the
    /// recurring scanner, both wipe-path appends, and `dismiss_batch`. The
    /// helpers in this module pair the two operations; this test is what stops
    /// a seventh. If you genuinely need a raw append, add the nudge and extend
    /// the exemption list below with a reason.
    #[test]
    fn no_command_appends_events_directly() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();

        fn walk(dir: &std::path::Path, offenders: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, offenders);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                // This module *is* the sanctioned append path.
                if path.file_name().and_then(|f| f.to_str()) == Some("shared.rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // Strip whitespace ENTIRELY rather than collapsing it to single
                // spaces. Collapsing looks equivalent and is not: rustfmt breaks
                // a long chain as `state\n    .event_store`, and since `.event_store`
                // is one whitespace-delimited token that collapses to
                // `state .event_store` — which matches neither a spaced nor an
                // unspaced needle. This test passed against a planted multi-line
                // violation until that was found.
                let flat: String = text.split_whitespace().collect();
                if flat.contains("state.event_store.append") {
                    offenders.push(path.display().to_string());
                }
            }
        }

        walk(&src, &mut offenders);
        assert!(
            offenders.is_empty(),
            "these append events without going through commands::shared, so the push \
             debouncer is never nudged and the events never sync: {offenders:#?}"
        );
    }

    /// Nothing in `src-tauri` may build an HTTP client without a timeout.
    ///
    /// The `src-tauri` half of `omni_me_core::http`'s own scan. `AppState.http`
    /// is the client every `box_request` rides on, so a client built here
    /// straight from `reqwest` means the phone can hang forever on an
    /// unresponsive box with no error surfaced — the failure mode that reads as
    /// "the app is frozen" rather than "the box is down".
    ///
    /// Note the prose above deliberately avoids spelling the constructors out:
    /// this file is itself in scope, so a literal in a comment makes the scan
    /// match itself. It did, on the first run.
    #[test]
    fn no_bare_reqwest_client_in_src_tauri() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();

        // Assembled at runtime: as literals they would appear in this file and
        // the scan would match itself.
        let needles = [
            format!("reqwest::Client::new{}", "()"),
            format!("reqwest::Client::builder{}", "()"),
        ];

        fn walk(dir: &std::path::Path, needles: &[String], offenders: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, needles, offenders);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // Stripped, not collapsed — see `no_command_appends_events_directly`.
                let flat: String = text.split_whitespace().collect();
                if needles.iter().any(|n| flat.contains(n.as_str())) {
                    offenders.push(path.display().to_string());
                }
            }
        }

        walk(&src, &needles, &mut offenders);
        assert!(
            offenders.is_empty(),
            "these build an HTTP client with no timeout; use \
             `omni_me_core::http::client()`: {offenders:#?}"
        );
    }

    /// Nothing may talk to the box except through [`AppState::box_request`].
    ///
    /// Companion to `no_command_appends_events_directly`, and the same reasoning:
    /// the box's bearer token rides on `box_request`, so a command that reaches
    /// for the raw client on `AppState` is a command that sends an unauthenticated
    /// request. Before the helper existed there were fourteen such call sites,
    /// each re-deriving the base URL by hand — a convention would not have
    /// survived the fifteenth.
    #[test]
    fn no_command_builds_a_box_request_by_hand() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();

        fn walk(dir: &std::path::Path, offenders: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, offenders);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                // `lib.rs` defines `box_request` itself and owns the one
                // `reqwest::Client` the helper borrows.
                if path.file_name().and_then(|f| f.to_str()) == Some("lib.rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // Whitespace stripped, not collapsed — see the note in the
                // sibling test above.
                let flat: String = text.split_whitespace().collect();
                // Needle assembled at runtime rather than written as a literal:
                // spelling it out would make this file match itself, and the
                // usual dodge — excluding the scanner's own file — would blind
                // the scan to a real violation added here later.
                let needle = format!("state{}http", ".");
                if flat.contains(&needle) {
                    offenders.push(path.display().to_string());
                }
            }
        }

        walk(&src, &mut offenders);
        assert!(
            offenders.is_empty(),
            "these use the raw reqwest client on AppState instead of \
             AppState::box_request, so \
             they send the box an unauthenticated request: {offenders:#?}"
        );
    }

}
