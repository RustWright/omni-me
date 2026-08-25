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

/// Bridges each page's in-app nav into the app-wide hardware/gesture-back
/// handling (#372). The Android `MainActivity` dispatches an `omni:back` DOM
/// event on a back press; the root ([`App`]) decides what "back" pops in this
/// order — open drawer → the active page's own drill-down → non-home tab → (at
/// home root) let the OS background the app. Pages don't know about any of that:
/// they just call [`use_page_back`] to report how deep they are and to receive a
/// "pop one level" pulse.
///
/// Provided once at the root; the active page reads it via [`use_page_back`].
#[derive(Clone, Copy)]
pub struct BackNav {
    /// In-page levels the active page can still pop (0 = at its own root). The
    /// active page keeps this current; the root ORs it with the drawer / tab
    /// state to publish `window.__omniCanGoBack`.
    page_depth: Signal<u32>,
    /// Bumped by the root to ask the active page to pop exactly one level. The
    /// page reacts in its own scope (see [`use_page_back`]), so all view-signal
    /// writes stay where they belong — the root never touches page internals.
    pop_seq: Signal<u32>,
}

/// Wire a page's in-app nav into hardware/gesture-back handling (#372).
///
/// `depth` returns the page's current poppable depth (0 = at its root) — it
/// reads the page's own `view` signal, so this stays reactive and the published
/// `can-go-back` flag tracks every drill-down. `on_pop` pops exactly one level
/// and runs in the page's scope (safe to mutate the page's view signals), fired
/// once per hardware-back while `depth() > 0`.
///
/// Pages with no drill-down simply don't call this (their depth stays 0, so a
/// back press falls through to the tab/app-background behavior at the root).
pub fn use_page_back(depth: impl Fn() -> u32 + Copy + 'static, on_pop: impl FnMut() + 'static) {
    let nav = use_context::<BackNav>();
    let mut page_depth = nav.page_depth;
    let pop_seq = nav.pop_seq;

    // Publish the page's depth upward whenever its view changes (reactive read
    // of the page signal inside `depth()`).
    use_effect(move || page_depth.set(depth()));

    // React to the root's pop pulses. `handled` dedupes the initial effect run
    // (seq 0) and any re-run that isn't a fresh bump, so we pop once per press.
    let mut on_pop = on_pop;
    let mut handled = use_signal(|| 0u32);
    use_effect(move || {
        let seq = *pop_seq.read();
        if seq > *handled.peek() {
            handled.set(seq);
            on_pop();
        }
    });
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

    // Auto-hide top bar (#7): the header (sync chip + mobile hamburger) slides
    // away when scrolling *down* through content and returns on scroll *up* or at
    // the top, reclaiming the strip while reading. `last_scroll_top` remembers the
    // previous offset so we can tell the direction from the content column's
    // `onscroll`. The mobile hamburger hiding with it is fine — scroll up (or the
    // 1.12 edge-swipe) brings the nav back.
    let mut header_hidden = use_signal(|| false);
    let mut last_scroll_top = use_signal(|| 0.0_f64);

    // Continuity store (Phase 1.1): root-held per-page editing state that
    // survives page unmount on tab switch. Pages read it via `use_continuity`.
    let continuity_store = continuity::use_continuity_provider();

    // Hardware/gesture-back plumbing (#372). The active page reports its
    // drill-down depth into `page_depth` and pops one level when `pop_seq`
    // bumps; the root orchestrates below. Provided as `BackNav` context.
    let mut page_depth = use_signal(|| 0u32);
    let mut pop_seq = use_signal(|| 0u32);
    use_context_provider(|| BackNav { page_depth, pop_seq });

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

    // Hardware/gesture-back handler (#372). The Android `MainActivity`
    // dispatches an `omni:back` DOM event on a back press; we drain it here (same
    // channel→spawn pattern as `sync:applied` above — the JS callback fires
    // outside any Dioxus scope, so it only nudges a channel and the in-scope
    // drain does the signal writes). Precedence: close the drawer → pop the
    // active page's drill-down → return to the home tab → (nothing left) let the
    // OS background the app. That last case never reaches here: `MainActivity`
    // only dispatches `omni:back` when `window.__omniCanGoBack` is true, which
    // the effect below keeps false at the home root.
    use_hook(move || {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
        bridge::listen_window_event("omni:back", move || {
            let _ = tx.unbounded_send(());
        });
        spawn(async move {
            while rx.next().await.is_some() {
                if *drawer_open.peek() {
                    drawer_open.set(false);
                } else if *page_depth.peek() > 0 {
                    // Ask the active page to pop one level (it reacts in its own
                    // scope via `use_page_back`).
                    let next = pop_seq.peek().wrapping_add(1);
                    pop_seq.set(next);
                } else if *active_tab.peek() != Tab::Journal {
                    active_tab.set(Tab::Journal);
                    continuity_store.update_nav(|n| n.tab = Some(Tab::Journal.as_key().to_string()));
                }
            }
        });
    });

    // Publish the app's can-go-back state to `window.__omniCanGoBack` so the
    // native back handler can decide pop-in-app vs background-the-app
    // synchronously (#372). Reactive on drawer / page-depth / tab, so the flag
    // is always current when a back press arrives.
    use_effect(move || {
        let can = *drawer_open.read() || *page_depth.read() > 0 || *active_tab.read() != Tab::Journal;
        bridge::set_can_go_back(can);
    });

    // Known-account suggestions: the shared `known_accounts` union behind every
    // `AccountInput` typeahead. Registered *after* the `SyncRefresh` provider
    // above so it can subscribe to `sync_epoch` and re-fetch when a pull lands —
    // on a fresh device the first fetch runs before the event backfill has
    // populated the ledger, so a one-shot fetch would leave the list empty and
    // flag every real account "No such account" until an app restart. Consumers
    // read it via `use_context`.
    components::account_input::use_account_suggestions_provider();

    // Reveal the top bar whenever the tab changes (#7) — a new page scrolls from
    // the top, so a header left hidden by the previous page's scroll would be
    // stuck until the user scrolled up. Resetting here keeps it predictable.
    use_effect(move || {
        let _ = active_tab.read(); // re-run on tab switch
        header_hidden.set(false);
        last_scroll_top.set(0.0);
        // Clean slate for hardware-back (#372): the newly-mounted page re-reports
        // its own depth via `use_page_back`; this just clears any stale value from
        // the outgoing page in the unmount→mount gap.
        page_depth.set(0);
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
                // Auto-hiding header (#7): collapses its height + padding (not just
                // a transform) so the content reclaims the strip. `overflow-hidden`
                // keeps the chip clipped mid-collapse; the border fades with it.
                header {
                    class: if *header_hidden.read() {
                        "flex items-center justify-end gap-3 px-4 md:px-6 bg-obsidian-bg/80 backdrop-blur-sm overflow-hidden transition-all duration-300 max-h-0 py-0 opacity-0 border-b border-transparent"
                    } else {
                        "flex items-center justify-end gap-3 px-4 md:px-6 bg-obsidian-bg/80 backdrop-blur-sm overflow-hidden transition-all duration-300 max-h-16 py-3 opacity-100 border-b border-white/5"
                    },
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
                div {
                    class: "flex-1 overflow-y-auto p-4 md:p-6 pb-mobile-nav md:pb-6",
                    // Scroll-direction drives the auto-hiding header (#7). Small
                    // thresholds debounce jitter; always reveal near the very top.
                    onscroll: move |e| {
                        let cur = e.scroll_top();
                        // Only auto-hide when the content clearly overflows. On a
                        // barely-scrollable page, collapsing the ~90px header
                        // shrinks the scroll range and clamps scrollTop, which the
                        // direction logic reads as "scrolled up" → reveal →
                        // un-clamp → hide … a constant flip (on-device batch 2
                        // jitter report, 2026-08-23). Below this range keep the
                        // header pinned — there's ample room there anyway.
                        if e.scroll_height() - e.client_height() < 150 {
                            header_hidden.set(false);
                            last_scroll_top.set(cur);
                            return;
                        }
                        let last = *last_scroll_top.peek();
                        if cur <= 8.0 {
                            header_hidden.set(false);
                        } else if cur - last > 6.0 {
                            header_hidden.set(true);
                        } else if last - cur > 6.0 {
                            header_hidden.set(false);
                        }
                        last_scroll_top.set(cur);
                    },
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

