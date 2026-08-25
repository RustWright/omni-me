//! Shared validated date field with an in-app calendar popover.
//!
//! The native `<input type="date">` can't be used: on the Linux desktop it opens
//! the **webkit2gtk native date picker** — a GTK popup that grabs focus and never
//! hands it back to the wry window, freezing the whole app until you click away to
//! another OS window (friction 2026-07-05). So this field takes plain `YYYY-MM-DD`
//! text (validated inline) **and** offers a calendar icon that opens an in-app
//! month grid (pure HTML/Dioxus — no native control, no freeze) to tap a day.
//! Typing full dates every time was "torture" on mobile; the popover fixes that
//! while keeping the field typeable. (batch-2 #3, user chose the calendar popover.)

use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::components::icon::{Icon, IconName};
use crate::components::month_grid::{build_month_cells, next_month, prev_month};
use crate::components::primitives::INPUT_CLASS;
use crate::user_date::UserDate;

/// A `YYYY-MM-DD` string is acceptable if it's empty (a cleared/optional field)
/// or it parses as a **real** calendar date — so impossible dates (`2026-13-40`,
/// `2026-02-30`) and malformed input are rejected, not just wrong-length ones.
/// Pure + unit-testable.
pub fn is_valid_date_str(s: &str) -> bool {
    let s = s.trim();
    s.is_empty() || chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// First day of the month `d` falls in.
fn first_of_month(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap()
}

/// Styling for one day button in the popover grid.
fn picker_day_class(is_today: bool, is_selected: bool, in_month: bool) -> String {
    let base = "flex items-center justify-center h-8 text-xs rounded-md transition-colors";
    let state = if is_selected {
        "bg-obsidian-accent text-black font-semibold"
    } else if is_today {
        "text-obsidian-accent font-semibold ring-1 ring-obsidian-accent/40 hover:bg-white/5"
    } else if in_month {
        "text-obsidian-text hover:bg-white/5"
    } else {
        "text-obsidian-text-muted/40 hover:bg-white/5"
    };
    format!("{base} {state}")
}

/// Default styling — the canonical shared [`INPUT_CLASS`].
const DEFAULT_DATE_CLASS: &str = INPUT_CLASS;

#[component]
pub fn DateField(
    /// Controlled value — mirror the call site's current signal/draft value.
    value: String,
    /// Fires with the raw text on each keystroke AND with `YYYY-MM-DD` when a day
    /// is picked from the calendar; the call site keeps its own "store this
    /// string" logic (signal set, `Option` wrapping, etc.).
    on_input: EventHandler<String>,
    /// Class on the `<input>` — carries the call site's sizing. Defaults to the
    /// full-width form style.
    #[props(default = DEFAULT_DATE_CLASS.to_string())]
    class: String,
    /// Tighter hint text for cramped filter rows.
    #[props(default = false)]
    compact: bool,
) -> Element {
    let invalid = !is_valid_date_str(&value);
    // A red ring (not a border-colour swap) so it reads as "error" regardless of
    // the base class's own `border-*`, which Tailwind ordering could otherwise win.
    let ring = if invalid { " ring-1 ring-red-500" } else { "" };

    let tz_signal: Signal<Tz> = use_context();
    let mut open = use_signal(|| false);
    // Month shown in the popover (first-of-month). Seeded to today; re-seeded to
    // the current value's month each time the popover opens.
    let mut anchor = use_signal(|| first_of_month(UserDate::today(&tz_signal.peek()).naive_date()));

    // Parse the current value once for the selected-day highlight; compute today
    // for the today ring. Both are cheap and drive only styling.
    let selected_date = NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok();
    let today_str = UserDate::today(&tz_signal.read()).to_date_string();

    // Wrapper width mirrors the input: full forms are `w-full`; compact filter
    // fields shrink to content (so they keep sitting in their flex-col label).
    let wrapper_class = if compact {
        "relative inline-block"
    } else {
        "relative w-full"
    };
    let is_open = *open.read();
    let month_label = anchor.read().format("%B %Y").to_string();
    let cells = build_month_cells(*anchor.read());

    // Opening seeds the grid to the current value's month (else today).
    let value_for_open = value.clone();
    let open_popover = move |_| {
        let base = NaiveDate::parse_from_str(value_for_open.trim(), "%Y-%m-%d")
            .unwrap_or_else(|_| UserDate::today(&tz_signal.peek()).naive_date());
        anchor.set(first_of_month(base));
        open.set(true);
    };

    rsx! {
        div { class: "{wrapper_class}",
            div { class: "relative",
                input {
                    r#type: "text",
                    placeholder: "YYYY-MM-DD",
                    autocomplete: "off",
                    class: "{class}{ring} pr-8",
                    value: "{value}",
                    oninput: move |e| on_input.call(e.value()),
                }
                button {
                    r#type: "button",
                    class: "absolute right-1.5 top-1/2 -translate-y-1/2 p-1 text-obsidian-text-muted hover:text-obsidian-text rounded hover:bg-white/5 transition-colors",
                    "aria-label": "Open calendar",
                    onclick: open_popover,
                    Icon { name: IconName::Calendar, class: "w-4 h-4" }
                }
            }

            if invalid {
                p {
                    class: if compact { "text-[10px] text-red-400 mt-0.5" } else { "text-[11px] text-red-400 mt-1" },
                    "Use YYYY-MM-DD"
                }
            }

            if is_open {
                // Transparent full-screen catcher: any outside click closes the
                // popover (the popover itself sits above it at z-30).
                div {
                    class: "fixed inset-0 z-20",
                    onclick: move |_| open.set(false),
                }
                div { class: "absolute left-0 top-full mt-1 z-30 w-64 max-w-[calc(100vw-2rem)] p-3 bg-obsidian-sidebar border border-white/10 rounded-lg shadow-lg shadow-black/40",
                    // Month navigation.
                    div { class: "flex items-center justify-between mb-2",
                        button {
                            r#type: "button",
                            class: "p-1.5 text-obsidian-text-muted hover:text-obsidian-text rounded hover:bg-white/5 transition-colors",
                            "aria-label": "Previous month",
                            onclick: move |_| {
                                let cur = *anchor.peek();
                                anchor.set(prev_month(cur));
                            },
                            Icon { name: IconName::ChevronLeft, class: "w-4 h-4" }
                        }
                        h3 { class: "text-sm font-semibold text-obsidian-text", "{month_label}" }
                        button {
                            r#type: "button",
                            class: "p-1.5 text-obsidian-text-muted hover:text-obsidian-text rounded hover:bg-white/5 transition-colors",
                            "aria-label": "Next month",
                            onclick: move |_| {
                                let cur = *anchor.peek();
                                anchor.set(next_month(cur));
                            },
                            Icon { name: IconName::ChevronRight, class: "w-4 h-4" }
                        }
                    }
                    // Weekday header (Mon-first, single letters).
                    div { class: "grid grid-cols-7 gap-1 mb-1",
                        for (i , label) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
                            div {
                                key: "{i}",
                                class: "text-center text-[10px] font-semibold uppercase tracking-wider text-obsidian-text-muted",
                                "{label}"
                            }
                        }
                    }
                    // Day grid — tap a day to fill the field and close.
                    div { class: "grid grid-cols-7 gap-1",
                        for cell in cells {
                            {
                                let date_str = cell.date.format("%Y-%m-%d").to_string();
                                let is_today = date_str == today_str;
                                let is_selected = Some(cell.date) == selected_date;
                                let cls = picker_day_class(is_today, is_selected, cell.in_current_month);
                                let day_num = cell.date.day();
                                rsx! {
                                    button {
                                        r#type: "button",
                                        class: "{cls}",
                                        onclick: {
                                            let d = date_str.clone();
                                            move |_| {
                                                on_input.call(d.clone());
                                                open.set(false);
                                            }
                                        },
                                        "{day_num}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{first_of_month, is_valid_date_str};
    use chrono::NaiveDate;

    #[test]
    fn empty_is_allowed() {
        assert!(is_valid_date_str(""));
        assert!(is_valid_date_str("   "));
    }

    #[test]
    fn real_date_is_valid() {
        assert!(is_valid_date_str("2026-07-05"));
        assert!(is_valid_date_str("2000-01-01"));
    }

    #[test]
    fn typos_are_rejected() {
        assert!(!is_valid_date_str("2026-13-40")); // month + day out of range
        assert!(!is_valid_date_str("2026-02-30")); // Feb 30 doesn't exist
        assert!(!is_valid_date_str("2026/07/05")); // wrong separators
        assert!(!is_valid_date_str("07-05-2026")); // wrong order
        assert!(!is_valid_date_str("not-a-date"));
        assert!(!is_valid_date_str("2026-07")); // incomplete
    }

    #[test]
    fn first_of_month_snaps_to_day_one() {
        let d = NaiveDate::from_ymd_opt(2026, 8, 23).unwrap();
        assert_eq!(first_of_month(d), NaiveDate::from_ymd_opt(2026, 8, 1).unwrap());
    }
}
