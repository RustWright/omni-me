mod autosave;
mod bridge;
mod components;
mod continuity;
mod duration;
mod journal_template;
mod note_frontmatter;
mod pages;
mod reorder;
mod sync_refresh;
mod timer;
mod types;
pub mod user_date;

use chrono_tz::Tz;
use dioxus::prelude::*;
use futures::StreamExt;

use components::nav::{NavDrawer, SideNav};
use sync_refresh::SyncRefresh;
use pages::finances::FinancesPage;
use pages::journal::JournalPage;
use pages::notes::NotesPage;
use pages::routines::RoutinesPage;
use pages::settings::SettingsPage;

/// Top-level feature tabs. Order matches the nav display order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Journal,
    Notes,
    Routines,
    Finances,
    Settings,
}

impl Tab {
    /// Stable string key for persistence (1.8b nav restoration). Kept separate
    /// from the display label so renaming a tab in the UI can't silently break
    /// the saved-position format.
    fn as_key(self) -> &'static str {
        match self {
            Tab::Journal => "journal",
            Tab::Notes => "notes",
            Tab::Routines => "routines",
            Tab::Finances => "finances",
            Tab::Settings => "settings",
        }
    }

    fn from_key(s: &str) -> Option<Tab> {
        match s {
            "journal" => Some(Tab::Journal),
            "notes" => Some(Tab::Notes),
            "routines" => Some(Tab::Routines),
            "finances" => Some(Tab::Finances),
            "settings" => Some(Tab::Settings),
            _ => None,
        }
    }
}

/// Left-edge strip width (CSS px) within which a touch may begin a drawer-open
/// swipe (1.12). The matching native `setSystemGestureExclusionRects` keeps
/// Android's back-gesture from stealing swipes in this strip.
const EDGE_SWIPE_START_PX: f64 = 24.0;
/// Rightward travel (CSS px) before an edge-swipe commits to opening the drawer.
const EDGE_SWIPE_OPEN_PX: f64 = 48.0;
/// Leftward travel (CSS px) before a swipe on the *open* drawer commits to
/// closing it — the inverse gesture. Mirrors the scrim-tap close (which stays as
/// the guaranteed fallback).
const EDGE_SWIPE_CLOSE_PX: f64 = 48.0;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut active_tab = use_signal(|| Tab::Journal);
    // Mobile nav drawer open/close (1.11). Desktop uses the persistent SideNav,
    // so this only drives the small-screen slide-in.
    let mut drawer_open = use_signal(|| false);
    // Drawer swipe tracking (1.12): `Some(start_x)` once a tracked touch begins —
    // in the left edge strip when the drawer is *closed* (candidate open-swipe),
    // or anywhere when it's *open* (candidate close-swipe). Cleared on
    // open/close/end. `peek` everywhere — the gesture mutates state but nothing
    // renders off this signal.
    let mut swipe_start_x = use_signal(|| None::<f64>);

    // Continuity store (Phase 1.1): root-held per-page editing state that
    // survives page unmount on tab switch. Pages read it via `use_continuity`.
    let continuity_store = continuity::use_continuity_provider();

    // Known-account suggestions: one fetch of the `known_accounts` union, shared
    // by every `AccountInput` typeahead. Consumers read it via `use_context`.
    components::account_input::use_account_suggestions_provider();

    // Live-refresh on inbound sync (see `sync_refresh`). The backend applies
    // auto-pulled remote events into the local DB and emits `sync:applied`, but
    // the WASM frontend has no event-listen binding, so the open page never
    // re-queried — remote edits only showed after a manual navigation or Sync.
    // Bridge each emit into an epoch bump that read views subscribe to via
    // `use_sync_epoch`, so they re-fetch automatically when a pull lands.
    let mut sync_epoch = use_signal(|| 0u64);
    use_context_provider(|| SyncRefresh(sync_epoch));
    use_hook(move || {
        // The JS event callback fires outside any Dioxus scope, where writing a
        // signal directly would panic — so it only nudges an unbounded channel.
        // The drain loop runs inside the runtime (via `spawn`), where the actual
        // signal bump is safe.
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
        bridge::listen_backend_event("sync:applied", move || {
            let _ = tx.unbounded_send(());
        });
        spawn(async move {
            while rx.next().await.is_some() {
                // Wrapping bump — consumers only care that the value changed.
                let next = sync_epoch.peek().wrapping_add(1);
                sync_epoch.set(next);
            }
        });
    });

    // 1.8b: restore the last-open tab once the store's disk snapshot has loaded.
    // Runs before any user interaction. The pending-share intake below still
    // wins when a capture is waiting — it sets Finances explicitly.
    use_future(move || async move {
        while !continuity_store.loaded_peek() {
            timer::sleep_ms(20).await;
        }
        if let Some(tab) = continuity_store
            .nav_peek()
            .tab
            .as_deref()
            .and_then(Tab::from_key)
        {
            active_tab.set(tab);
        }
    });

    // Timezone: default to UTC, load from backend on mount.
    let mut tz_signal = use_signal(|| Tz::UTC);
    use_context_provider(|| tz_signal);
    use_future(move || async move {
        if let Ok(info) = bridge::invoke_get_timezone().await
            && let Ok(tz) = info.timezone.parse::<Tz>()
        {
            tz_signal.set(tz);
        }
    });

    // Pending Android share-target intake (Phase 3.3). The Kotlin handler
    // writes bytes to filesDir whenever a SEND intent arrives; we pull on
    // mount and switch to Finances so the capture flow picks it up.
    let pending_share: Signal<Option<types::PendingShareCapture>> = use_signal(|| None);
    use_context_provider(|| pending_share);
    let mut pending_share_mut = pending_share;
    use_future(move || async move {
        if let Ok(Some(capture)) = bridge::invoke_take_pending_share_intent().await {
            pending_share_mut.set(Some(capture));
            active_tab.set(Tab::Finances);
        }
    });

    rsx! {
        // Required for Dioxus 0.7 Tailwind integration
        link { rel: "stylesheet", href: asset!("/assets/tailwind.css") }

        // Shell: side nav (desktop) + content + mobile drawer.
        // `md:flex-row` swaps to side-by-side at 768px and above.
        div { class: "flex flex-col md:flex-row h-screen w-screen m-0 p-0 font-sans bg-obsidian-bg text-obsidian-text overflow-hidden",

            // Left-edge swipe to open the drawer (1.12). We don't preventDefault,
            // so normal scrolling/typing is untouched; we only act on a touch
            // that *starts* in the edge strip while the drawer is closed.
            ontouchstart: move |e| {
                let start = e.touches().first().map(|t| t.client_coordinates().x);
                if *drawer_open.peek() {
                    // Open: track any touch so a leftward swipe can close it.
                    swipe_start_x.set(start);
                } else {
                    // Closed: only track touches starting in the left edge strip.
                    swipe_start_x.set(start.filter(|x| *x <= EDGE_SWIPE_START_PX));
                }
            },
            ontouchmove: move |e| {
                // Copy the start out first so the `peek` guard is released before
                // the `set` below (can't hold an immutable borrow across a write).
                let Some(start) = *swipe_start_x.peek() else {
                    return;
                };
                // Extract the x in the same statement so the temporary touch
                // list doesn't outlive the borrow (the open path did this inline).
                let Some(x) = e.touches().first().map(|t| t.client_coordinates().x) else {
                    return;
                };
                if *drawer_open.peek() {
                    // Open drawer: leftward travel past the threshold closes it.
                    if start - x >= EDGE_SWIPE_CLOSE_PX {
                        drawer_open.set(false);
                        swipe_start_x.set(None);
                    }
                } else if x - start >= EDGE_SWIPE_OPEN_PX {
                    // Closed drawer: rightward travel from the edge opens it.
                    drawer_open.set(true);
                    swipe_start_x.set(None);
                }
            },
            ontouchend: move |_| swipe_start_x.set(None),

            // Sidebar — visible at md+
            SideNav {
                active: *active_tab.read(),
                on_switch: move |tab: Tab| {
                    active_tab.set(tab);
                    continuity_store.update_nav(|n| n.tab = Some(tab.as_key().to_string()));
                },
            }

            // Main column: sticky header (sync chip) + scrollable content.
            // Bottom padding only applies on mobile so the bottom nav doesn't
            // overlap the last item.
            main { class: "flex-1 flex flex-col overflow-hidden",
                header { class: "flex items-center justify-end gap-3 px-4 md:px-6 py-3 border-b border-white/5 bg-obsidian-bg/80 backdrop-blur-sm",
                    // Hamburger — mobile only (desktop has the persistent SideNav).
                    // `mr-auto` keeps it hard-left while the sync chip stays right;
                    // when hidden at md+, `justify-end` keeps the chip right.
                    button {
                        class: "md:hidden mr-auto p-1.5 -ml-1.5 rounded-md text-obsidian-text-muted hover:text-obsidian-text hover:bg-white/5 transition-colors",
                        "aria-label": "Open navigation",
                        onclick: move |_| drawer_open.set(true),
                        svg { class: "w-6 h-6", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M4 6h16M4 12h16M4 18h16" }
                        }
                    }
                    components::sync_status::SyncStatusIndicator {}
                }
                // Mobile bottom padding clears the Android gesture/system-nav
                // (and keyboard) inset so the last item in any scroll view is
                // reachable. Desktop keeps its plain `pb-6` (SideNav doesn't
                // overlap the content column). Values live in
                // `input.css::.pb-mobile-nav`.
                div { class: "flex-1 overflow-y-auto p-4 md:p-6 pb-mobile-nav md:pb-6",
                    match *active_tab.read() {
                        Tab::Journal => rsx! { JournalPage {} },
                        Tab::Notes => rsx! { NotesPage {} },
                        Tab::Routines => rsx! { RoutinesPage {} },
                        Tab::Finances => rsx! { FinancesPage {} },
                        Tab::Settings => rsx! { SettingsPage {} },
                    }
                }
            }

            // Mobile slide-in drawer — replaces the old bottom nav (1.11).
            NavDrawer {
                active: *active_tab.read(),
                open: *drawer_open.read(),
                on_switch: move |tab: Tab| {
                    active_tab.set(tab);
                    continuity_store.update_nav(|n| n.tab = Some(tab.as_key().to_string()));
                },
                on_close: move |_| drawer_open.set(false),
            }
        }
    }
}

