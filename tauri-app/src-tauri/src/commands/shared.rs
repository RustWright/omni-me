use chrono::Utc;

use omni_me_core::config::Feature;
use omni_me_core::events::{Event, EventStore, EventType, NewEvent};

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
    if features.iter().any(|f| state.boot_features.contains(f)) {
        return Ok(());
    }
    let refusal = feature_off_message(features);
    tracing::warn!(%refusal, "command refused: feature off");
    Err(refusal)
}

/// Refuse to author an event whose feature is switched off.
///
/// Called from the shared append tails, so it covers **every** local write path
/// in one place — see [`EventType::authoring_features`] for why the map lives on
/// the event type rather than at each command. Inbound events are untouched: the
/// sync pull path does not come through here, so a device keeps a complete log of
/// features it has switched off.
fn guard_event_type(state: &AppState, event_type: &str) -> Result<(), String> {
    // An unparseable type is not this guard's business. The event store's own
    // validation owns it, and answering here would report a typo as "feature off".
    let Ok(parsed) = event_type.parse::<EventType>() else {
        return Ok(());
    };
    let features = parsed.authoring_features();
    if features.is_empty() {
        return Ok(());
    }
    require_any_feature(state, features)
}

/// Pure so the wording is testable without standing up an `AppState`, following
/// `check_wipe_confirmation`'s precedent.
fn feature_off_message(features: &[Feature]) -> String {
    let names: Vec<&str> = features.iter().map(|f| f.label()).collect();
    let subject = match names.as_slice() {
        [] => "This feature".to_string(),
        [one] => format!("{one} is"),
        [rest @ .., last] => format!("{} and {last} are", rest.join(", ")),
    };
    format!("{subject} switched off. Turn it back on in Settings, then restart the app.")
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
    guard_event_type(state, &event.event_type)?;

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
    // All-or-nothing, like the server's push validation: a batch that is half a
    // disabled feature's would otherwise land partially.
    for event in &events {
        guard_event_type(state, &event.event_type)?;
    }

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
    use omni_me_core::config::Feature;
    use omni_me_core::events::EventType;

    /// Every command that reaches the box on a feature's behalf must refuse when
    /// that feature is off.
    ///
    /// The **write** side needs no scan: `authoring_features` is an exhaustive
    /// match, and `guard_event_type` sits in the append tails that
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

    /// The write-side map must not quietly acquire an unowned event type.
    ///
    /// The match in `authoring_features` is exhaustive, so a new variant cannot
    /// compile without an arm — but an arm returning `&[]` is the easy way to
    /// silence it, and `&[]` means ungated forever. This pins the four that are
    /// legitimately unowned so a fifth has to be argued for here.
    ///
    /// `record_type_declared` is the fourth: a declaration is the shape of your
    /// own data, so it must not depend on the feature that renders it being on,
    /// and the first-run seed is emitted at startup before any feature has been
    /// consulted. `events::registry` makes the same call on the read side, where
    /// `RecordTypeProjection` sits in `NEVER_GATED` beside config's.
    #[test]
    fn only_the_four_app_level_events_are_unowned() {
        let unowned: Vec<String> = EventType::ALL
            .iter()
            .filter(|t| t.authoring_features().is_empty())
            .map(|t| t.to_string())
            .collect();
        assert_eq!(
            unowned,
            vec![
                "data_wiped",
                "feedback_captured",
                "config_set",
                "record_type_declared"
            ],
            "an event type became unowned (ungated) — or a legitimately unowned \
             one gained an owner; if this is deliberate, update this list and say why"
        );
    }

    /// Each feature must own at least one event type, or turning it off would
    /// leave its write path unguarded.
    #[test]
    fn every_feature_owns_at_least_one_event_type() {
        for feature in omni_me_core::config::ALL_FEATURES {
            assert!(
                EventType::ALL
                    .iter()
                    .any(|t| t.authoring_features().contains(feature)),
                "{feature} owns no event type, so nothing guards its writes"
            );
        }
    }

    #[test]
    fn the_refusal_names_the_feature_and_says_what_to_do() {
        let one = super::feature_off_message(&[Feature::Finances]);
        assert!(one.contains("Finances is switched off"), "{one}");
        assert!(one.contains("Settings"), "{one}");
        assert!(one.contains("restart"), "{one}");

        let two = super::feature_off_message(&[Feature::Journal, Feature::Notes]);
        assert!(two.contains("Journal and Notes are switched off"), "{two}");
    }

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
    /// a seventh.
    ///
    /// **Known blind spot, deliberately left open.** The scan matches the
    /// literal `state.event_store.append`, so a function that receives the
    /// store as a `&dyn EventStore` parameter is invisible to it. Exactly one
    /// does: `import::commit_import_inner`, which threads the store *and* an
    /// `Option<&PushDebouncer>` so its tests can run without an `AppState`, and
    /// nudges by hand once the batch lands. Widening the needle to a bare
    /// `event_store.append` would flag that correct site on every run, so the
    /// audit lives here instead: a second parameter-threaded appender must nudge
    /// the same way, and this note is what tells you the test won't catch it.
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
