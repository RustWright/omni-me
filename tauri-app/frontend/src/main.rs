mod bridge;
mod components;
mod pages;

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

    rsx! {
        div {
            style: "
                display: flex;
                flex-direction: column;
                height: 100vh;
                width: 100vw;
                margin: 0;
                padding: 0;
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
                background: #f5f5f5;
                color: #1a1a2e;
                overflow: hidden;
            ",

            // Content area — scrollable
            div {
                style: "
                    flex: 1;
                    overflow-y: auto;
                    padding: 16px;
                    padding-bottom: 64px;
                ",
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
