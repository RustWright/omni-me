use dioxus::prelude::*;
use crate::Tab;

#[component]
pub fn BottomNav(active: Tab, on_switch: EventHandler<Tab>) -> Element {
    let get_tab_class = move |tab: Tab| -> String {
        let is_active = active == tab;
        if is_active {
            "flex-1 flex items-center justify-center min-h-[48px] py-2 border-none bg-obsidian-sidebar text-obsidian-accent font-semibold text-sm rounded-lg transition-all duration-150 cursor-pointer".to_string()
        } else {
            "flex-1 flex items-center justify-center min-h-[48px] py-2 border-none bg-transparent text-obsidian-text-muted font-normal text-sm rounded-lg transition-all duration-150 cursor-pointer hover:bg-white/5".to_string()
        }
    };

    rsx! {
        nav { class: "flex items-center gap-1 px-2 py-1 bg-obsidian-sidebar border-t border-white/5 fixed bottom-0 left-0 right-0 z-[100]",
            button {
                class: "{get_tab_class(Tab::Journal)}",
                onclick: move |_| on_switch.call(Tab::Journal),
                "Journal"
            }

            button {
                class: "{get_tab_class(Tab::Routines)}",
                onclick: move |_| on_switch.call(Tab::Routines),
                "Routines"
            }

            button {
                class: "{get_tab_class(Tab::Settings)}",
                onclick: move |_| on_switch.call(Tab::Settings),
                "Settings"
            }
        }
    }
}
