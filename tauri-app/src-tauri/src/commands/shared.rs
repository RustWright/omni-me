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
                // Collapse whitespace so a method chain split across lines is
                // matched the same as a single-line one.
                let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                for needle in [
                    "state . event_store . append",
                    "state.event_store.append",
                ] {
                    if flat.contains(needle) {
                        offenders.push(path.display().to_string());
                        break;
                    }
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
}
