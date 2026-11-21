//! The command-level feature guard (refusing a command outright), plus thin delegation to
//! `omni_me_core::events::EventWriter`, which keeps the append guard, projection fold and push
//! nudge together for every binary that authors events.

use omni_me_core::config::Feature;
use omni_me_core::events::{Event, EventType, NewEvent, feature_off_message};

use crate::AppState;

/// Percent-encode one path segment or query value for a request to the server.
/// Every value interpolated into a path goes through this: a `/` or `?` in it
/// would otherwise reach a different route. Local rather than a new dependency.
pub(crate) fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// GET `path` from the server and decode its JSON. `label` names the read in each
/// error ("<label> fetch", "<label>: server returned", "<label> decode").
pub(crate) async fn get_json<T: serde::de::DeserializeOwned>(
    state: &AppState,
    path: &str,
    label: &str,
) -> Result<T, String> {
    let resp = state
        .box_request(reqwest::Method::GET, path)
        .await
        .send()
        .await
        .map_err(|e| format!("{label} fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("{label}: server returned {}", resp.status()));
    }
    resp.json::<T>()
        .await
        .map_err(|e| format!("{label} decode: {e}"))
}

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
    #[test]
    fn a_path_value_cannot_reach_another_route() {
        assert_eq!(super::encode("gmail_personal"), "gmail_personal");
        assert_eq!(super::encode("a/b?c d#"), "a%2Fb%3Fc%20d%23");
    }

    /// Every command that reaches the box on a feature's behalf must refuse when
    /// that feature is off.
    ///
    /// Writes are guarded inside `EventWriter`; outbound HTTP has no chokepoint, so this scan
    /// checks each box-reaching module mentions a guard (somewhere, not per function).
    /// Exempt, belonging to no one feature: `sync.rs` (must work with everything off),
    /// `update.rs` (replaces a broken build), `attachments.rs` (blob housekeeping), `jobs.rs`.
    #[test]
    fn every_box_reaching_feature_module_guards_itself() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
        const EXEMPT: &[&str] = &[
            "sync.rs",
            "update.rs",
            "attachments.rs",
            "shared.rs",
            "jobs.rs",
        ];
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
            // `get_json(` is the shared GET; a module calling it reaches the box too.
            let reaches_box = flat.contains(".box_request(")
                || flat.contains(".box_url(")
                || flat.contains("get_json(");
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
    /// An append that skips the push nudge does not sync at all (`pusher::run_loop` has no
    /// interval fallback), so every append goes through `EventWriter`.
    /// The needle is the broad `store.append`, so a store passed as a parameter is caught too;
    /// any match is a real bypass. A scan bounded by one crate is why `EventWriter` lives in core.
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
    /// The `src-tauri` half of `omni_me_core::http`'s scan: without a timeout the phone hangs on
    /// an unresponsive box and reads as frozen. Never spell the constructors out in a comment
    /// here: this file is in scope, and the scan would match itself.
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
    /// The bearer token rides on `box_request`, so a command using the raw client sends an
    /// unauthenticated request.
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
