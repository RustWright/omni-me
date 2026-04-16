use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::bridge;
use crate::components::editor::Editor;
use crate::types::NoteListItem;
use crate::user_date::UserDate;

#[derive(Clone, PartialEq)]
enum JournalView {
    List,
    NewNote,
    EditNote(String),
    Search,
}

#[component]
pub fn JournalPage() -> Element {
    let mut view = use_signal(|| JournalView::List);
    let mut notes = use_signal(Vec::<NoteListItem>::new);
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(Vec::<NoteListItem>::new);
    let mut error_msg = use_signal(|| None::<String>);

    // Load notes on mount
    let _load = use_future(move || async move {
        match bridge::invoke_list_notes().await {
            Ok(list) => notes.set(list),
            Err(e) => error_msg.set(Some(e)),
        }
    });

    let refresh_notes = move || {
        spawn(async move {
            match bridge::invoke_list_notes().await {
                Ok(list) => {
                    notes.set(list);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
        });
    };

    rsx! {
        div { class: "max-w-3xl mx-auto w-full",

            // Error banner
            if let Some(err) = &*error_msg.read() {
                div { class: "bg-red-900/20 text-red-400 px-3 py-2 rounded border border-red-900/50 mb-4 text-sm",
                    "{err}"
                }
            }

            match &*view.read() {
                JournalView::List => rsx! {
                    NoteListView {
                        notes: notes.read().clone(),
                        on_new: move |_| view.set(JournalView::NewNote),
                        on_edit: move |id: String| view.set(JournalView::EditNote(id)),
                        on_search: move |_| view.set(JournalView::Search),
                    }
                },
                JournalView::NewNote => rsx! {
                    NoteEditorView {
                        note_id: None,
                        initial_content: String::new(),
                        on_save: move |_| {
                            view.set(JournalView::List);
                            refresh_notes();
                        },
                        on_cancel: move |_| view.set(JournalView::List),
                    }
                },
                JournalView::EditNote(id) => rsx! {
                    NoteEditorView {
                        note_id: Some(id.clone()),
                        initial_content: notes.read().iter()
                            .find(|n| n.id == *id)
                            .map(|n| n.raw_text.clone())
                            .unwrap_or_default(),
                        on_save: move |_| {
                            view.set(JournalView::List);
                            refresh_notes();
                        },
                        on_cancel: move |_| view.set(JournalView::List),
                    }
                },
                JournalView::Search => rsx! {
                    NoteSearchView {
                        query: search_query.read().clone(),
                        results: search_results.read().clone(),
                        on_query_change: move |q: String| {
                            search_query.set(q.clone());
                            if q.trim().is_empty() {
                                search_results.set(vec![]);
                            } else {
                                spawn(async move {
                                    if let Ok(results) = bridge::invoke_search_notes(&q).await {
                                        search_results.set(results);
                                    }
                                });
                            }
                        },
                        on_select: move |id: String| view.set(JournalView::EditNote(id)),
                        on_back: move |_| view.set(JournalView::List),
                    }
                },
            }
        }
    }
}

// --- Note List View ---

#[component]
fn NoteListView(
    notes: Vec<NoteListItem>,
    on_new: EventHandler<()>,
    on_edit: EventHandler<String>,
    on_search: EventHandler<()>,
) -> Element {
    let tz_signal: Signal<Tz> = use_context();
    rsx! {
        // Header
        div { class: "flex justify-between items-center mb-6",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent", "Journal" }
            div { class: "flex gap-2",
                button {
                    class: "p-2 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text-muted transition-colors",
                    onclick: move |_| on_search.call(()),
                    // Search Icon placeholder or text
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" }
                    }
                }
                button {
                    class: "flex items-center gap-2 px-4 py-2 bg-obsidian-accent text-white font-semibold rounded-md hover:opacity-90 transition-opacity shadow-lg shadow-obsidian-accent/20",
                    onclick: move |_| on_new.call(()),
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M12 4v16m8-8H4" }
                    }
                    span { "New Note" }
                }
            }
        }

        if notes.is_empty() {
            div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted",
                svg { class: "w-16 h-16 mb-4 opacity-20", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1", d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" }
                }
                p { class: "text-lg font-medium", "No notes yet" }
                p { class: "text-sm", "Tap \"New Note\" to get started" }
            }
        } else {
            // Group notes by date
            {render_grouped_notes(&notes, on_edit, &*tz_signal.read())}
        }
    }
}

fn render_grouped_notes(notes: &[NoteListItem], on_edit: EventHandler<String>, tz: &Tz) -> Element {
    let today = UserDate::today(tz).to_date_string();
    let yesterday = UserDate::yesterday(tz).to_date_string();

    let mut today_notes = vec![];
    let mut yesterday_notes = vec![];
    let mut older_notes = vec![];

    for note in notes {
        if note.date == today {
            today_notes.push(note.clone());
        } else if note.date == yesterday {
            yesterday_notes.push(note.clone());
        } else {
            older_notes.push(note.clone());
        }
    }

    rsx! {
        if !today_notes.is_empty() {
            NoteGroup { label: "Today".to_string(), notes: today_notes, on_edit }
        }
        if !yesterday_notes.is_empty() {
            NoteGroup { label: "Yesterday".to_string(), notes: yesterday_notes, on_edit }
        }
        if !older_notes.is_empty() {
            NoteGroup { label: "Older".to_string(), notes: older_notes, on_edit }
        }
    }
}

#[component]
fn NoteGroup(label: String, notes: Vec<NoteListItem>, on_edit: EventHandler<String>) -> Element {
    rsx! {
        div { class: "mb-8",
            h3 { class: "text-[11px] font-bold text-obsidian-text-muted uppercase tracking-[0.1em] mb-3 ml-1",
                "{label}"
            }
            div { class: "space-y-1",
                for note in &notes {
                    NoteCard { note: note.clone(), on_click: on_edit }
                }
            }
        }
    }
}

#[component]
fn NoteCard(note: NoteListItem, on_click: EventHandler<String>) -> Element {
    let preview: String = note.raw_text.chars().take(80).collect::<String>()
        + if note.raw_text.len() > 80 { "..." } else { "" };

    let id = note.id.clone();

    rsx! {
        div {
            class: "group p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-lg cursor-pointer transition-all hover:bg-white/5 hover:border-white/10 active:scale-[0.98]",
            onclick: move |_| on_click.call(id.clone()),

            div { class: "text-[15px] leading-relaxed text-obsidian-text group-hover:text-white transition-colors mb-2",
                "{preview}"
            }
            div { class: "flex items-center gap-3 text-[11px] text-obsidian-text-muted",
                span { class: "flex items-center gap-1",
                    svg { class: "w-3 h-3", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" }
                    }
                    "{note.date}"
                }
                if !note.tags.is_empty() {
                    span { class: "px-1.5 py-0.5 bg-obsidian-accent/10 text-obsidian-accent rounded border border-obsidian-accent/20",
                        "{note.tags.len()} tags"
                    }
                }
            }
        }
    }
}

// --- Note Editor View ---

#[component]
fn NoteEditorView(
    note_id: Option<String>,
    initial_content: String,
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut saving = use_signal(|| false);
    let mut processing = use_signal(|| false);
    let mut content = use_signal(|| initial_content.clone());
    let mut save_error = use_signal(|| None::<String>);
    let mut llm_result = use_signal(|| None::<crate::types::LlmResult>);
    let mut llm_error = use_signal(|| None::<String>);
    let tz_signal: Signal<Tz> = use_context();
    let is_new = note_id.is_none();
    let note_id_for_save = note_id.clone();
    let note_id_for_llm = note_id.clone();

    rsx! {
        div { class: "animate-in fade-in slide-in-from-bottom-4 duration-300",
            // Header
            div { class: "flex justify-between items-center mb-6",
                button {
                    class: "p-2 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text transition-colors",
                    onclick: move |_| on_cancel.call(()),
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M10 19l-7-7m0 0l7-7m-7 7h18" }
                    }
                }
                h2 { class: "text-lg font-bold text-obsidian-text",
                    if is_new { "New Note" } else { "Edit Note" }
                }
                div { class: "flex gap-2",
                    // Process with AI button (only for saved notes)
                    if !is_new {
                        button {
                            class: "flex items-center gap-2 px-3 py-1.5 bg-purple-600/20 text-purple-400 border border-purple-600/30 font-medium rounded-md hover:bg-purple-600/30 transition-colors disabled:opacity-50",
                            disabled: *processing.read(),
                            onclick: {
                                let nid = note_id_for_llm.clone();
                                move |_| {
                                    let nid = nid.clone();
                                    processing.set(true);
                                    llm_error.set(None);
                                    spawn(async move {
                                        if let Some(id) = nid {
                                            match bridge::invoke_process_note_llm(&id).await {
                                                Ok(result) => llm_result.set(Some(result)),
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
                    button {
                        class: "px-4 py-1.5 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity disabled:opacity-50",
                        disabled: *saving.read(),
                        onclick: {
                            let note_id = note_id_for_save.clone();
                            move |_| {
                                let note_id = note_id.clone();
                                saving.set(true);
                                spawn(async move {
                                    let text = content.read().clone();
                                    let result = if let Some(id) = note_id {
                                        bridge::invoke_update_note(&id, &text).await
                                    } else {
                                        let tz = *tz_signal.read();
                                        let today = UserDate::today(&tz).to_date_string();
                                        bridge::invoke_create_note(&text, &today).await.map(|_| ())
                                    };
                                    saving.set(false);
                                    match result {
                                        Ok(_) => on_save.call(()),
                                        Err(e) => save_error.set(Some(e)),
                                    }
                                });
                            }
                        },
                        if *saving.read() { "Saving..." } else { "Save" }
                    }
                }
            }

            // Editor
            div { class: "rounded-lg border border-white/10 overflow-hidden shadow-2xl",
                Editor {
                    initial_content: initial_content.clone(),
                    on_change: move |new_content: String| {
                        content.set(new_content);
                    },
                }
            }

            // Save error
            if let Some(err) = &*save_error.read() {
                div { class: "mt-4 p-3 bg-red-900/20 text-red-400 rounded border border-red-900/50 text-sm",
                    "Save failed: {err}"
                }
            }

            // LLM error
            if let Some(err) = &*llm_error.read() {
                div { class: "mt-4 p-3 bg-red-900/20 text-red-400 rounded border border-red-900/50 text-sm",
                    "{err}"
                }
            }

            // LLM results
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

            // Warnings (e.g., sync failed after LLM processing)
            if !result.warnings.is_empty() {
                div { class: "mb-4 p-3 bg-yellow-900/20 text-yellow-400 rounded border border-yellow-900/50 text-sm",
                    for warning in &result.warnings {
                        p { "{warning}" }
                    }
                }
            }

            // Tags
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

            // Tasks
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

            // Dates & Expenses would go here with similar styling...

            // Summary
            if let Some(summary) = &result.summary {
                div { class: "mt-4 pt-4 border-t border-white/5 text-sm text-obsidian-text-muted italic leading-relaxed",
                    "{summary}"
                }
            }
        }
    }
}

// --- Search View ---

#[component]
fn NoteSearchView(
    query: String,
    results: Vec<NoteListItem>,
    on_query_change: EventHandler<String>,
    on_select: EventHandler<String>,
    on_back: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "animate-in fade-in duration-200",
            // Header
            div { class: "flex items-center gap-3 mb-6",
                button {
                    class: "p-2 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text transition-colors",
                    onclick: move |_| on_back.call(()),
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M10 19l-7-7m0 0l7-7m-7 7h18" }
                    }
                }
                div { class: "flex-1 relative",
                    svg { class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-obsidian-text-muted", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" }
                    }
                    input {
                        class: "w-full pl-10 pr-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent transition-colors",
                        r#type: "text",
                        placeholder: "Search your journal...",
                        value: "{query}",
                        autofocus: true,
                        oninput: move |e| on_query_change.call(e.value()),
                    }
                }
            }

            if query.trim().is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted opacity-40",
                    svg { class: "w-16 h-16 mb-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1", d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" }
                    }
                    p { "Type to explore your thoughts" }
                }
            } else if results.is_empty() {
                div { class: "text-center py-20 text-obsidian-text-muted",
                    "No matching notes found."
                }
            } else {
                div { class: "space-y-1",
                    for note in &results {
                        NoteCard { note: note.clone(), on_click: on_select }
                    }
                }
            }
        }
    }
}
