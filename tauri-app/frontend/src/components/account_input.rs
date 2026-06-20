//! Shared account-name typeahead (friction-log [M]).
//!
//! Every place the user types a ledger account name (`Assets:Wise:CAD`,
//! `Expenses:Food`) used to be a raw `<input>` with no autocomplete. This is the
//! single component those sites swap to: a controlled text input + a suggestion
//! dropdown fed by the 3.9 `known_accounts` union (every account seen in the
//! ledger ∪ declared ∪ each name's ancestor segments — so segment-aware
//! completion falls out for free).
//!
//! The component exposes the same `value` + `on_input` shape as a plain input, so
//! each call site keeps its existing read-modify-write save logic unchanged.

use dioxus::prelude::*;

use crate::bridge;

/// Most suggestions to show in the dropdown at once.
const MAX_SUGGESTIONS: usize = 8;

/// Default input styling (matches the standard form-input class used across the
/// app). Call sites with their own width/size needs override via `input_class`.
const DEFAULT_INPUT_CLASS: &str = "w-full px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-sm outline-none focus:border-obsidian-accent";

/// Root-provided known-account list, shared by every `AccountInput`.
///
/// Fetched once (one network round-trip, not one-per-posting-row) and held at the
/// app root like the continuity / tz / pending-share stores. `Copy` because
/// `Signal` is `Copy` — handlers can hold it freely.
#[derive(Clone, Copy)]
pub struct AccountSuggestions {
    list: Signal<Vec<String>>,
}

impl AccountSuggestions {
    /// Re-pull the list from the backend. Call after an account-creating save so a
    /// just-used account autocompletes next time. Advisory — fire-and-forget.
    pub fn refresh(self) {
        let mut list = self.list;
        spawn(async move {
            if let Ok(accounts) = bridge::invoke_list_known_accounts().await {
                list.set(accounts);
            }
        });
    }
}

/// Install the suggestion context at app root + kick off the initial fetch.
/// Mirrors `continuity::use_continuity_provider`; call once in `App`.
pub fn use_account_suggestions_provider() {
    let mut list = use_signal(Vec::<String>::new);
    use_context_provider(|| AccountSuggestions { list });
    use_future(move || async move {
        if let Ok(accounts) = bridge::invoke_list_known_accounts().await {
            list.set(accounts);
        }
    });
}

/// Whether the typed value being an *unknown* account is normal (an add/edit
/// site, where a new account is allowed) or a problem (a query/filter site, where
/// an unknown account means the result set will be empty for the wrong reason).
/// Drives only the affordance copy — the data source is the same union either way.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum AccountMode {
    /// Creating/editing — an unknown account is fine ("will be created").
    #[default]
    Add,
    /// Querying/filtering — an unknown account is flagged ("no such account").
    Query,
}

/// Filter the known-account set against what the user has typed and return the
/// best handful to show in the dropdown (most relevant first), capped at
/// `MAX_SUGGESTIONS`. Matching should be forgiving of case.
fn rank_suggestions(all: &[String], query: &str) -> Vec<String> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    all.iter()
        .filter(|m| m.to_lowercase().starts_with(query.as_str()))
        .take(MAX_SUGGESTIONS)
        .cloned()
        .collect()
}

#[component]
pub fn AccountInput(
    /// Controlled value — mirror the call site's current signal value.
    value: String,
    /// Fires on every keystroke AND on suggestion-select. The call site reuses
    /// its existing "update my signal" closure here unchanged.
    on_input: EventHandler<String>,
    #[props(default)] mode: AccountMode,
    #[props(default = false)] disabled: bool,
    #[props(default = String::new())] placeholder: String,
    /// Class on the outer wrapper — carries how the component *sizes itself*
    /// within its parent (e.g. `flex-1 min-w-[200px]` inside a flex row).
    /// `relative` is always added (the dropdown is absolutely positioned).
    #[props(default = "w-full".to_string())]
    wrapper_class: String,
    /// Class on the `<input>` itself — its visual styling. Should keep `w-full`
    /// so it fills the wrapper.
    #[props(default = DEFAULT_INPUT_CLASS.to_string())]
    input_class: String,
) -> Element {
    let suggestions_ctx = use_context::<AccountSuggestions>();
    // Dropdown visibility + which row is keyboard-highlighted. Per-instance, so a
    // multi-row form keeps each row's dropdown independent.
    let mut open = use_signal(|| false);
    let mut highlighted = use_signal(|| 0usize);

    // Recompute on every render: `value` is a prop (changes each keystroke as the
    // parent re-renders), not a signal a memo could subscribe to. Reading the
    // context signal here subscribes us to list refreshes.
    let suggestions = rank_suggestions(&suggestions_ctx.list.read(), &value);
    let trimmed = value.trim();
    // Case-insensitive to match the dropdown's filter (`rank_suggestions`), so a
    // differently-cased-but-real account doesn't flash the "new account" hint.
    let is_known = {
        let q = trimmed.to_lowercase();
        !q.is_empty() && suggestions_ctx.list.read().iter().any(|a| a.to_lowercase() == q)
    };

    let show_unknown = !disabled && !trimmed.is_empty() && !is_known;
    let unknown_msg = match mode {
        AccountMode::Add => "New account — will be created",
        AccountMode::Query => "No such account in the ledger",
    };
    let unknown_class = match mode {
        AccountMode::Add => "mt-1 text-[11px] text-obsidian-text-muted",
        AccountMode::Query => "mt-1 text-[11px] text-amber-400",
    };

    let dropdown_open = !disabled && *open.read() && !suggestions.is_empty();
    let hl = *highlighted.read();
    let suggestions_for_keys = suggestions.clone();

    rsx! {
        div { class: "relative {wrapper_class}",
            input {
                class: "{input_class}",
                r#type: "text",
                placeholder: "{placeholder}",
                value: "{value}",
                autocomplete: "off",
                disabled,
                oninput: move |e| {
                    on_input.call(e.value());
                    open.set(true);
                    highlighted.set(0);
                },
                onfocus: move |_| open.set(true),
                onblur: move |_| open.set(false),
                onkeydown: move |e| {
                    let sugg = &suggestions_for_keys;
                    match e.key() {
                        Key::ArrowDown => {
                            e.prevent_default();
                            if !sugg.is_empty() {
                                open.set(true);
                                let cur = *highlighted.peek();
                                highlighted.set((cur + 1).min(sugg.len() - 1));
                            }
                        }
                        Key::ArrowUp => {
                            e.prevent_default();
                            let cur = *highlighted.peek();
                            highlighted.set(cur.saturating_sub(1));
                        }
                        Key::Enter => {
                            let cur = *highlighted.peek();
                            if *open.peek() && let Some(s) = sugg.get(cur) {
                                e.prevent_default();
                                on_input.call(s.clone());
                                open.set(false);
                            }
                        }
                        Key::Escape => open.set(false),
                        _ => {}
                    }
                },
            }

            if dropdown_open {
                ul { class: "absolute z-30 mt-1 w-full max-h-60 overflow-y-auto bg-obsidian-sidebar border border-white/10 rounded-md shadow-lg shadow-black/40",
                    for (i, s) in suggestions.iter().enumerate() {
                        li {
                            key: "{s}",
                            class: if i == hl {
                                "px-3 py-1.5 text-sm cursor-pointer bg-white/10 text-obsidian-text"
                            } else {
                                "px-3 py-1.5 text-sm cursor-pointer text-obsidian-text hover:bg-white/5"
                            },
                            // Select on mousedown (fires before the input's blur),
                            // and prevent_default so focus stays on the input.
                            onmousedown: {
                                let s = s.clone();
                                move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    on_input.call(s.clone());
                                    open.set(false);
                                }
                            },
                            "{s}"
                        }
                    }
                }
            }

            if show_unknown {
                div { class: "{unknown_class}", "{unknown_msg}" }
            }
        }
    }
}
