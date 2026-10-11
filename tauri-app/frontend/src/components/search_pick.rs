//! Pick one value out of a long list by typing part of it.
//!
//! A `<select>` was fine at ten tags and unusable at a hundred (his report,
//! 2026-10-11). Only a listed value can be picked, so a typo never becomes an
//! exact-match filter that returns nothing.

use dioxus::prelude::*;

const MAX_MATCHES: usize = 8;

/// Matches for `query` in `options` (value, label), by substring of either,
/// ignoring case; the first `MAX_MATCHES` in list order when the query is empty.
pub fn matches(options: &[(String, String)], query: &str) -> Vec<(String, String)> {
    let q = query.trim().to_lowercase();
    options
        .iter()
        .filter(|(v, l)| {
            q.is_empty() || l.to_lowercase().contains(&q) || v.to_lowercase().contains(&q)
        })
        .take(MAX_MATCHES)
        .cloned()
        .collect()
}

#[component]
pub fn SearchPick(
    /// (value, label) pairs.
    options: Vec<(String, String)>,
    /// The picked value; empty for none.
    value: String,
    on_pick: EventHandler<String>,
    placeholder: String,
    #[props(default = String::new())] class: String,
) -> Element {
    let mut query = use_signal(String::new);
    let mut open = use_signal(|| false);

    let label = options
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, l)| l.clone())
        .unwrap_or_default();
    let found = matches(&options, &query.read());
    let shown = if *open.read() {
        query.read().clone()
    } else {
        label
    };
    let more = options.len() > found.len() && query.read().trim().is_empty();

    rsx! {
        div { class: "relative {class}",
            input {
                r#type: "text",
                autocomplete: "off",
                placeholder: "{placeholder}",
                value: "{shown}",
                class: "w-full px-3 py-2 pr-8 text-sm rounded-lg bg-obsidian-sidebar/60 \
                        border border-obsidian-border/10 text-obsidian-text \
                        placeholder:text-obsidian-text-muted/60 focus:outline-none \
                        focus:border-obsidian-accent/40",
                onfocus: move |_| {
                    query.set(String::new());
                    open.set(true);
                },
                oninput: move |e| query.set(e.value()),
                onblur: move |_| open.set(false),
            }
            if !value.is_empty() {
                button {
                    class: "absolute right-1 top-1/2 -translate-y-1/2 px-2 py-1 text-sm \
                            text-obsidian-text-muted",
                    "aria-label": "Clear",
                    onclick: move |_| on_pick.call(String::new()),
                    "✕"
                }
            }
            if *open.read() {
                ul { class: "absolute z-30 mt-1 w-full max-h-72 overflow-y-auto bg-obsidian-sidebar \
                             border border-obsidian-border/10 rounded-md shadow-lg shadow-black/40",
                    if found.is_empty() {
                        li { class: "px-3 py-2 text-sm text-obsidian-text-muted", "No match" }
                    }
                    for (v, l) in found.into_iter() {
                        li {
                            key: "{v}",
                            class: "px-3 py-2.5 text-sm cursor-pointer text-obsidian-text \
                                    hover:bg-obsidian-border/5",
                            // Mousedown, before the input's blur closes the list.
                            onmousedown: move |e: Event<MouseData>| {
                                e.prevent_default();
                                on_pick.call(v.clone());
                                open.set(false);
                            },
                            "{l}"
                        }
                    }
                    if more {
                        li { class: "px-3 py-2 text-[11px] text-obsidian-text-muted",
                            "Type to find the rest of the {options.len()}."
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(xs: &[&str]) -> Vec<(String, String)> {
        xs.iter().map(|x| (x.to_string(), x.to_string())).collect()
    }

    #[test]
    fn a_query_matches_anywhere_in_the_name_ignoring_case() {
        let o = opts(&["receipt", "promo", "login-code", "Receipts-2024"]);
        let got: Vec<String> = matches(&o, "RECEIPT").into_iter().map(|(v, _)| v).collect();
        assert_eq!(got, ["receipt", "Receipts-2024"]);
    }

    #[test]
    fn an_empty_query_shows_the_first_few_only() {
        let o = opts(&["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]);
        assert_eq!(matches(&o, "").len(), MAX_MATCHES);
    }
}
