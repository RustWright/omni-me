use dioxus::prelude::*;

use crate::bridge;
use crate::components::editor::Editor;
use crate::types::NoteListItem;

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
        div {
            style: "max-width: 720px; margin: 0 auto;",

            // Error banner
            if let Some(err) = &*error_msg.read() {
                div {
                    style: "background: #fee; color: #c33; padding: 8px 12px; border-radius: 6px; margin-bottom: 12px; font-size: 14px;",
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
    rsx! {
        // Header
        div {
            style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
            h1 {
                style: "font-size: 24px; font-weight: 600; margin: 0; color: #1a1a2e;",
                "Journal"
            }
            div {
                style: "display: flex; gap: 8px;",
                button {
                    style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_search.call(()),
                    "Search"
                }
                button {
                    style: "padding: 8px 16px; background: #4a6fa5; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_new.call(()),
                    "+ New Note"
                }
            }
        }

        if notes.is_empty() {
            div {
                style: "text-align: center; padding: 48px 16px; color: #888;",
                p { style: "font-size: 18px; margin-bottom: 8px;", "No notes yet" }
                p { style: "font-size: 14px;", "Tap \"+ New Note\" to get started" }
            }
        } else {
            // Group notes by date
            {render_grouped_notes(&notes, on_edit)}
        }
    }
}

fn render_grouped_notes(notes: &[NoteListItem], on_edit: EventHandler<String>) -> Element {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

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
        div {
            style: "margin-bottom: 20px;",
            h3 {
                style: "font-size: 13px; font-weight: 600; color: #888; text-transform: uppercase; letter-spacing: 0.5px; margin: 0 0 8px 0;",
                "{label}"
            }
            for note in &notes {
                NoteCard { note: note.clone(), on_click: on_edit }
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
            style: "padding: 12px; border-bottom: 1px solid #eee; cursor: pointer; transition: background 0.15s;",
            onclick: move |_| on_click.call(id.clone()),

            div {
                style: "font-size: 15px; color: #1a1a2e; margin-bottom: 4px; line-height: 1.4;",
                "{preview}"
            }
            div {
                style: "display: flex; gap: 8px; font-size: 12px; color: #888;",
                span { "{note.date}" }
                if !note.tags.is_empty() {
                    span {
                        style: "background: #e8eef4; padding: 1px 6px; border-radius: 4px; color: #4a6fa5;",
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
    let is_new = note_id.is_none();
    let note_id_for_save = note_id.clone();
    let note_id_for_llm = note_id.clone();

    rsx! {
        div {
            // Header
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;",
                button {
                    style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_cancel.call(()),
                    "Back"
                }
                h2 {
                    style: "font-size: 18px; font-weight: 600; margin: 0;",
                    if is_new { "New Note" } else { "Edit Note" }
                }
                div {
                    style: "display: flex; gap: 8px;",
                    // Process with AI button (only for saved notes)
                    if !is_new {
                        button {
                            style: "padding: 8px 12px; background: #6b5b95; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 14px;",
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
                            if *processing.read() { "Processing..." } else { "Process with AI" }
                        }
                    }
                    button {
                        style: "padding: 8px 16px; background: #4a6fa5; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 14px;",
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
                                        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
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
            Editor {
                initial_content: initial_content.clone(),
                on_change: move |new_content: String| {
                    content.set(new_content);
                },
            }

            // Save error
            if let Some(err) = &*save_error.read() {
                div {
                    style: "margin-top: 12px; padding: 8px 12px; background: #fee; color: #c33; border-radius: 6px; font-size: 14px;",
                    "Save failed: {err}"
                }
            }

            // LLM error
            if let Some(err) = &*llm_error.read() {
                div {
                    style: "margin-top: 12px; padding: 8px 12px; background: #fee; color: #c33; border-radius: 6px; font-size: 14px;",
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
        div {
            style: "margin-top: 16px; padding: 12px; background: #f8f8fc; border: 1px solid #e0e0e8; border-radius: 8px;",

            h3 {
                style: "font-size: 14px; font-weight: 600; color: #6b5b95; margin: 0 0 12px 0;",
                "AI Analysis"
            }

            // Tags
            if !result.tags.is_empty() {
                div {
                    style: "margin-bottom: 10px;",
                    span { style: "font-size: 12px; font-weight: 600; color: #888;", "Tags: " }
                    for tag in &result.tags {
                        span {
                            style: "display: inline-block; background: #e8eef4; color: #4a6fa5; padding: 2px 8px; border-radius: 4px; font-size: 13px; margin: 2px 4px 2px 0;",
                            "{tag}"
                        }
                    }
                }
            }

            // Tasks
            if !result.tasks.is_empty() {
                div {
                    style: "margin-bottom: 10px;",
                    span { style: "font-size: 12px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Tasks:" }
                    for task in &result.tasks {
                        div {
                            style: "font-size: 13px; padding: 2px 0; padding-left: 12px;",
                            span {
                                style: "color: #888; margin-right: 6px;",
                                match task.priority.as_str() {
                                    "high" => "[!]",
                                    "medium" => "[-]",
                                    _ => "[ ]",
                                }
                            }
                            "{task.description}"
                        }
                    }
                }
            }

            // Dates
            if !result.dates.is_empty() {
                div {
                    style: "margin-bottom: 10px;",
                    span { style: "font-size: 12px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Dates:" }
                    for date in &result.dates {
                        div {
                            style: "font-size: 13px; padding: 2px 0; padding-left: 12px;",
                            span { style: "font-weight: 500;", "{date.date}" }
                            span { style: "color: #888; margin-left: 6px;", "— {date.context}" }
                        }
                    }
                }
            }

            // Expenses
            if !result.expenses.is_empty() {
                div {
                    span { style: "font-size: 12px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Expenses:" }
                    for expense in &result.expenses {
                        div {
                            style: "font-size: 13px; padding: 2px 0; padding-left: 12px;",
                            span { style: "font-weight: 500;", "{expense.currency} {expense.amount:.2}" }
                            span { style: "color: #888; margin-left: 6px;", "— {expense.description}" }
                        }
                    }
                }
            }

            // Summary
            if let Some(summary) = &result.summary {
                div {
                    style: "margin-top: 10px; font-size: 13px; color: #555; font-style: italic; border-top: 1px solid #e0e0e8; padding-top: 8px;",
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
        div {
            // Header
            div {
                style: "display: flex; align-items: center; gap: 8px; margin-bottom: 16px;",
                button {
                    style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_back.call(()),
                    "Back"
                }
                input {
                    style: "flex: 1; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; font-size: 15px; outline: none;",
                    r#type: "text",
                    placeholder: "Search notes...",
                    value: "{query}",
                    autofocus: true,
                    oninput: move |e| on_query_change.call(e.value()),
                }
            }

            if query.trim().is_empty() {
                div {
                    style: "text-align: center; padding: 32px 16px; color: #888; font-size: 14px;",
                    "Type to search notes"
                }
            } else if results.is_empty() {
                div {
                    style: "text-align: center; padding: 32px 16px; color: #888; font-size: 14px;",
                    "No results found"
                }
            } else {
                for note in &results {
                    NoteCard { note: note.clone(), on_click: on_select }
                }
            }
        }
    }
}
