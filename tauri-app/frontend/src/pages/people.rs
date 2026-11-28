//! The People tab and the person view (rulings 119, 120; `design/0102/` § 3). Both read local
//! records with no model call, so they work with the assistant down. Read-only, except Undo and
//! "Wrong?" on how a record was built.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::primitives::{PageHeader, TextInput};
use crate::sync_refresh::use_sync_epoch;
use crate::types::{PersonDecision, PersonRow, PersonView};

#[component]
pub fn PeoplePage() -> Element {
    let sync_epoch = use_sync_epoch();
    let mut query = use_signal(String::new);
    let mut open = use_signal(|| None::<String>);
    let list = use_resource(move || {
        let _ = sync_epoch.read();
        let q = query.read().clone();
        async move { bridge::invoke_people_list(q).await }
    });

    if let Some(id) = open.read().clone() {
        return rsx! {
            div { class: "animate-in fade-in duration-300 max-w-2xl",
                Person { id, on_back: move |_| open.set(None) }
            }
        };
    }

    let (subtitle, body) = match list.read().clone() {
        None => (
            String::new(),
            rsx! { p { class: "text-sm text-obsidian-text-muted", "Reading people…" } },
        ),
        Some(Err(e)) => (
            String::new(),
            rsx! { p { class: "text-sm text-error", "Couldn't read people: {e}" } },
        ),
        Some(Ok(l)) => {
            let searching = !query.read().trim().is_empty();
            let body = if l.rows.is_empty() {
                let text = if searching {
                    "Nobody matches that name, email or fact."
                } else {
                    "Nobody yet. People appear as your journal, tasks and email mention them."
                };
                rsx! { p { class: "text-sm text-obsidian-text-muted", "{text}" } }
            } else {
                rsx! {
                    if !searching {
                        p { class: "text-xs text-obsidian-text-muted mb-2", "Most recent first" }
                    }
                    ul { class: "flex flex-col divide-y divide-obsidian-border/20",
                        for row in l.rows.iter().cloned() {
                            Row {
                                key: "{row.id}",
                                row: row.clone(),
                                on_open: move |id| open.set(Some(id)),
                            }
                        }
                    }
                }
            };
            let count = match l.total {
                1 => "1 person".to_string(),
                n => format!("{n} people"),
            };
            (count, body)
        }
    };

    rsx! {
        div { class: "animate-in fade-in duration-300 max-w-2xl",
            PageHeader { title: "People", subtitle }
            div { class: "mb-4",
                TextInput {
                    value: query.read().clone(),
                    on_input: move |v| query.set(v),
                    placeholder: "Search names, emails, facts",
                }
            }
            {body}
        }
    }
}

#[component]
fn Row(row: PersonRow, on_open: EventHandler<String>) -> Element {
    let detail: Vec<String> = row.about.iter().chain(row.last.iter()).cloned().collect();
    let detail = detail.join(" · ");
    rsx! {
        li {
            class: "flex items-center justify-between gap-3 py-2.5 cursor-pointer active:opacity-60",
            onclick: {
                let id = row.id.clone();
                move |_| on_open.call(id.clone())
            },
            div { class: "min-w-0",
                div { class: "flex items-center gap-2",
                    span { class: "text-sm text-obsidian-text truncate", "{row.name}" }
                    if row.provisional {
                        span {
                            class: "text-[10px] font-medium uppercase tracking-wide px-1.5 rounded bg-obsidian-accent/15 text-obsidian-accent",
                            title: "Met once so far; confirmed by a second mention or by you",
                            "New"
                        }
                    }
                }
                if !detail.is_empty() {
                    p { class: "text-xs text-obsidian-text-muted truncate", "{detail}" }
                }
            }
            span { class: "text-obsidian-text-muted shrink-0", "aria-hidden": "true", "›" }
        }
    }
}

#[component]
fn Person(id: String, on_back: EventHandler<()>) -> Element {
    let sync_epoch = use_sync_epoch();
    let mut reread = use_signal(|| 0u32);
    let mut built_open = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let view = use_resource({
        let id = id.clone();
        move || {
            let _ = sync_epoch.read();
            let _ = reread.read();
            let id = id.clone();
            async move { bridge::invoke_person_view(id).await }
        }
    });

    let back = rsx! {
        button {
            class: "text-sm text-obsidian-text-muted py-1.5 pr-3 -mt-1.5 mb-1.5 active:opacity-60",
            onclick: move |_| on_back.call(()),
            "‹ People"
        }
    };
    let v: PersonView = match view.read().clone() {
        None => {
            return rsx! { {back} p { class: "text-sm text-obsidian-text-muted", "Reading…" } };
        }
        Some(Err(e)) => {
            return rsx! { {back} p { class: "text-sm text-error", "Couldn't read this person: {e}" } };
        }
        Some(Ok(None)) => {
            return rsx! { {back} p { class: "text-sm text-obsidian-text-muted", "This person is no longer on record." } };
        }
        Some(Ok(Some(v))) => v,
    };

    let also: Vec<String> = v
        .also
        .iter()
        .map(|a| format!("also {a}"))
        .chain(v.addresses.iter().cloned())
        .collect();
    let also = also.join(" · ");
    let undo = move |d: PersonDecision| {
        spawn(async move {
            match bridge::invoke_person_undo(d.undo).await {
                Ok(()) => error.set(None),
                Err(e) => error.set(Some(e)),
            }
            reread += 1;
        });
    };

    rsx! {
        {back}
        h1 { class: "text-2xl font-bold text-obsidian-accent", "{v.name}" }
        if !also.is_empty() {
            p { class: "text-sm text-obsidian-text-muted mt-0.5", "{also}" }
        }
        if let Some(last) = v.last.clone() {
            p { class: "text-sm text-obsidian-text mt-2", "Last: {last}" }
        }
        if let Some(e) = error.read().clone() {
            p { class: "text-sm text-error mt-2", "Couldn't undo that: {e}" }
        }

        Section { title: "What you've confirmed" }
        if v.facts.is_empty() {
            p { class: "text-xs text-obsidian-text-muted",
                "Nothing yet. Facts are proposed in chat, with the line they came from."
            }
        } else {
            ul { class: "flex flex-col gap-2",
                for f in v.facts.iter() {
                    li { key: "{f.line}", class: "flex items-baseline justify-between gap-3",
                        span { class: "text-sm text-obsidian-text", "{f.line}" }
                        span { class: "text-[11px] text-obsidian-text-muted shrink-0 px-1.5 rounded border border-obsidian-border/30",
                            "{f.source}"
                        }
                    }
                }
            }
        }

        if let Some(appears) = v.appears.clone() {
            p { class: "text-sm text-obsidian-text-muted mt-6", "{appears}" }
        }

        if !v.decisions.is_empty() {
            button {
                class: "w-full flex justify-between items-baseline text-sm text-obsidian-text mt-4 py-2 border-t border-obsidian-border/20 active:opacity-60",
                "aria-expanded": "{built_open}",
                onclick: move |_| built_open.toggle(),
                span { "How this record was built ({v.decisions.len()})" }
                span { class: "text-obsidian-text-muted", if built_open() { "⌄" } else { "›" } }
            }
            if built_open() {
                ul { class: "flex flex-col gap-2 mt-1",
                    for d in v.decisions.iter().cloned() {
                        li {
                            key: "{d.text}{d.by}",
                            class: "flex items-baseline justify-between gap-3",
                            div { class: "min-w-0",
                                if d.guessed {
                                    span { class: "text-[10px] font-medium uppercase tracking-wide text-warn mr-1.5",
                                        "Guessed"
                                    }
                                }
                                span { class: "text-sm text-obsidian-text", "{d.text}" }
                                span { class: "text-xs text-obsidian-text-muted ml-1.5 whitespace-nowrap",
                                    "{d.by}"
                                }
                            }
                            // Padded with a matching negative margin: a phone-sized target
                            // without moving the line.
                            button {
                                class: "text-xs text-obsidian-accent shrink-0 px-2 py-1.5 -mx-2 -my-1.5 active:opacity-60",
                                onclick: {
                                    let d = d.clone();
                                    move |_| undo(d.clone())
                                },
                                if d.guessed { "Wrong?" } else { "Undo" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Section(title: String) -> Element {
    rsx! {
        div { class: "text-sm font-medium text-obsidian-text border-b border-obsidian-border/20 pb-1 mt-6 mb-2",
            "{title}"
        }
    }
}
