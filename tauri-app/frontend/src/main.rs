mod bridge;
mod components;
mod pages;
mod types;
pub mod user_date;

use chrono_tz::Tz;
use dioxus::prelude::*;

use components::nav::BottomNav;
use pages::journal::JournalPage;
use pages::routines::RoutinesPage;
use pages::settings::SettingsPage;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Journal,
    Routines,
    Settings,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut active_tab = use_signal(|| Tab::Journal);

    // Timezone: default to UTC, load from backend on mount
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

        // Main container using Tailwind for Obsidian-like colors and layout
        div { class: "flex flex-col h-screen w-screen m-0 p-0 font-sans bg-obsidian-bg text-obsidian-text overflow-hidden",

            // Content area — scrollable
            div { class: "flex-1 overflow-y-auto p-4 pb-16",
                match *active_tab.read() {
                    Tab::Journal => rsx! { JournalPage {} },
                    Tab::Routines => rsx! { RoutinesPage {} },
                    Tab::Settings => rsx! { SettingsPage {} },
                }
            }

            // Bottom nav — fixed at bottom
            BottomNav {
                active: *active_tab.read(),
                on_switch: move |tab: Tab| {
                    active_tab.set(tab);
                },
            }
        }
    }
}
