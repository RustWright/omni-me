//! Asking omni-me a question and reading the answer.
//!
//! **Nothing here waits on a model.** Asking appends an event and returns; the
//! answer arrives later, through sync, from the agent that holds the database.
//! So the screen has exactly two states — a question with an answer behind it,
//! and one without — and "without" is not an error until the user decides it is.
//!
//! That shape is why the poll below exists rather than a request that blocks.
//! It also means a question survives the app being closed: the event is durable
//! the moment it is appended, and the answer is waiting on the next launch.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::primitives::{
    Banner, BannerKind, Button, ButtonSize, ButtonVariant, PageHeader,
};
use crate::types::{AssistantMessage, AssistantThread, AssistantThreadView};

/// How often to nudge sync while a question is outstanding.
///
/// ⚠️ **Only while outstanding.** The agent polls every 3s because it is on
/// mains power beside the server; a phone doing that continuously would be
/// spending radio for nothing. Between questions this screen is silent and the
/// ordinary background pull carries answers in.
const POLL_MS: i32 = 3000;

/// When to stop reassuring and start explaining.
///
/// A cold answer measured 6.6s end to end, so ~14× that is not a slow answer —
/// it means the agent is not running, or the phone cannot reach the server.
/// Saying so beats a spinner that never resolves.
const OFFLINE_HINT_MS: i64 = 90_000;

#[derive(Clone, PartialEq)]
enum View {
    /// The list of conversations.
    Threads,
    /// One conversation, by id.
    Thread(String),
}

#[component]
pub fn AssistantPage() -> Element {
    let mut view = use_signal(|| View::Threads);

    // Hardware/gesture-back (#372): an open thread is one level deep.
    crate::use_page_back(
        move || match *view.read() {
            View::Threads => 0,
            View::Thread(_) => 1,
        },
        move || view.set(View::Threads),
    );

    let current = view.read().clone();
    match current {
        View::Threads => rsx! {
            ThreadList {
                on_open: move |id: String| view.set(View::Thread(id)),
            }
        },
        View::Thread(id) => rsx! {
            ThreadDetail {
                thread_id: id,
                on_back: move |_| view.set(View::Threads),
            }
        },
    }
}

/// The conversation list, plus the composer that starts a new one.
#[component]
fn ThreadList(on_open: EventHandler<String>) -> Element {
    let mut threads = use_signal(Vec::<AssistantThread>::new);
    let mut error_msg = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| false);

    // ⚠️ Refetched on entry and on every inbound sync, never fetched once at
    // boot. A thread answered on another device — or a whole backfill — arrives
    // in a later pull, and a fetch-once provider would never show it.
    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read(); // subscribe: re-run on inbound sync
        spawn(async move {
            match bridge::invoke_list_assistant_threads().await {
                Ok(list) => {
                    threads.set(list);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            loaded.set(true);
        });
    });

    rsx! {
        div { class: "flex flex-col h-full",
            PageHeader { title: "Assistant".to_string() }

            if let Some(e) = error_msg.read().clone() {
                Banner { kind: BannerKind::Error, "{e}" }
            }

            div { class: "flex-1 overflow-y-auto px-4",
                if *loaded.read() && threads.read().is_empty() {
                    div { class: "text-obsidian-text-muted text-sm py-8 text-center",
                        p { "Ask a question about anything you have written." }
                        p { class: "mt-2 text-xs",
                            "Answers come from the assistant running on your server, so they can take a moment — and they arrive even if you close the app."
                        }
                    }
                }
                for thread in threads.read().iter().cloned() {
                    ThreadRow {
                        key: "{thread.thread_id}",
                        thread: thread.clone(),
                        on_open: move |_| on_open.call(thread.thread_id.clone()),
                    }
                }
            }

            Composer {
                placeholder: "Ask a question…".to_string(),
                on_send: move |text: String| {
                    spawn(async move {
                        match bridge::invoke_ask_assistant(&text, None).await {
                            Ok(asked) => on_open.call(asked.thread_id),
                            Err(e) => error_msg.set(Some(e)),
                        }
                    });
                },
            }
        }
    }
}

#[component]
fn ThreadRow(thread: AssistantThread, on_open: EventHandler<()>) -> Element {
    let title = thread
        .title
        .clone()
        .unwrap_or_else(|| "Untitled conversation".to_string());
    rsx! {
        div {
            class: "px-3 py-3 border-b border-obsidian-border/10 cursor-pointer hover:bg-obsidian-border/5",
            onclick: move |_| on_open.call(()),
            div { class: "text-obsidian-text text-sm font-medium truncate", "{title}" }
            div { class: "text-obsidian-text-muted text-xs mt-1",
                "{thread.message_count} message"
                if thread.message_count != 1 { "s" }
            }
        }
    }
}

/// One conversation: the transcript, the pending state, and the follow-up box.
#[component]
fn ThreadDetail(thread_id: String, on_back: EventHandler<()>) -> Element {
    let mut view = use_signal(|| None::<AssistantThreadView>);
    let mut error_msg = use_signal(|| None::<String>);
    // Milliseconds the current question has been waiting, as this screen counts
    // it. Reset every time the pending question changes.
    let mut waited_ms = use_signal(|| 0i64);

    let id_for_load = thread_id.clone();
    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read();
        let id = id_for_load.clone();
        spawn(async move {
            match bridge::invoke_read_assistant_thread(&id).await {
                Ok(v) => {
                    view.set(Some(v));
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
        });
    });

    // While a question is outstanding, pull on a short interval. `trigger_sync`
    // pushes, pulls *and* projects, so one call covers getting the question out
    // and the answer back — there is no separate command to add.
    let id_for_poll = thread_id.clone();
    use_future(move || {
        let id = id_for_poll.clone();
        async move {
            loop {
                crate::timer::sleep_ms(POLL_MS).await;
                let pending = view.read().as_ref().and_then(|v| v.pending.clone());
                if pending.is_none() {
                    waited_ms.set(0);
                    continue;
                }
                waited_ms.with_mut(|w| *w += POLL_MS as i64);
                // A failed sync is not worth surfacing here: the next tick
                // retries, and the answer is durable on the server regardless.
                let _ = bridge::invoke_trigger_sync().await;
                if let Ok(v) = bridge::invoke_read_assistant_thread(&id).await {
                    if v.pending.is_none() {
                        waited_ms.set(0);
                    }
                    view.set(Some(v));
                }
            }
        }
    });

    let snapshot = view.read().clone();
    let pending = snapshot.as_ref().and_then(|v| v.pending.clone());
    let messages = snapshot.map(|v| v.messages).unwrap_or_default();
    let offline_hint = pending.is_some() && *waited_ms.read() >= OFFLINE_HINT_MS;
    let id_for_send = thread_id.clone();

    rsx! {
        div { class: "flex flex-col h-full",
            div { class: "flex items-center gap-2 px-3 py-2 border-b border-obsidian-border/10",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    onclick: move |_| on_back.call(()),
                    "Back"
                }
            }

            if let Some(e) = error_msg.read().clone() {
                Banner { kind: BannerKind::Error, "{e}" }
            }

            div { class: "flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-3",
                for message in messages.iter().cloned() {
                    MessageBubble { key: "{message.message_id}", message }
                }

                if pending.is_some() {
                    div { class: "text-obsidian-text-muted text-sm italic",
                        if offline_hint {
                            div { class: "not-italic",
                                Banner {
                                    kind: BannerKind::Warn,
                                    "No answer yet. The assistant may not be running, or this device cannot reach your server. Your question is saved either way — it will be answered when the assistant comes back."
                                }
                            }
                        } else {
                            "Thinking…"
                        }
                    }
                }
            }

            Composer {
                placeholder: "Ask a follow-up…".to_string(),
                on_send: move |text: String| {
                    let id = id_for_send.clone();
                    spawn(async move {
                        match bridge::invoke_ask_assistant(&text, Some(&id)).await {
                            // Re-read straight away so the question appears in the
                            // transcript without waiting for the first poll tick.
                            Ok(_) => {
                                waited_ms.set(0);
                                if let Ok(v) = bridge::invoke_read_assistant_thread(&id).await {
                                    view.set(Some(v));
                                }
                            }
                            Err(e) => error_msg.set(Some(e)),
                        }
                    });
                },
            }
        }
    }
}

#[component]
fn MessageBubble(message: AssistantMessage) -> Element {
    let is_user = message.is_user();
    let bubble = if is_user {
        "self-end max-w-[85%] bg-obsidian-accent/15 text-obsidian-text rounded-lg px-3 py-2 text-sm whitespace-pre-wrap"
    } else {
        "self-start max-w-[85%] bg-obsidian-sidebar text-obsidian-text rounded-lg px-3 py-2 text-sm whitespace-pre-wrap"
    };

    // An answer that never arrived says why, in prose. `stopped` codes are for
    // the log, not for a person deciding whether to ask again.
    let failure = message.failure_note();
    let citations = message.records_read.clone().unwrap_or_default();

    rsx! {
        div { class: "flex flex-col gap-1",
            div { class: "{bubble}",
                if let Some(note) = failure.clone() {
                    span { class: "text-obsidian-text-muted italic", "{note}" }
                } else {
                    "{message.text.clone().unwrap_or_default()}"
                }
            }

            if !is_user && !citations.is_empty() {
                div { class: "self-start flex flex-wrap gap-1 pl-1",
                    span { class: "text-obsidian-text-muted text-xs", "From:" }
                    for cite in citations.iter() {
                        // ⚠️ The event carries no title on purpose, so a renamed
                        // record still cites correctly. Until this resolves the
                        // name from the local record tables, the kind is what
                        // there is to show — an id would be worse than useless.
                        span {
                            key: "{cite.kind}-{cite.id}",
                            class: "text-obsidian-text-muted text-xs px-1.5 py-0.5 rounded bg-obsidian-border/10",
                            "{cite.title.clone().unwrap_or_else(|| cite.kind.clone())}"
                        }
                    }
                }
            }
        }
    }
}

/// The text box and its send button.
///
/// Enter sends and Shift+Enter breaks the line, which is the convention every
/// chat surface shares — but the button stays visible rather than relying on it,
/// because on a phone there is no Enter key doing that job.
#[component]
fn Composer(placeholder: String, on_send: EventHandler<String>) -> Element {
    let mut draft = use_signal(String::new);

    let mut send = move || {
        let text = draft.read().trim().to_string();
        if text.is_empty() {
            return;
        }
        draft.set(String::new());
        on_send.call(text);
    };

    rsx! {
        div { class: "flex items-end gap-2 px-3 py-2 border-t border-obsidian-border/10",
            textarea {
                class: "flex-1 px-3 py-2 bg-obsidian-sidebar border border-obsidian-border/10 rounded-md text-obsidian-text text-sm outline-none focus:border-obsidian-accent resize-none",
                rows: 2,
                placeholder: "{placeholder}",
                value: "{draft}",
                oninput: move |e| draft.set(e.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter && !e.modifiers().shift() {
                        e.prevent_default();
                        send();
                    }
                },
            }
            Button {
                variant: ButtonVariant::Primary,
                size: ButtonSize::Sm,
                onclick: move |_| send(),
                "Ask"
            }
        }
    }
}
