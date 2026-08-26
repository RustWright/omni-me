//! The one place an outbound `reqwest::Client` is built.
//!
//! Every client in this workspace used to be a bare `reqwest::Client::new()`,
//! which has **no timeout of any kind** — not on connect, not on the response
//! body. That is not merely "a request might hang". Two of these clients sit on
//! single-task loops:
//!
//! - `sync::puller`/`sync::pusher` await their request *inside* one arm of the
//!   scheduler's `tokio::select!`, so one unresponsive connection stalls the
//!   whole 20 s pull loop and the debounced push loop forever — nothing else in
//!   that task runs until the OS gives up, which for a black-holed TCP
//!   connection can be minutes or never.
//! - each auto-import source owns its own spawn loop, so a wedged poll silently
//!   retires that source until the app restarts.
//!
//! A hung request is therefore indistinguishable from "sync is broken" and
//! surfaces no error, no retry and no `NeedsReauth`. Timeouts are what turn it
//! back into an ordinary failure the retry engine already knows how to handle.
//!
//! `no_bare_reqwest_client_in_core` (below) and its `src-tauri` counterpart fail
//! the build if a new call site skips this module.

use std::time::Duration;

/// Total request budget for ordinary API calls — sync, FX, REST auto-import.
///
/// Generous relative to the work (these are small JSON round trips), because
/// the cost of firing early is a spurious failure on a slow mobile link, while
/// the cost of firing late is only a delayed error.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Model inference is slow by nature: a long extraction prompt over a
/// multi-page receipt legitimately runs past a minute, so the ordinary budget
/// would abort real work. Still bounded — an unbounded wait is what this module
/// exists to remove.
pub const LLM_TIMEOUT: Duration = Duration::from_secs(180);

/// Separate from the total budget so a dead host fails fast even on the long
/// LLM budget: unreachable is knowable in seconds, whereas "still generating"
/// is not.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A `ClientBuilder` carrying the timeouts, for callers that need to add
/// something else (default headers, for instance) before building.
///
/// Prefer [`client`] or [`llm_client`]; reach for this only when the extra
/// configuration is genuinely required, and never call `reqwest::Client::builder`
/// directly — a builder that starts here cannot forget the timeout.
pub fn builder(timeout: Duration) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT)
}

/// Client for ordinary API calls. See [`DEFAULT_TIMEOUT`].
pub fn client() -> reqwest::Client {
    build(DEFAULT_TIMEOUT)
}

/// Client for model inference calls. See [`LLM_TIMEOUT`].
pub fn llm_client() -> reqwest::Client {
    build(LLM_TIMEOUT)
}

/// `expect` rather than a fallback to `reqwest::Client::new()`, deliberately.
///
/// `build()` fails only when the TLS backend cannot be initialised, and
/// `Client::new()` is itself `builder().build().expect(..)` — so a "safe"
/// fallback would panic on the same input, one line later, having first thrown
/// away the timeouts. Matching `Client::new()`'s own contract keeps this a drop-in.
fn build(timeout: Duration) -> reqwest::Client {
    builder(timeout)
        .build()
        .expect("failed to initialise the HTTP client (TLS backend unavailable)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_clients_build() {
        let _ = client();
        let _ = llm_client();
    }

    #[test]
    fn the_llm_budget_is_longer_than_the_ordinary_one() {
        // Guards the pairing rather than the numbers: if someone lowers
        // LLM_TIMEOUT to the default they have reintroduced "extraction times
        // out on a long receipt", which reads as a model failure, not a config one.
        assert!(LLM_TIMEOUT > DEFAULT_TIMEOUT);
        assert!(CONNECT_TIMEOUT < DEFAULT_TIMEOUT);
    }

    /// Nothing in `core` may construct a client outside this module.
    ///
    /// The needles are assembled at runtime: written as literals they would
    /// appear in this file's own source and the scan would match itself.
    #[test]
    fn no_bare_reqwest_client_in_core() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();

        // `Client::new` and `Client::builder` — the two ways to get an untimed
        // client. `builder` counts because `.timeout()` is opt-in on it.
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
                // This module *is* the sanctioned construction site.
                if path.file_name().and_then(|f| f.to_str()) == Some("http.rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // Strip whitespace entirely rather than collapsing it to single
                // spaces: rustfmt breaks a long chain across lines, and a
                // collapsed `reqwest::Client ::builder()` matches neither a
                // spaced nor an unspaced needle. The sibling test in
                // `commands/shared.rs` shipped with exactly that hole.
                let flat: String = text.split_whitespace().collect();
                if needles.iter().any(|n| flat.contains(n.as_str())) {
                    offenders.push(path.display().to_string());
                }
            }
        }

        walk(&src, &needles, &mut offenders);
        assert!(
            offenders.is_empty(),
            "these build an HTTP client without a timeout — a hung upstream then \
             stalls the sync loop or an auto-import source forever, with no error \
             surfaced. Use `http::client()` / `http::llm_client()` / \
             `http::builder(..)`: {offenders:#?}"
        );
    }
}
