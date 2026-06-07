use dioxus::prelude::*;

use crate::autosave::{self, SaveIndicator, SaveState};
use crate::bridge;
use crate::components::editor::Editor;
use crate::continuity::{use_continuity, ContinuityKey, EditSession};
use crate::timer::{sleep_ms, AUTOSAVE_DEBOUNCE_MS};
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
    let store = use_continuity();
    let mut sub_tab = use_signal(|| NotesSubTab::Recent);
    let mut view = use_signal(|| NotesView::List);

    // 1.8b nav restoration: re-open the note + sub-tab the user last had here.
    // Gated on `is_loaded` for the boot race; re-applies per mount for within-
    // session continuity. One-shot per mount via `restored`.
    let mut restored = use_signal(|| false);
    use_effect(move || {
        if *restored.peek() || !store.is_loaded() {
            return;
        }
        let saved = store.nav_peek();
        view.set(match saved.notes_view.as_deref() {
            Some("edit") => saved
                .notes_edit_id
                .clone()
                .map(NotesView::Edit)
                .unwrap_or(NotesView::List),
            // "new" intentionally falls back to List on restore: the draft's
            // content is preserved in the store (cursor/content continuity) and
            // resumes when New is reopened, so we skip the new→edit promotion
            // bookkeeping a faithful New restore would need.
            _ => NotesView::List,
        });
        sub_tab.set(match saved.notes_subtab.as_deref() {
            Some("search") => NotesSubTab::Search,
            _ => NotesSubTab::Recent,
        });
        restored.set(true);
    });

    // Write-through: mirror the view + sub-tab into nav (and persist to disk).
    // Gated on `restored` so empty defaults can't clobber saved nav pre-restore.
    use_effect(move || {
        if !*restored.read() {
            return;
        }
        let (vk, eid) = match &*view.read() {
            NotesView::List => ("list", None),
            NotesView::New => ("new", None),
            NotesView::Edit(id) => ("edit", Some(id.clone())),
        };
        let sub = match *sub_tab.read() {
            NotesSubTab::Recent => "recent",
            NotesSubTab::Search => "search",
        };
        store.update_nav(|n| {
            n.notes_view = Some(vk.to_string());
            n.notes_edit_id = eid;
            n.notes_subtab = Some(sub.to_string());
        });
    });

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
                        class: "w-full pl-10 pr-10 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent transition-colors",
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
                        onkeydown: move |e| {
                            if e.key() == Key::Escape && !query.read().is_empty() {
                                query.set(String::new());
                                results.set(vec![]);
                                loading.set(false);
                            }
                        },
                    }
                    if !query.read().is_empty() {
                        button {
                            r#type: "button",
                            "aria-label": "Clear search",
                            class: "absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded-md text-obsidian-text-muted hover:text-obsidian-text hover:bg-white/5 transition-colors",
                            onclick: move |_| {
                                query.set(String::new());
                                results.set(vec![]);
                                loading.set(false);
                            },
                            svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M6 18L18 6M6 6l12 12" }
                            }
                        }
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
    // Continuity store (1.3): the notes editing session is held at the app root
    // so a tab switch (which unmounts NoteEditor) doesn't lose typed text or a
    // half-written title. Saved notes key by id; an unsaved draft keys to the
    // single `NewNote` slot until its first save promotes it to `Note(id)`.
    let store = use_continuity();

    let mut loading = use_signal(|| true);
    let mut title = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut initial_content = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut save_status = use_signal(|| None::<String>);
    // True once an auto-save exhausts its retries (1.7); drives the `Failed`
    // pill, cleared when the next save starts or succeeds.
    let mut save_failed = use_signal(|| false);
    let mut fetch_error = use_signal(|| None::<String>);
    // Runtime-tracked id. Starts as the prop value, gets populated after the
    // first manual Save creates a new note. This is what auto-save, subsequent
    // manual Saves, and the continuity key all consult — without it, a second
    // click on a never-created note would create a duplicate.
    let mut local_note_id = use_signal(|| note_id.clone());
    // Mirrors the body that was last persisted to the backend. Auto-save
    // diffs `content` against this; load and successful save both update it
    // so programmatic content changes don't trigger phantom saves.
    let mut last_saved_content = use_signal(String::new);
    // Generation counter so a newer keystroke can cancel an older pending
    // save (each scheduled save bails if `save_generation` has moved on).
    let mut save_generation = use_signal(|| 0u64);
    // Caret offset (1.8b): tracked live via the editor's `on_cursor`, mirrored
    // into the session, fed back as `initial_cursor` on remount.
    let mut cursor = use_signal(|| 0usize);
    // Gate the write-through mirror until the first hydrate completes, so the
    // empty pre-load signals can't clobber an existing stored session.
    let mut hydrated = use_signal(|| false);

    let note_id_for_load = note_id.clone();
    let _load = use_future(move || {
        let id = note_id_for_load.clone();
        async move {
            // 1.8b boot race: wait for the store's disk snapshot so a note left
            // open at app-kill re-shows its unsaved session at boot instead of
            // racing the load and falling back to the backend copy.
            while !store.loaded_peek() {
                sleep_ms(20).await;
            }
            // Mount key: an existing note by id, else the single draft slot.
            let key = match &id {
                Some(id) => ContinuityKey::Note(id.clone()),
                None => ContinuityKey::NewNote,
            };
            let stored = store.get(&key);

            if let Some(s) = stored {
                // Restore an in-flight session: a saved note re-opened mid-edit,
                // or a draft resumed after navigating away. Prefer it over the
                // persisted copy (it's newer).
                title.set(s.title);
                last_saved_content.set(s.last_saved_content);
                content.set(s.content.clone());
                initial_content.set(s.content);
                save_generation.set(s.save_generation);
                cursor.set(s.cursor);
            } else if let Some(id) = id {
                // No session: load the persisted note from the backend.
                match bridge::invoke_get_generic_note(&id).await {
                    Ok(n) => {
                        title.set(n.title);
                        let raw = n.raw_text.clone();
                        last_saved_content.set(raw.clone());
                        content.set(raw.clone());
                        initial_content.set(raw);
                    }
                    Err(e) => fetch_error.set(Some(e)),
                }
            }
            // else: brand-new blank draft — signals keep their empty defaults.

            hydrated.set(true);
            loading.set(false);
        }
    });

    // Write-through mirror (1.3): keep the root-held session current so a tab
    // switch can't lose typed-but-unsaved work. The key is derived from
    // `local_note_id` each run, so when the first save promotes the draft
    // (None -> Some(id)) the session follows to `Note(id)`; the save handler
    // clears the stale `NewNote` slot.
    use_effect(move || {
        if !*hydrated.read() {
            return;
        }
        let key = match local_note_id.read().clone() {
            Some(id) => ContinuityKey::Note(id),
            None => ContinuityKey::NewNote,
        };
        let session = EditSession {
            title: title.read().clone(),
            content: content.read().clone(),
            last_saved_content: last_saved_content.read().clone(),
            save_generation: *save_generation.read(),
            cursor: *cursor.read(),
        };
        store.put(key, session);
    });

    // Auto-save (option ii): only runs once the note has an id. New-note
    // creation still requires a manual Save click; after that, local_note_id
    // is populated and this effect takes over for body updates.
    use_effect(move || {
        let current = content.read().clone();
        if current == *last_saved_content.peek() {
            return;
        }
        // Bail if we don't have an id yet — manual Save handles creation.
        let nid = match local_note_id.peek().clone() {
            Some(id) => id,
            None => return,
        };

        let scheduled_gen = {
            let mut g = save_generation.write();
            *g += 1;
            *g
        };

        spawn(async move {
            sleep_ms(AUTOSAVE_DEBOUNCE_MS).await;
            if *save_generation.peek() != scheduled_gen {
                return;
            }
            let snapshot = content.peek().clone();

            saving.set(true);
            save_failed.set(false);
            // Retry/backoff (1.7): re-issue the update with a fresh future each
            // attempt so a transient failure rides out per the backoff policy.
            let result = autosave::save_with_retry(|| {
                let nid = nid.clone();
                let snapshot = snapshot.clone();
                async move { bridge::invoke_update_generic_note(&nid, &snapshot).await }
            })
            .await;
            saving.set(false);

            match result {
                Ok(()) => {
                    last_saved_content.set(snapshot.clone());
                    if *content.peek() == snapshot {
                        bridge::js_mark_editor_clean();
                    }
                }
                Err(e) => {
                    save_failed.set(true);
                    save_status.set(Some(format!("Auto-save failed: {e}")));
                }
            }
        });
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
                    placeholder: if local_note_id.read().is_none() { "Untitled note" } else { "Title" },
                    value: "{title}",
                    oninput: move |e| title.set(e.value()),
                }
                {
                    // Glanceable save state (1.7): in-flight > failed > dirty > clean.
                    let save_state = if *saving.read() {
                        SaveState::Saving
                    } else if *save_failed.read() {
                        SaveState::Failed
                    } else if *content.read() != *last_saved_content.read() {
                        SaveState::Unsaved
                    } else {
                        SaveState::Saved
                    };
                    rsx! { SaveIndicator { state: save_state } }
                }
                button {
                    class: "px-4 py-1.5 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity disabled:opacity-50 shrink-0",
                    disabled: *saving.read() || title.read().trim().is_empty(),
                    onclick: move |_| {
                        let existing_id = local_note_id.peek().clone();
                        saving.set(true);
                        save_status.set(None);
                        save_failed.set(false);
                        spawn(async move {
                            let t = title.read().clone();
                            let body = content.read().clone();
                            let outcome = if let Some(nid) = existing_id {
                                let update = bridge::invoke_update_generic_note(&nid, &body).await;
                                if update.is_ok() {
                                    bridge::invoke_rename_generic_note(&nid, &t).await
                                } else {
                                    update
                                }
                            } else {
                                // First save creates the note. Capture the
                                // returned id so subsequent edits run through
                                // the update path (and auto-save) instead of
                                // creating duplicates.
                                bridge::invoke_create_generic_note(&t, &body)
                                    .await
                                    .map(|created| {
                                        local_note_id.set(Some(created.id));
                                        // Draft promoted to a real note: the
                                        // mirror now writes to `Note(id)`, so
                                        // clear the stale `NewNote` slot — a
                                        // later "New Note" should start blank.
                                        store.remove(&ContinuityKey::NewNote);
                                    })
                            };
                            saving.set(false);
                            match outcome {
                                Ok(()) => {
                                    last_saved_content.set(body.clone());
                                    save_status.set(Some("Saved".into()));
                                    if *content.peek() == body {
                                        bridge::js_mark_editor_clean();
                                    }
                                }
                                Err(e) => {
                                    save_failed.set(true);
                                    save_status.set(Some(format!("Save failed: {e}")));
                                }
                            }
                        });
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
                        initial_cursor: *cursor.peek(),
                        on_cursor: move |p: usize| cursor.set(p),
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
