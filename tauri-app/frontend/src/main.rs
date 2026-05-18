mod bridge;
mod components;
mod duration;
mod journal_template;
mod pages;
mod reorder;
mod timer;
mod types;
pub mod user_date;

use chrono_tz::Tz;
use dioxus::prelude::*;

use components::nav::{BottomNav, SideNav};
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

fn main() {
    dioxus::launch(App);
}

/// Update the document's `<meta name="viewport">` to include `viewport-fit=cover`
/// (without disturbing the existing `width=device-width, initial-scale=1`).
///
/// Why this lives in code rather than the static `index.html`: the HTML
/// shipped by `dx serve` / `dx build` is generated from a Dioxus template we
/// don't currently customize. Mutating the meta tag from WASM on first mount
/// is a one-line fix that survives Dioxus template upgrades.
///
/// Without `viewport-fit=cover`, Android WebView constrains the entire
/// viewport to the system "safe area" — so `env(safe-area-inset-bottom)`
/// returns 0 and the BottomNav still appears flush with the device edge,
/// behind the gesture bar. With it, the WebView paints edge-to-edge and
/// `env()` returns the real inset that the nav uses to lift itself.
fn install_viewport_fit_cover() {
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Some(meta) = document.query_selector("meta[name=\"viewport\"]").ok().flatten() else {
        return;
    };
    let current = meta.get_attribute("content").unwrap_or_default();
    if current.contains("viewport-fit=cover") {
        return;
    }
    let next = if current.is_empty() {
        "width=device-width, initial-scale=1, viewport-fit=cover".to_string()
    } else {
        format!("{current}, viewport-fit=cover")
    };
    let _ = meta.set_attribute("content", &next);
}

#[component]
fn App() -> Element {
    let mut active_tab = use_signal(|| Tab::Journal);

    // Runs exactly once per mount — the meta tag mutation is idempotent
    // (early-returns if `viewport-fit=cover` is already present) so even
    // double-mount under HMR is safe.
    use_hook(install_viewport_fit_cover);

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

        // Shell: side nav (desktop) + content + bottom nav (mobile).
        // `md:flex-row` swaps to side-by-side at 768px and above.
        div { class: "flex flex-col md:flex-row h-screen w-screen m-0 p-0 font-sans bg-obsidian-bg text-obsidian-text overflow-hidden",

            // Sidebar — visible at md+
            SideNav {
                active: *active_tab.read(),
                on_switch: move |tab: Tab| active_tab.set(tab),
            }

            // Main column: sticky header (sync chip) + scrollable content.
            // Bottom padding only applies on mobile so the bottom nav doesn't
            // overlap the last item.
            main { class: "flex-1 flex flex-col overflow-hidden",
                header { class: "flex items-center justify-end gap-3 px-4 md:px-6 py-3 border-b border-white/5 bg-obsidian-bg/80 backdrop-blur-sm",
                    components::sync_status::SyncStatusIndicator {}
                }
                // Mobile bottom padding = nav height (4rem) + safe-area inset
                // so the last item in any scroll view clears the gesture bar.
                // Desktop keeps its plain `pb-6` (SideNav doesn't overlap the
                // content column). The mobile-only safe-area inflation lives
                // in `input.css::.pb-mobile-nav`.
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

            // Bottom nav — mobile only
            BottomNav {
                active: *active_tab.read(),
                on_switch: move |tab: Tab| active_tab.set(tab),
            }
        }
    }
}
