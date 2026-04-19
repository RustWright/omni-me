use dioxus::prelude::*;

use crate::Tab;

/// Returns the display label and the heroicons-outline path data for each tab.
/// Keeping this as a plain function (rather than a separate struct) keeps the
/// nav components easy to scan — the tab list is the surface area, not the data.
fn tab_meta(tab: Tab) -> (&'static str, &'static str) {
    match tab {
        Tab::Journal => (
            "Journal",
            "M12 4v16m8-8H4",
        ),
        Tab::Notes => (
            "Notes",
            "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z",
        ),
        Tab::Routines => (
            "Routines",
            "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4",
        ),
        Tab::Settings => (
            "Settings",
            "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
        ),
    }
}

const ALL_TABS: &[Tab] = &[Tab::Journal, Tab::Notes, Tab::Routines, Tab::Settings];

/// Mobile bottom tab bar. Hidden at `md` breakpoint and above.
#[component]
pub fn BottomNav(active: Tab, on_switch: EventHandler<Tab>) -> Element {
    let button_class = move |tab: Tab| -> String {
        let is_active = active == tab;
        if is_active {
            "flex-1 flex flex-col items-center justify-center min-h-[48px] py-1 border-none bg-obsidian-sidebar text-obsidian-accent font-semibold text-xs rounded-lg transition-all duration-150 cursor-pointer gap-0.5".into()
        } else {
            "flex-1 flex flex-col items-center justify-center min-h-[48px] py-1 border-none bg-transparent text-obsidian-text-muted font-normal text-xs rounded-lg transition-all duration-150 cursor-pointer hover:bg-white/5 gap-0.5".into()
        }
    };

    rsx! {
        nav { class: "md:hidden flex items-center gap-1 px-2 py-1 bg-obsidian-sidebar border-t border-white/5 fixed bottom-0 left-0 right-0 z-[100]",
            for tab in ALL_TABS.iter().copied() {
                {
                    let (label, icon_path) = tab_meta(tab);
                    rsx! {
                        button {
                            key: "{label}",
                            class: "{button_class(tab)}",
                            onclick: move |_| on_switch.call(tab),
                            svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: icon_path }
                            }
                            span { "{label}" }
                        }
                    }
                }
            }
        }
    }
}

/// Desktop sidebar. Hidden below `md` breakpoint.
///
/// Rendered as a left-rail with icon + label rows. Uses the same active-state
/// color rule as the bottom nav (obsidian-sidebar fill + accent text).
#[component]
pub fn SideNav(active: Tab, on_switch: EventHandler<Tab>) -> Element {
    let row_class = move |tab: Tab| -> String {
        let is_active = active == tab;
        if is_active {
            "flex items-center gap-3 px-3 py-2 rounded-lg bg-obsidian-bg text-obsidian-accent font-semibold text-sm cursor-pointer transition-all duration-150".into()
        } else {
            "flex items-center gap-3 px-3 py-2 rounded-lg bg-transparent text-obsidian-text-muted font-medium text-sm cursor-pointer hover:bg-white/5 hover:text-obsidian-text transition-all duration-150".into()
        }
    };

    rsx! {
        aside { class: "hidden md:flex md:flex-col w-56 shrink-0 bg-obsidian-sidebar border-r border-white/5 px-3 py-5 gap-1",
            div { class: "px-3 pb-4 mb-2 border-b border-white/5",
                h1 { class: "text-lg font-bold text-obsidian-accent tracking-tight", "Omni-Me" }
                p { class: "text-[10px] uppercase tracking-[0.2em] text-obsidian-text-muted mt-1", "Personal OS" }
            }

            for tab in ALL_TABS.iter().copied() {
                {
                    let (label, icon_path) = tab_meta(tab);
                    rsx! {
                        button {
                            key: "{label}",
                            class: "{row_class(tab)}",
                            onclick: move |_| on_switch.call(tab),
                            svg { class: "w-5 h-5 shrink-0", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: icon_path }
                            }
                            span { class: "flex-1 text-left", "{label}" }
                        }
                    }
                }
            }
        }
    }
}
