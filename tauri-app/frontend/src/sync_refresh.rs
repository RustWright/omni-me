//! Inbound-sync live-refresh plumbing.
//!
//! The backend auto-pulls remote events every ~20s, applies them to the local
//! DB, and emits a `sync:applied` Tauri event meaning "new data landed". The
//! WASM frontend imports only `__TAURI__.core.invoke` — it has no event-listen
//! binding — so that nudge used to go nowhere: pulled data sat in the local DB
//! until the user navigated or hit manual Sync (the "nothing syncs to the
//! desktop" symptom). `main.rs` now bridges each `sync:applied` into a bump of
//! the shared epoch signal held here.
//!
//! Read views subscribe by reading [`use_sync_epoch`] inside their fetch
//! `use_effect`, so a pull makes them re-query automatically.
//!
//! Despite the name, the epoch means "**the local DB changed under you**", not
//! strictly "a pull landed". A LOCAL write bumps it too when it commits data
//! another mounted view is already displaying — the review inbox does this after
//! committing a batch, because the Ledger's cached page would otherwise keep
//! showing pre-approval rows until a filter Apply. Prefer this over threading a
//! bespoke signal between two views: any read view that subscribes gets every
//! source of change for free. Active editors
//! deliberately do NOT subscribe — re-loading an open note mid-edit would
//! clobber unsaved keystrokes, so open-entry live-update is left to a manual
//! navigation / save (a deliberate follow-up, not this wiring).

use dioxus::prelude::*;

/// Shared inbound-sync epoch, provided once at the app root and bumped a step
/// per applied pull. A newtype so `use_context` can't collide with any other
/// `Signal<u64>` a component might provide.
#[derive(Clone, Copy)]
pub struct SyncRefresh(pub Signal<u64>);

/// Subscribe the calling component to inbound-sync refreshes. Read the returned
/// signal inside a `use_effect` (`let _ = use_sync_epoch().read();`, or capture
/// it and `.read()` in the effect body) to re-run that effect whenever a
/// background pull applies new events.
///
/// Falls back to an inert local signal when no [`SyncRefresh`] provider is
/// mounted (e.g. a component rendered in isolation), so consumers never panic.
/// Being a hook, it must be called unconditionally at the top of a component,
/// like any other `use_*`.
pub fn use_sync_epoch() -> Signal<u64> {
    // Allocated unconditionally to keep hook order stable; only used as the
    // fallback when the app root's provider is absent.
    let fallback = use_signal(|| 0u64);
    try_consume_context::<SyncRefresh>()
        .map(|sr| sr.0)
        .unwrap_or(fallback)
}
