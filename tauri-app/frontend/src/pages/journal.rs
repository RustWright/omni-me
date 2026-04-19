use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::bridge;
use crate::components::editor::Editor;
use crate::types::JournalEntryItem;
use crate::user_date::UserDate;

/// Second-level tabs inside the Journal feature.
///
/// `Today` is the default landing view and shows today's entry (creating one
/// via the editor when missing). `Calendar` is a stub that Phase 4 will turn
/// into a month grid with per-day dots.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JournalSubTab {
    Today,
    Calendar,
}

#[component]
pub fn JournalPage() -> Element {
    let sub_tab = use_signal(|| JournalSubTab::Today);

    rsx! {
        div { class: "max-w-3xl mx-auto w-full",
            JournalSubNav { active: *sub_tab.read(), on_switch: {
                let mut sub_tab = sub_tab;
                move |tab: JournalSubTab| sub_tab.set(tab)
            } }

            match *sub_tab.read() {
                JournalSubTab::Today => rsx! { TodayView {} },
                JournalSubTab::Calendar => rsx! { CalendarStubView {} },
            }
        }
    }
}

#[component]
fn JournalSubNav(active: JournalSubTab, on_switch: EventHandler<JournalSubTab>) -> Element {
    let tab_class = move |tab: JournalSubTab| -> &'static str {
        if tab == active {
            "px-4 py-1.5 text-sm font-semibold rounded-md bg-obsidian-sidebar text-obsidian-accent transition-colors"
        } else {
            "px-4 py-1.5 text-sm font-medium rounded-md bg-transparent text-obsidian-text-muted hover:text-obsidian-text transition-colors"
        }
    };

    rsx! {
        div { class: "flex gap-1 mb-6 p-1 bg-obsidian-sidebar/40 border border-white/5 rounded-lg w-fit",
            button {
                class: "{tab_class(JournalSubTab::Today)}",
                onclick: move |_| on_switch.call(JournalSubTab::Today),
                "Today"
            }
            button {
                class: "{tab_class(JournalSubTab::Calendar)}",
                onclick: move |_| on_switch.call(JournalSubTab::Calendar),
                "Calendar"
            }
        }
    }
}

#[component]
fn CalendarStubView() -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted",
            svg { class: "w-16 h-16 mb-4 opacity-20", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1", d: "M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" }
            }
            p { class: "text-lg font-medium", "Calendar" }
            p { class: "text-sm", "Coming in Phase 4" }
        }
    }
}

/// "Today" view: one-per-day entry. If no entry exists yet, the editor starts
/// blank and creates an entry on first save. If today's entry is already closed,
/// render a closed-state banner with a reopen action.
#[component]
fn TodayView() -> Element {
    let tz_signal: Signal<Tz> = use_context();
    let today = UserDate::today(&*tz_signal.read()).to_date_string();

    let mut entry = use_signal(|| None::<JournalEntryItem>);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut processing = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut save_status = use_signal(|| None::<String>);
    let mut llm_result = use_signal(|| None::<crate::types::LlmResult>);
    let mut llm_error = use_signal(|| None::<String>);
    let mut content = use_signal(String::new);

    let today_for_load = today.clone();
    let _load = use_future(move || {
        let d = today_for_load.clone();
        async move {
            match bridge::invoke_get_journal_by_date(&d).await {
                Ok(Some(e)) => {
                    content.set(e.raw_text.clone());
                    entry.set(Some(e));
                    error_msg.set(None);
                }
                Ok(None) => {
                    content.set(String::new());
                    entry.set(None);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            loading.set(false);
        }
    });

    rsx! {
        div { class: "animate-in fade-in duration-200",
            if let Some(err) = &*error_msg.read() {
                div { class: "bg-red-900/20 text-red-400 px-3 py-2 rounded border border-red-900/50 mb-4 text-sm",
                    "{err}"
                }
            }

            // Header: date + status pills + action buttons
            div { class: "flex flex-wrap justify-between items-center gap-3 mb-6",
                div { class: "flex items-center gap-3",
                    h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent", "Today" }
                    span { class: "text-sm font-mono text-obsidian-text-muted", "{today}" }
                    {
                        if let Some(e) = entry.read().as_ref() {
                            let closed = e.closed;
                            let complete = e.complete;
                            rsx! {
                                if closed {
                                    span { class: "px-2 py-0.5 bg-obsidian-text-muted/10 text-obsidian-text-muted border border-white/10 rounded text-[10px] font-bold uppercase tracking-wider",
                                        "Closed"
                                    }
                                } else if complete {
                                    span { class: "px-2 py-0.5 bg-obsidian-accent/10 text-obsidian-accent border border-obsidian-accent/20 rounded text-[10px] font-bold uppercase tracking-wider",
                                        "Complete"
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }

                div { class: "flex items-center gap-2",
                    {
                        let is_closed = entry.read().as_ref().map(|e| e.closed).unwrap_or(false);
                        let journal_id = entry.read().as_ref().map(|e| e.journal_id.clone());
                        let have_entry = entry.read().is_some();

                        rsx! {
                            if have_entry && !is_closed {
                                button {
                                    class: "flex items-center gap-2 px-3 py-1.5 bg-purple-600/20 text-purple-400 border border-purple-600/30 font-medium rounded-md hover:bg-purple-600/30 transition-colors disabled:opacity-50",
                                    disabled: *processing.read(),
                                    onclick: {
                                        let jid = journal_id.clone();
                                        move |_| {
                                            let jid = jid.clone();
                                            processing.set(true);
                                            llm_error.set(None);
                                            spawn(async move {
                                                if let Some(id) = jid {
                                                    match bridge::invoke_process_note_llm(&id).await {
                                                        Ok(r) => llm_result.set(Some(r)),
                                                        Err(e) => llm_error.set(Some(e)),
                                                    }
                                                }
                                                processing.set(false);
                                            });
                                        }
                                    },
                                    if *processing.read() { "..." } else { "AI Analyze" }
                                }
                            }

                            if is_closed {
                                button {
                                    class: "px-3 py-1.5 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text text-sm transition-colors",
                                    onclick: {
                                        let jid = journal_id.clone();
                                        let today = today.clone();
                                        move |_| {
                                            let jid = jid.clone();
                                            let today = today.clone();
                                            spawn(async move {
                                                if let Some(id) = jid {
                                                    if bridge::invoke_reopen_journal_entry(&id).await.is_ok() {
                                                        if let Ok(Some(refreshed)) =
                                                            bridge::invoke_get_journal_by_date(&today).await
                                                        {
                                                            content.set(refreshed.raw_text.clone());
                                                            entry.set(Some(refreshed));
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    "Reopen"
                                }
                            } else if have_entry {
                                button {
                                    class: "px-3 py-1.5 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text text-sm transition-colors",
                                    onclick: {
                                        let jid = journal_id.clone();
                                        let today = today.clone();
                                        move |_| {
                                            let jid = jid.clone();
                                            let today = today.clone();
                                            spawn(async move {
                                                if let Some(id) = jid {
                                                    if bridge::invoke_close_journal_entry(&id, "manual").await.is_ok() {
                                                        if let Ok(Some(refreshed)) =
                                                            bridge::invoke_get_journal_by_date(&today).await
                                                        {
                                                            content.set(refreshed.raw_text.clone());
                                                            entry.set(Some(refreshed));
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    "Close Day"
                                }
                            }

                            button {
                                class: "px-4 py-1.5 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity disabled:opacity-50",
                                disabled: *saving.read() || is_closed,
                                onclick: {
                                    let today = today.clone();
                                    let jid = journal_id.clone();
                                    move |_| {
                                        let today = today.clone();
                                        let jid = jid.clone();
                                        saving.set(true);
                                        save_status.set(None);
                                        spawn(async move {
                                            let text = content.read().clone();
                                            let result = if let Some(id) = jid {
                                                bridge::invoke_update_journal_entry(&id, &text)
                                                    .await
                                                    .map(|_| ())
                                            } else {
                                                match bridge::invoke_create_journal_entry(&today, &text).await {
                                                    Ok(created) => {
                                                        entry.set(Some(created));
                                                        Ok(())
                                                    }
                                                    Err(e) => Err(e),
                                                }
                                            };
                                            saving.set(false);
                                            match result {
                                                Ok(()) => save_status.set(Some("Saved".into())),
                                                Err(e) => save_status.set(Some(format!("Save failed: {e}"))),
                                            }
                                        });
                                    }
                                },
                                if *saving.read() { "Saving..." } else { "Save" }
                            }
                        }
                    }
                }
            }

            if *loading.read() {
                div { class: "py-20 text-center text-obsidian-text-muted", "Loading..." }
            } else {
                {
                    let is_closed = entry.read().as_ref().map(|e| e.closed).unwrap_or(false);
                    let is_new = entry.read().is_none();
                    let initial = if is_new {
                        String::new()
                    } else {
                        entry.read().as_ref().map(|e| e.raw_text.clone()).unwrap_or_default()
                    };
                    let editor_class = if is_closed {
                        "rounded-lg border border-white/10 overflow-hidden shadow-2xl opacity-60"
                    } else {
                        "rounded-lg border border-white/10 overflow-hidden shadow-2xl"
                    };

                    rsx! {
                        div { class: "{editor_class}",
                            Editor {
                                initial_content: initial,
                                on_change: move |new_content: String| content.set(new_content),
                                read_only: is_closed,
                            }
                        }
                    }
                }
            }

            if let Some(status) = &*save_status.read() {
                div { class: "mt-4 p-3 bg-obsidian-accent/5 border border-obsidian-accent/20 rounded text-sm text-obsidian-accent animate-in zoom-in-95 duration-200",
                    "{status}"
                }
            }

            if let Some(err) = &*llm_error.read() {
                div { class: "mt-4 p-3 bg-red-900/20 text-red-400 rounded border border-red-900/50 text-sm",
                    "{err}"
                }
            }

            if let Some(result) = &*llm_result.read() {
                LlmResultsDisplay { result: result.clone() }
            }
        }
    }
}

#[component]
fn LlmResultsDisplay(result: crate::types::LlmResult) -> Element {
    rsx! {
        div { class: "mt-6 p-4 bg-obsidian-sidebar/60 border border-obsidian-accent/30 rounded-lg shadow-inner animate-in zoom-in-95 duration-200",

            h3 { class: "text-xs font-bold text-obsidian-accent uppercase tracking-widest mb-4",
                "AI Analysis"
            }

            if !result.warnings.is_empty() {
                div { class: "mb-4 p-3 bg-yellow-900/20 text-yellow-400 rounded border border-yellow-900/50 text-sm",
                    for warning in &result.warnings {
                        p { "{warning}" }
                    }
                }
            }

            if !result.tags.is_empty() {
                div { class: "mb-4",
                    span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase mb-1 block", "Tags" }
                    div { class: "flex flex-wrap gap-1.5",
                        for tag in &result.tags {
                            span { class: "px-2 py-0.5 bg-obsidian-accent/10 text-obsidian-accent border border-obsidian-accent/20 rounded text-xs",
                                "#{tag}"
                            }
                        }
                    }
                }
            }

            if !result.tasks.is_empty() {
                div { class: "mb-4",
                    span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase mb-2 block", "Derived Tasks" }
                    div { class: "space-y-1.5",
                        for task in &result.tasks {
                            div { class: "flex items-start gap-2 text-sm text-obsidian-text",
                                span { class: "mt-1 shrink-0",
                                    match task.priority.as_str() {
                                        "high" => rsx! { span { class: "text-red-500", "●" } },
                                        "medium" => rsx! { span { class: "text-yellow-500", "●" } },
                                        _ => rsx! { span { class: "text-green-500", "●" } },
                                    }
                                }
                                "{task.description}"
                            }
                        }
                    }
                }
            }

            if let Some(summary) = &result.summary {
                div { class: "mt-4 pt-4 border-t border-white/5 text-sm text-obsidian-text-muted italic leading-relaxed",
                    "{summary}"
                }
            }
        }
    }
}
