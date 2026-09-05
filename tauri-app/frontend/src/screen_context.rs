//! What the open screen is showing — read only when a problem report is filed.
//!
//! **Why this is not the continuity store.** `continuity.rs` persists what would
//! be *lost* on an unmount or an Android app-kill, so it holds unsaved input and
//! writes to disk on a debounce. A report wants what was *shown*, which is a
//! different set: it includes loaded and derived state that was never at risk
//! (a cached figure, a result count, a selected range) and excludes plenty that
//! was. Widening the continuity store to cover reporting would pay disk churn
//! and an eviction policy on every keystroke for a read that happens maybe once
//! a week.
//!
//! So pages publish into a memory-only signal instead, and only the capture
//! modal reads it.
//!
//! **The rule for a describer: summarise, never quote.** A report may be read by
//! someone other than the person who filed it, and Settings holds the server
//! token field and the LLM key state — so it reports which section was open and
//! nothing else. The editor buffer is the one deliberate exception, because
//! quoting the draft is the entire value of an editor report; it is also the
//! line the modal lets the user drop before sending.

use dioxus::prelude::*;

/// A page's own account of what it is displaying.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenReport {
    /// Stable coordinate for the surface, e.g. `notes:edit`, `finances:ledger`.
    /// Matches the vocabulary `NavState` persists so the two never disagree.
    pub screen: String,
    /// The identity the screen is addressing, when it has one — a note ULID, a
    /// journal date. Separate from `screen` so a reader can look it up.
    pub screen_ref: Option<String>,
    /// Short human-readable summary, one fact per line.
    pub detail: Option<String>,
}

/// Shared screen description, provided once at the app root. A newtype so
/// `use_context` cannot collide with another `Signal<ScreenReport>`, matching
/// [`crate::sync_refresh::SyncRefresh`]'s reasoning.
#[derive(Clone, Copy)]
pub struct ScreenContext(pub Signal<ScreenReport>);

/// Read the current screen description. Used by the capture modal; a page that
/// wants to *publish* one calls [`use_publish_screen_report`] instead.
///
/// Falls back to an inert local signal when no provider is mounted, so a
/// component rendered in isolation never panics — same contract as
/// [`crate::sync_refresh::use_sync_epoch`].
pub fn use_screen_report() -> Signal<ScreenReport> {
    let fallback = use_signal(ScreenReport::default);
    try_consume_context::<ScreenContext>()
        .map(|sc| sc.0)
        .unwrap_or(fallback)
}

/// Publish this page's description, recomputed whenever the state it reads
/// changes.
///
/// `build` runs inside a `use_effect`, so every signal it reads becomes a
/// dependency — a page describes itself by writing the closure naturally and
/// gets re-publishing for free. The equality check before writing keeps an
/// unchanged description from waking the modal's subscribers on every render.
///
/// Being a hook it must be called unconditionally at the top of a component.
pub fn use_publish_screen_report(build: impl Fn() -> ScreenReport + 'static) {
    let ctx = try_consume_context::<ScreenContext>();
    use_effect(move || {
        let report = build();
        if let Some(ScreenContext(mut signal)) = ctx
            && *signal.peek() != report
        {
            signal.set(report);
        }
    });
}

/// Render a byte count as a short "N chars" phrase, or `None` when empty.
///
/// Shared by the describers so an empty draft reads the same everywhere — a
/// report saying "unsaved draft (0 chars)" is noise that looks like a finding.
pub fn describe_len(label: &str, text: &str) -> Option<String> {
    let n = text.chars().count();
    (n > 0).then(|| format!("{label} ({n} chars)"))
}
