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
        Tab::Finances => (
            "Finances",
            "M17 9V7a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2m2 4h10a2 2 0 002-2v-6a2 2 0 00-2-2H9a2 2 0 00-2 2v6a2 2 0 002 2zm7-5a2 2 0 11-4 0 2 2 0 014 0z",
        ),
        Tab::Settings => (
            "Settings",
            "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
        ),
    }
}

const ALL_TABS: &[Tab] = &[
    Tab::Journal,
    Tab::Notes,
    Tab::Routines,
    Tab::Finances,
    Tab::Settings,
];

/// Mobile slide-in navigation drawer (1.11). Replaces the bottom tab bar on
/// small screens: opened by the header hamburger button (and, in 1.12, a
/// left-edge swipe), it slides over the content with a tap-to-dismiss scrim.
/// Hidden at `md+`, where `SideNav` is the persistent nav.
///
/// Both the scrim and the panel are *always* rendered (their visibility is
/// class-toggled, not `if`-gated) so the open/close transition can animate —
/// a conditionally-mounted node would just pop in.
#[component]
pub fn NavDrawer(
    active: Tab,
    open: bool,
    on_switch: EventHandler<Tab>,
    on_close: EventHandler<()>,
) -> Element {
    let row_class = move |tab: Tab| -> String {
        let is_active = active == tab;
        if is_active {
            "flex items-center gap-3 px-3 py-2 rounded-lg bg-obsidian-bg text-obsidian-accent font-semibold text-sm cursor-pointer transition-all duration-150".into()
        } else {
            "flex items-center gap-3 px-3 py-2 rounded-lg bg-transparent text-obsidian-text-muted font-medium text-sm cursor-pointer hover:bg-white/5 hover:text-obsidian-text transition-all duration-150".into()
        }
    };

    // Scrim: fades in; `pointer-events-none` when closed so it never blocks the
    // content underneath. Tap anywhere on it to dismiss.
    let scrim_class = if open {
        "md:hidden fixed inset-0 z-[140] bg-black/50 transition-opacity duration-200 opacity-100"
    } else {
        "md:hidden fixed inset-0 z-[140] bg-black/50 transition-opacity duration-200 opacity-0 pointer-events-none"
    };
    // Panel: slides between fully off-screen (`-translate-x-full`) and flush.
    let panel_base = "md:hidden fixed inset-y-0 left-0 z-[150] w-64 max-w-[80vw] bg-obsidian-sidebar border-r border-white/5 px-3 py-5 flex flex-col gap-1 transition-transform duration-200 ease-out";
    let panel_class = if open {
        format!("{panel_base} translate-x-0")
    } else {
        format!("{panel_base} -translate-x-full")
    };

    rsx! {
        div {
            class: "{scrim_class}",
            "aria-hidden": "true",
            onclick: move |_| on_close.call(()),
        }
        aside {
            class: "{panel_class}",
            // Clear the status bar / gesture bar on Android via the inset vars.
            style: "padding-top: calc(1.25rem + var(--safe-area-inset-top)); padding-bottom: calc(1.25rem + var(--safe-area-inset-bottom));",
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
                            onclick: move |_| {
                                on_switch.call(tab);
                                on_close.call(());
                            },
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
