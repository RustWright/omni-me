//! Shared validated date text field.
//!
//! Replaces the native `<input type="date">`, which on the Linux desktop opens
//! the **webkit2gtk native date picker** — a GTK popup that grabs focus and
//! doesn't hand it back to the wry window, freezing the whole app until you
//! click away to another OS window (friction 2026-07-05). Instead we take plain
//! `YYYY-MM-DD` text (consistent with the Settings date fields) and validate it
//! inline, so a typo — a bad format or an impossible date like `2026-13-40` — is
//! caught visually before it's saved. (user chose text-entry + validation.)

use dioxus::prelude::*;

/// A `YYYY-MM-DD` string is acceptable if it's empty (a cleared/optional field)
/// or it parses as a **real** calendar date — so impossible dates (`2026-13-40`,
/// `2026-02-30`) and malformed input are rejected, not just wrong-length ones.
/// Pure + unit-testable.
pub fn is_valid_date_str(s: &str) -> bool {
    let s = s.trim();
    s.is_empty() || chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// Default styling — matches the full-width form date inputs it replaces.
const DEFAULT_DATE_CLASS: &str = "w-full px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text outline-none focus:border-obsidian-accent";

#[component]
pub fn DateField(
    /// Controlled value — mirror the call site's current signal/draft value.
    value: String,
    /// Fires on every keystroke with the raw text; the call site keeps its own
    /// "store this string" logic (signal set, `Option` wrapping, etc.).
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
    rsx! {
        input {
            r#type: "text",
            placeholder: "YYYY-MM-DD",
            autocomplete: "off",
            class: "{class}{ring}",
            value: "{value}",
            oninput: move |e| on_input.call(e.value()),
        }
        if invalid {
            p {
                class: if compact { "text-[10px] text-red-400 mt-0.5" } else { "text-[11px] text-red-400 mt-1" },
                "Use YYYY-MM-DD"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_date_str;

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
}
