use dioxus::prelude::*;

use crate::bridge;
use crate::components::editor::Editor;
use crate::types::GenericNoteItem;

/// Second-level tabs inside the Notes feature.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotesSubTab {
    Recent,
    Search,
}

#[derive(Clone, PartialEq)]
enum NotesView {
    List,
    Edit(String),
    New,
}

#[component]
pub fn NotesPage() -> Element {
    let sub_tab = use_signal(|| NotesSubTab::Recent);
    let view = use_signal(|| NotesView::List);

    rsx! {
        div { class: "max-w-3xl mx-auto w-full",
            {
                let current_view = view.read().clone();

                rsx! {
                    // Sub-tabs + back/action header. The sub-nav only renders
                    // when we're on the list-level view; editing a note takes
                    // full width.
                    if matches!(current_view, NotesView::List) {
                        NotesSubNav { active: *sub_tab.read(), on_switch: {
                            let mut sub_tab = sub_tab;
                            move |tab: NotesSubTab| sub_tab.set(tab)
                        } }
                    }

                    match current_view {
                        NotesView::List => rsx! {
                            NotesListRouter {
                                sub_tab: *sub_tab.read(),
                                on_edit: {
                                    let mut view = view;
                                    move |id: String| view.set(NotesView::Edit(id))
                                },
                                on_new: {
                                    let mut view = view;
                                    move |_| view.set(NotesView::New)
                                },
                            }
                        },
                        NotesView::Edit(id) => rsx! {
                            NoteEditor {
                                note_id: Some(id),
                                on_back: {
                                    let mut view = view;
                                    move |_| view.set(NotesView::List)
                                },
                            }
                        },
                        NotesView::New => rsx! {
                            NoteEditor {
                                note_id: None,
                                on_back: {
                                    let mut view = view;
                                    move |_| view.set(NotesView::List)
                                },
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn NotesSubNav(active: NotesSubTab, on_switch: EventHandler<NotesSubTab>) -> Element {
    let tab_class = move |tab: NotesSubTab| -> &'static str {
        if tab == active {
            "px-4 py-1.5 text-sm font-semibold rounded-md bg-obsidian-sidebar text-obsidian-accent transition-colors"
        } else {
            "px-4 py-1.5 text-sm font-medium rounded-md bg-transparent text-obsidian-text-muted hover:text-obsidian-text transition-colors"
        }
    };

    rsx! {
        div { class: "flex gap-1 mb-6 p-1 bg-obsidian-sidebar/40 border border-white/5 rounded-lg w-fit",
            button {
                class: "{tab_class(NotesSubTab::Recent)}",
                onclick: move |_| on_switch.call(NotesSubTab::Recent),
                "Recent"
            }
            button {
                class: "{tab_class(NotesSubTab::Search)}",
                onclick: move |_| on_switch.call(NotesSubTab::Search),
                "Search"
            }
        }
    }
}

#[component]
fn NotesListRouter(
    sub_tab: NotesSubTab,
    on_edit: EventHandler<String>,
    on_new: EventHandler<()>,
) -> Element {
    match sub_tab {
        NotesSubTab::Recent => rsx! { RecentView { on_edit, on_new } },
        NotesSubTab::Search => rsx! { SearchView { on_select: on_edit } },
    }
}

#[component]
fn RecentView(on_edit: EventHandler<String>, on_new: EventHandler<()>) -> Element {
    let mut notes = use_signal(Vec::<GenericNoteItem>::new);
    let mut error_msg = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);

    let _load = use_future(move || async move {
        match bridge::invoke_list_generic_notes().await {
            Ok(list) => {
                notes.set(list);
                error_msg.set(None);
            }
            Err(e) => error_msg.set(Some(e)),
        }
        loading.set(false);
    });

    rsx! {
        div { class: "animate-in fade-in duration-200",
            div { class: "flex justify-between items-center mb-6",
                h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent", "Notes" }
                button {
                    class: "flex items-center gap-2 px-4 py-2 bg-obsidian-accent text-white font-semibold rounded-md hover:opacity-90 transition-opacity shadow-lg shadow-obsidian-accent/20",
                    onclick: move |_| on_new.call(()),
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M12 4v16m8-8H4" }
                    }
                    span { "New Note" }
                }
            }

            if let Some(err) = &*error_msg.read() {
                div { class: "bg-red-900/20 text-red-400 px-3 py-2 rounded border border-red-900/50 mb-4 text-sm",
                    "{err}"
                }
            }

            if *loading.read() {
                div { class: "py-20 text-center text-obsidian-text-muted", "Loading..." }
            } else if notes.read().is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted",
                    svg { class: "w-16 h-16 mb-4 opacity-20", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1", d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" }
                    }
                    p { class: "text-lg font-medium", "No notes yet" }
                    p { class: "text-sm", "Tap \"New Note\" to capture a thought" }
                }
            } else {
                div { class: "space-y-1",
                    for note in notes.read().iter() {
                        NoteCard { note: note.clone(), on_click: on_edit }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchView(on_select: EventHandler<String>) -> Element {
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<GenericNoteItem>::new);
    let mut loading = use_signal(|| false);

    rsx! {
        div { class: "animate-in fade-in duration-200",
            div { class: "flex items-center gap-3 mb-6",
                div { class: "flex-1 relative",
                    svg { class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-obsidian-text-muted", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" }
                    }
                    input {
                        class: "w-full pl-10 pr-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent transition-colors",
                        r#type: "text",
                        placeholder: "Search generic notes...",
                        value: "{query}",
                        autofocus: true,
                        oninput: move |e| {
                            let q = e.value();
                            query.set(q.clone());
                            // Empty query = empty results (per user preference:
                            // see feedback_search_empty_query memory).
                            if q.trim().is_empty() {
                                results.set(vec![]);
                                loading.set(false);
                            } else {
                                loading.set(true);
                                spawn(async move {
                                    match bridge::invoke_search_generic_notes(&q).await {
                                        Ok(list) => results.set(list),
                                        Err(_) => results.set(vec![]),
                                    }
                                    loading.set(false);
                                });
                            }
                        },
                    }
                }
            }

            if query.read().trim().is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted opacity-40",
                    svg { class: "w-16 h-16 mb-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1", d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" }
                    }
                    p { "Type to search your notes" }
                }
            } else if *loading.read() {
                div { class: "py-20 text-center text-obsidian-text-muted", "Searching..." }
            } else if results.read().is_empty() {
                div { class: "text-center py-20 text-obsidian-text-muted",
                    "No matching notes found."
                }
            } else {
                div { class: "space-y-1",
                    for note in results.read().iter() {
                        NoteCard { note: note.clone(), on_click: on_select }
                    }
                }
            }
        }
    }
}

#[component]
fn NoteCard(note: GenericNoteItem, on_click: EventHandler<String>) -> Element {
    let preview: String = note.raw_text.chars().take(80).collect::<String>()
        + if note.raw_text.len() > 80 { "..." } else { "" };
    let id = note.id.clone();

    rsx! {
        div {
            class: "group p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-lg cursor-pointer transition-all hover:bg-white/5 hover:border-white/10 active:scale-[0.98]",
            onclick: move |_| on_click.call(id.clone()),
            div { class: "font-semibold text-obsidian-text mb-1", "{note.title}" }
            div { class: "text-[13px] leading-relaxed text-obsidian-text-muted line-clamp-2 mb-2",
                "{preview}"
            }
            if !note.tags.is_empty() {
                div { class: "flex flex-wrap gap-1",
                    for tag in &note.tags {
                        span { class: "px-1.5 py-0.5 bg-obsidian-accent/10 text-obsidian-accent rounded border border-obsidian-accent/20 text-[10px]",
                            "#{tag}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NoteEditor(note_id: Option<String>, on_back: EventHandler<()>) -> Element {
    let is_new = note_id.is_none();
    let mut loading = use_signal(|| !is_new);
    let mut title = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut initial_content = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut save_status = use_signal(|| None::<String>);
    let mut fetch_error = use_signal(|| None::<String>);

    let note_id_for_load = note_id.clone();
    let _load = use_future(move || {
        let id = note_id_for_load.clone();
        async move {
            if let Some(id) = id {
                match bridge::invoke_get_generic_note(&id).await {
                    Ok(n) => {
                        title.set(n.title);
                        content.set(n.raw_text.clone());
                        initial_content.set(n.raw_text);
                    }
                    Err(e) => fetch_error.set(Some(e)),
                }
                loading.set(false);
            }
        }
    });

    rsx! {
        div { class: "animate-in fade-in slide-in-from-bottom-4 duration-300",
            div { class: "flex justify-between items-center mb-6 gap-3",
                button {
                    class: "p-2 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text transition-colors shrink-0",
                    onclick: move |_| on_back.call(()),
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M10 19l-7-7m0 0l7-7m-7 7h18" }
                    }
                }
                input {
                    class: "flex-1 px-3 py-2 bg-transparent border-b border-white/10 text-lg font-bold text-obsidian-text outline-none focus:border-obsidian-accent transition-colors",
                    r#type: "text",
                    placeholder: if is_new { "Untitled note" } else { "Title" },
                    value: "{title}",
                    oninput: move |e| title.set(e.value()),
                }
                button {
                    class: "px-4 py-1.5 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity disabled:opacity-50 shrink-0",
                    disabled: *saving.read() || title.read().trim().is_empty(),
                    onclick: {
                        let id = note_id.clone();
                        move |_| {
                            let id = id.clone();
                            saving.set(true);
                            save_status.set(None);
                            spawn(async move {
                                let t = title.read().clone();
                                let body = content.read().clone();
                                let result = if let Some(nid) = id {
                                    let update = bridge::invoke_update_generic_note(&nid, &body).await;
                                    if update.is_ok() {
                                        bridge::invoke_rename_generic_note(&nid, &t).await
                                    } else {
                                        update
                                    }
                                } else {
                                    bridge::invoke_create_generic_note(&t, &body).await.map(|_| ())
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

            if let Some(err) = &*fetch_error.read() {
                div { class: "bg-red-900/20 text-red-400 px-3 py-2 rounded border border-red-900/50 mb-4 text-sm",
                    "{err}"
                }
            }

            if *loading.read() {
                div { class: "py-20 text-center text-obsidian-text-muted", "Loading..." }
            } else {
                div { class: "rounded-lg border border-white/10 overflow-hidden shadow-2xl",
                    Editor {
                        initial_content: initial_content.read().clone(),
                        on_change: move |new_content: String| content.set(new_content),
                    }
                }
            }

            if let Some(status) = &*save_status.read() {
                div { class: "mt-4 p-3 bg-obsidian-accent/5 border border-obsidian-accent/20 rounded text-sm text-obsidian-accent animate-in zoom-in-95 duration-200",
                    "{status}"
                }
            }
        }
    }
}
