use dioxus::prelude::*;

use crate::bridge;
use crate::timer::sleep_ms;
use crate::types::{SyncState, SyncStatusSnapshot};

/// Header chip for background sync status, polled every 5 s (the command reads one in-memory
/// value). An error reads as `Idle` rather than flashing red.
#[component]
pub fn SyncStatusIndicator() -> Element {
    let mut snapshot = use_signal(SyncStatusSnapshot::default);

    use_future(move || async move {
        loop {
            match bridge::invoke_get_sync_status().await {
                Ok(s) => snapshot.set(s),
                Err(_) => snapshot.set(SyncStatusSnapshot::default()),
            }
            sleep_ms(5_000).await;
        }
    });

    // Inbound restore outranks push status, which is `Idle` during a backfill
    // (`main.rs::RestoreProgress`). Show what is left, ticking down, so a stalled restore reads
    // as a frozen number rather than an animation that means nothing.
    let restoring = *use_context::<crate::RestoreProgress>().0.read();
    if restoring > 0 {
        return rsx! {
            div {
                class: "flex items-center gap-2 text-xs font-medium",
                title: "Restoring data from the server",
                aria_label: "Restoring, {restoring} events left",
                span { class: "w-2 h-2 rounded-full bg-obsidian-accent animate-pulse" }
                span { class: "text-obsidian-accent", "Restoring… {restoring} events left" }
            }
        };
    }

    let snap = snapshot.read();
    let current = snap.status;
    let (label, dot_class, text_class, animated) = match current {
        SyncState::Idle => ("Synced", "bg-success", "text-obsidian-text-muted", false),
        SyncState::Syncing => (
            "Syncing",
            "bg-obsidian-accent",
            "text-obsidian-accent",
            true,
        ),
        SyncState::Retrying => ("Retrying", "bg-warn", "text-warn", true),
        SyncState::Error => ("Sync error", "bg-error", "text-error", false),
    };

    let dot_pulse = if animated { "animate-pulse" } else { "" };

    rsx! {
        div {
            class: "flex items-center gap-2 text-xs font-medium",
            title: "Background sync status",
            aria_label: "Sync status: {label}",
            span { class: "w-2 h-2 rounded-full {dot_class} {dot_pulse}" }
            span { class: "{text_class}", "{label}" }
        }
    }
}
