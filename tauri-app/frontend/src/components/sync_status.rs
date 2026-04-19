use dioxus::prelude::*;

use crate::bridge;
use crate::types::{SyncState, SyncStatusSnapshot};

/// Small header chip that reflects the Track D (Phase 2.6) background sync
/// status. Polls every 5s — the command is cheap (reads a single in-memory
/// state variable) and a short poll keeps the UI honest without a dedicated
/// event stream. When Track D lands an `events` channel, this can switch to
/// `tauri-plugin-event` subscriptions.
///
/// If the backend command is unregistered (Track D hasn't merged), we treat
/// the error as `Idle` so the UI doesn't flash a spurious red state — the
/// actual sync still works through the manual Settings -> Sync Now button.
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

    let snap = snapshot.read();
    let current = snap.status;
    let (label, dot_class, text_class, animated) = match current {
        SyncState::Idle => (
            "Synced",
            "bg-green-500",
            "text-obsidian-text-muted",
            false,
        ),
        SyncState::Syncing => (
            "Syncing",
            "bg-obsidian-accent",
            "text-obsidian-accent",
            true,
        ),
        SyncState::Retrying => (
            "Retrying",
            "bg-yellow-500",
            "text-yellow-500",
            true,
        ),
        SyncState::Error => (
            "Sync error",
            "bg-red-500",
            "text-red-400",
            false,
        ),
    };

    let dot_pulse = if animated {
        "animate-pulse"
    } else {
        ""
    };

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

#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: i32) {
    if let Some(window) = web_sys::window() {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
    // If window is None we're not in a browser context; drop silently so the
    // caller's loop still advances on a future poll.
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_ms(ms: i32) {
    // Native fallback — exists so non-wasm check builds don't fail even
    // though the frontend is only ever built as wasm.
    let _ = ms;
}
