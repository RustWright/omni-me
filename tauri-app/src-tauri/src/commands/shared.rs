//! The command layer's half of the feature gate, plus thin delegation to the
//! append boundary.
//!
//! The append tails themselves live in `omni_me_core::events::EventWriter` —
//! the guard, the projection fold and the pusher nudge belong together, and a
//! second binary that authors events must inherit all three rather than
//! re-implement them. What stays here is the *command*-level guard, which has no
//! core equivalent because it is about refusing a command outright, not about
//! refusing an append.

use omni_me_core::config::Feature;
use omni_me_core::events::{Event, EventType, NewEvent, feature_off_message};

use crate::AppState;

/// Refuse a command whose feature is switched off.
///
/// The last line of the "off means inert" promise. A hidden tab is what a user
/// sees; this is what holds when something reaches the command anyway — a stale
/// frontend, a share intent, an overlay, a future LLM tool call.
pub(crate) fn require_feature(state: &AppState, feature: Feature) -> Result<(), String> {
    require_any_feature(state, &[feature])
}

/// Refuse unless at least one of `features` is on.
///
/// Some commands genuinely serve two: Obsidian import/export moves journal
/// entries *and* generic notes, so it stays available while either is on.
pub(crate) fn require_any_feature(state: &AppState, features: &[Feature]) -> Result<(), String> {
    if features.iter().any(|f| state.writer.features().contains(f)) {
        return Ok(());
    }
    // Same wording as the writer's own refusal, shared from core so the two
    // cannot drift.
    let refusal = feature_off_message(features);
    tracing::warn!(%refusal, "command refused: feature off");
    Err(refusal)
}

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
    state
        .writer
        .append_new(event)
        .await
        .map_err(|e| e.to_string())
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
    state
        .writer
        .append(event_type, aggregate_id, payload)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Batch twin of [`append_new_and_apply`].
pub(crate) async fn append_batch_and_apply(
    state: &AppState,
    events: Vec<NewEvent>,
) -> Result<Vec<Event>, String> {
    state
        .writer
        .append_batch(events)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {

    /// Every command that reaches the box on a feature's behalf must refuse when
    /// that feature is off.
    ///
    /// The **write** side needs no scan: `authoring_features` is an exhaustive
    /// match, and the guard sits inside `EventWriter`, which
    /// `no_command_appends_events_directly` already forces every write through.
    /// Outbound HTTP has no such chokepoint — `box_request` is generic — so a
    /// command can spend an LLM call or hit a bank source with its feature off
    /// and nothing downstream would notice.
    ///
    /// **Blind spot, deliberately open.** This checks that a module reaching the
    /// box mentions a guard *somewhere*, not that every function in it does. A
    /// finer check would need to parse Rust. Two modules are exempt because they
    /// belong to no feature: `sync.rs` (sync must work with everything off, or a
    /// disabled feature's events would never reach another device) and
    /// `update.rs` (the updater is how a broken build gets replaced).
    /// `attachments.rs` is exempt on purpose too: it is content-addressed blob
    /// housekeeping, and gating cache-size/clear would strand disk usage behind a
    /// hidden settings section.
    #[test]
    fn every_box_reaching_feature_module_guards_itself() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
        const EXEMPT: &[&str] = &["sync.rs", "update.rs", "attachments.rs", "shared.rs"];
        let mut offenders = Vec::new();

        for entry in std::fs::read_dir(&dir).expect("commands dir").flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            if path.extension().and_then(|e| e.to_str()) != Some("rs") || EXEMPT.contains(&name) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            // Whitespace stripped entirely, for the reason spelled out in
            // `no_command_appends_events_directly`: rustfmt splits these chains
            // across lines, and collapsing to single spaces matches neither form.
            let flat: String = text.split_whitespace().collect();
            let reaches_box = flat.contains(".box_request(") || flat.contains(".box_url(");
            let guards = flat.contains("require_feature(") || flat.contains("require_any_feature(");
            if reaches_box && !guards {
                offenders.push(name.to_string());
            }
        }

        assert!(
            offenders.is_empty(),
            "these reach the box but never call a feature guard, so they run with \
             their feature switched off: {offenders:#?}"
        );
    }

    /// Nothing in this crate may append an event outside the writer.
    ///
    /// Appending an event and nudging the push debouncer have to happen
    /// together: `pusher::run_loop` blocks on `trigger.notified()` with **no
    /// interval fallback**, so an append that skips the nudge doesn't sync
    /// slowly, it doesn't sync at all — until an unrelated edit, a manual Sync,
    /// or the retry engine happens to fire.
    ///
    /// This started as a rule people remembered, and six sites had already
    /// forgotten it. `EventWriter` pairs the operations; this test is what stops
    /// a seventh.
    ///
    /// ⚠️ **The needle is `store.append`, not `state.event_store.append`.** The
    /// narrower form missed every function that received the store as a
    /// parameter instead of reading it off `AppState`, and the old note here
    /// claimed there was exactly one. There were four: both schedulers,
    /// `import::commit_import_inner`, and `core`'s own
    /// `seed_journal_record_type` — which this scan could never have seen, since
    /// it only walks this crate. All four take an `&EventWriter` now, so the
    /// broad needle has nothing legitimate left to flag and any new match is a
    /// real bypass rather than an accepted exception.
    ///
    /// The one that got away is the reason `EventWriter` lives in `core` rather
    /// than here: a scan bounded by one crate cannot police an append performed
    /// in another.
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
                // Catches `state.event_store.append`, a threaded `event_store`
                // parameter, and `append_batch` alike.
                if flat.contains("store.append") {
                    offenders.push(path.display().to_string());
                }
            }
        }

        walk(&src, &mut offenders);

        // The agent is a second *host* that authors events, so it needs the same
        // chokepoint. Reached by traversal from this crate's manifest dir, which
        // is admittedly the wrong home for it — a scan that polices another crate
        // belongs in `core`. Moving it there means triaging the eight non-test
        // `store.append` sites core and server still hold (auto-import sources,
        // the LLM pipeline, and the two legitimately-exempt sync paths); until
        // that is done, covering the agent here beats not covering it at all.
        let agent_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../agent/src");
        assert!(
            agent_src.is_dir(),
            "agent/src not found at {agent_src:?} — this scan has silently stopped \
             covering the agent, which is exactly the gap it was added to close"
        );
        walk(&agent_src, &mut offenders);

        assert!(
            offenders.is_empty(),
            "these append events without going through EventWriter, so the push \
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
