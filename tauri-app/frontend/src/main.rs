mod bridge;
mod components;
mod pages;
mod types;
pub mod user_date;

use chrono_tz::Tz;
use dioxus::prelude::*;

use components::nav::{BottomNav, SideNav};
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
    Settings,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut active_tab = use_signal(|| Tab::Journal);

    // Timezone: default to UTC, load from backend on mount.
    let mut tz_signal = use_signal(|| Tz::UTC);
    use_context_provider(|| tz_signal);
    use_future(move || async move {
        if let Ok(info) = bridge::invoke_get_timezone().await {
            if let Ok(tz) = info.timezone.parse::<Tz>() {
                tz_signal.set(tz);
            }
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

            // Main column. Scrollable content; bottom padding only applies on
            // mobile so the bottom nav doesn't overlap the last item.
            main { class: "flex-1 flex flex-col overflow-hidden",
                div { class: "flex-1 overflow-y-auto p-4 md:p-6 pb-16 md:pb-6",
                    match *active_tab.read() {
                        Tab::Journal => rsx! { JournalPage {} },
                        Tab::Notes => rsx! { NotesPage {} },
                        Tab::Routines => rsx! { RoutinesPage {} },
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
