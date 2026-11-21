//! Inbound-sync live-refresh plumbing.
//!
//! An epoch meaning "the local DB changed under you": `main.rs` bumps it on each `sync:applied`,
//! and read views re-query by reading [`use_sync_epoch`] in their fetch effect. A local write
//! that changes what another view shows bumps it through [`bump_sync_epoch`]; any write that can
//! shrink a badge-counted queue must (`core::approvals::summary`). Editors never subscribe:
//! a reload mid-edit would clobber unsaved keystrokes.

use dioxus::prelude::*;

/// Shared inbound-sync epoch, provided once at the app root and bumped a step
/// per applied pull. A newtype so `use_context` can't collide with any other
/// `Signal<u64>` a component might provide.
#[derive(Clone, Copy)]
pub struct SyncRefresh(pub Signal<u64>);

/// Read the returned signal inside a `use_effect` to re-run it on every refresh. Inert without a
/// [`SyncRefresh`] provider, so consumers never panic; a hook, so call it unconditionally.
pub fn use_sync_epoch() -> Signal<u64> {
    // Allocated unconditionally to keep hook order stable; only used as the
    // fallback when the app root's provider is absent.
    let fallback = use_signal(|| 0u64);
    try_consume_context::<SyncRefresh>()
        .map(|sr| sr.0)
        .unwrap_or(fallback)
}

/// Announce that a local write changed data other mounted views are showing.
///
/// The inbound half ticks only when another device's change arrives, never on a timer, so a
/// local write must call this when a different view counts or lists what it changed.
/// Take the signal with [`use_sync_epoch`] up front and call this after the `await`.
pub fn bump_sync_epoch(mut epoch: Signal<u64>) {
    // Wrapping: consumers only ever compare for change, never for order.
    let next = epoch.peek().wrapping_add(1);
    epoch.set(next);
}
