use dioxus::prelude::*;

use crate::autosave::{self, SaveIndicator, SaveState};
use crate::bridge;
use crate::components::editor::Editor;
use crate::components::icon::{Icon, IconName};
use crate::components::primitives::{
    Banner, BannerKind, Button, ButtonSize, ButtonVariant, Card, IconButton, PageHeader,
    SegmentedNav,
};
use crate::components::tag_editor::TagChipEditor;
use crate::continuity::{use_continuity, ContinuityKey, EditSession};
use crate::note_frontmatter::{serialize_note, split_note, NoteProps};
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
        // `min-h-full flex flex-col` establishes the height chain so an open
        // note's editor (flex-1) fills the screen (Phase 5).
        div { class: "max-w-3xl mx-auto w-full min-h-full flex flex-col",
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
    let active_key = match active {
        NotesSubTab::Recent => "recent",
        NotesSubTab::Search => "search",
    };
    rsx! {
        SegmentedNav {
            class: "mb-6",
            items: vec![
                ("recent".to_string(), "Recent".to_string()),
                ("search".to_string(), "Search".to_string()),
            ],
            active: active_key.to_string(),
            on_select: move |k: String| {
                on_switch
                    .call(match k.as_str() {
                        "search" => NotesSubTab::Search,
                        _ => NotesSubTab::Recent,
                    })
            },
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

    // Load on mount, and re-load whenever a background pull lands (sync_refresh)
    // so notes synced from another device appear without a manual navigation.
    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read(); // subscribe: re-run on inbound sync
        spawn(async move {
            match bridge::invoke_list_generic_notes().await {
                Ok(list) => {
                    notes.set(list);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "animate-in fade-in duration-200",
            PageHeader { title: "Notes", class: "mb-6",
                Button {
                    onclick: move |_| on_new.call(()),
                    Icon { name: IconName::Plus, class: "w-5 h-5" }
                    span { "New Note" }
                }
            }

            if let Some(err) = &*error_msg.read() {
                Banner { kind: BannerKind::Error, class: "mb-4", "{err}" }
            }

            if *loading.read() {
                div { class: "py-20 text-center text-obsidian-text-muted", "Loading..." }
            } else if notes.read().is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted",
                    Icon { name: IconName::DocumentText, class: "w-16 h-16 mb-4 opacity-20", stroke: "1" }
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
                    Icon { name: IconName::Search, class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-obsidian-text-muted" }
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
                        IconButton {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            label: "Clear search",
                            class: "absolute right-2 top-1/2 -translate-y-1/2",
                            onclick: move |_| {
                                query.set(String::new());
                                results.set(vec![]);
                                loading.set(false);
                            },
                            Icon { name: IconName::Close, class: "w-4 h-4" }
                        }
                    }
                }
            }

            if query.read().trim().is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted opacity-40",
                    Icon { name: IconName::Search, class: "w-16 h-16 mb-4", stroke: "1" }
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
        Card {
            interactive: true,
            onclick: move |_| on_click.call(id.clone()),
            class: "group active:scale-[0.98]",
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

/// Split a raw note into typed properties + body and seed the editor from the
/// body. Called on every hydrate path — but **never** writes `content`, which
/// stays the full-text source of truth (so an untouched note can't phantom-save).
fn apply_raw_note(
    mut props: Signal<NoteProps>,
    mut body: Signal<String>,
    mut initial: Signal<String>,
    raw: &str,
) {
    let (p, b) = split_note(raw);
    props.set(p);
    initial.set(b.clone());
    body.set(b);
}

/// Recombine the typed properties + body back into `content` (the full raw text
/// that autosave/save persist). Called only from user edits — the panel's
/// `on_change` and the editor's `on_change` — never on hydrate.
fn recombine_note(
    props: Signal<NoteProps>,
    body: Signal<String>,
    mut content: Signal<String>,
) {
    content.set(serialize_note(&props.read(), body.read().as_str()));
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
    // `content` is the full raw text (frontmatter + body) — the single source of
    // truth for autosave/save. The typed properties panel + a body-only editor
    // are two inputs that recombine into it (Phase 5.1/5.2, mirroring the
    // journal). `props`/`body` are derived from `content` on load and only feed
    // back into it on a user edit.
    let mut content = use_signal(String::new);
    let props = use_signal(NoteProps::default);
    let mut body = use_signal(String::new);
    let initial_content = use_signal(String::new);
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
            // Only a DIRTY session (unsaved edits) may shadow the backend copy;
            // a clean one must yield so edits that synced in from another device
            // surface instead of being permanently masked (the "nothing syncs to
            // desktop" bug). Continuity of typed-but-unsaved text is preserved.
            let stored = store.get(&key).filter(|s| s.content != s.last_saved_content);

            if let Some(s) = stored {
                // Restore an in-flight session: a saved note re-opened mid-edit,
                // or a draft resumed after navigating away. Prefer it over the
                // persisted copy (it's newer).
                title.set(s.title);
                last_saved_content.set(s.last_saved_content);
                content.set(s.content.clone());
                apply_raw_note(props, body, initial_content, &s.content);
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
                        apply_raw_note(props, body, initial_content, &raw);
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

    // Live inbound refresh: mirror of the journal `DayView` effect. On an applied
    // pull (`sync_epoch` bump), if this note is open with no unsaved BODY edits,
    // adopt the fresh backend body and push it into the already-seeded editor.
    // Only saved notes (id present) have a backend copy; drafts are skipped.
    // BODY only — the title is a separate signal with no last-saved mirror to
    // diff against, so live-syncing it could clobber an unsaved title edit; the
    // title reconciles on the next remount via the (dirty-filtered) load future.
    {
        let sync_epoch = crate::sync_refresh::use_sync_epoch();
        use_effect(move || {
            let _ = sync_epoch.read();
            if !*hydrated.peek() {
                return;
            }
            // Freeze once the user has typed in this editor this session — see the
            // journal `DayView` effect for why `content == last_saved_content`
            // alone is insufficient (autosave clears it between keystrokes).
            if bridge::js_editor_ever_dirty().unwrap_or(false)
                || *content.peek() != *last_saved_content.peek()
            {
                return;
            }
            let Some(nid) = local_note_id.peek().clone() else {
                return;
            };
            spawn(async move {
                let Ok(n) = bridge::invoke_get_generic_note(&nid).await else {
                    return;
                };
                // Re-check after the await — the user may have begun typing.
                if bridge::js_editor_ever_dirty().unwrap_or(false)
                    || *content.peek() != *last_saved_content.peek()
                {
                    return;
                }
                let raw = n.raw_text.clone();
                if raw == *content.peek() {
                    return;
                }
                last_saved_content.set(raw.clone());
                // `apply_raw_note` splits the full raw (frontmatter + body) into the
                // properties panel + the editor body. The editor only holds the
                // BODY, so push the split body — pushing the full raw would dump the
                // frontmatter as plain text under the rendered properties.
                apply_raw_note(props, body, initial_content, &raw);
                content.set(raw.clone());
                let editor_body = body.peek().clone();
                bridge::js_set_editor_content(&editor_body);
                bridge::js_mark_editor_clean();
            });
        });
    }

    rsx! {
        // Fill-height flex column so the editor grows to fill the screen (Phase 5).
        div { class: "animate-in fade-in slide-in-from-bottom-4 duration-300 flex flex-col flex-1 min-h-0",
            div { class: "flex justify-between items-center mb-6 gap-3",
                IconButton {
                    label: "Back",
                    onclick: move |_| on_back.call(()),
                    Icon { name: IconName::ArrowLeft, class: "w-5 h-5" }
                }
                input {
                    // `min-w-0`: without it the `flex-1` input keeps its
                    // intrinsic (auto) min-width, so on a narrow (mobile) header
                    // the row overflows and shoves the Save button off-screen.
                    // min-w-0 lets flex actually shrink the field. (friction 2026-07-04)
                    class: "flex-1 min-w-0 px-3 py-2 bg-transparent border-b border-white/10 text-lg font-bold text-obsidian-text outline-none focus:border-obsidian-accent transition-colors",
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
                Button {
                    class: "shrink-0",
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
                Banner { kind: BannerKind::Error, class: "mb-4", "{err}" }
            }

            if *loading.read() {
                div { class: "py-20 text-center text-obsidian-text-muted", "Loading..." }
            } else {
                NotePropertiesPanel {
                    model: props,
                    on_change: move |_| recombine_note(props, body, content),
                }
                div { class: "flex-1 flex flex-col min-h-0",
                    Editor {
                        initial_content: initial_content.read().clone(),
                        on_change: move |new_body: String| {
                            body.set(new_body);
                            recombine_note(props, body, content);
                        },
                        initial_cursor: *cursor.peek(),
                        on_cursor: move |p: usize| cursor.set(p),
                    }
                }
            }

            if let Some(status) = &*save_status.read() {
                Banner {
                    kind: BannerKind::Info,
                    class: "mt-4 animate-in zoom-in-95 duration-200",
                    "{status}"
                }
            }
        }
    }
}

/// Typed properties card above a generic note's editor (Phase 5.1/5.2). The
/// note's `title` is a separate field and notes carry no date/reflections, so
/// this is just the tags chip editor + the raw escape hatch for any other
/// (imported/legacy) frontmatter. Mirrors `journal::JournalPropertiesPanel`.
#[component]
fn NotePropertiesPanel(model: Signal<NoteProps>, on_change: EventHandler<()>) -> Element {
    // `model` renamed to `props` locally — a component param literally named
    // `props` collides with the `#[component]` macro's generated binding.
    let mut props = model;
    // Expand the raw escape hatch by default only when it already holds content.
    let mut show_raw = use_signal(|| !props.peek().legacy_raw.is_empty());

    let tags = props.read().tags.clone();
    let has_legacy = !props.read().legacy_raw.is_empty();

    rsx! {
        div { class: "mb-4 rounded-lg border border-white/5 bg-obsidian-sidebar/30 divide-y divide-white/5 text-sm",
            // Tags — chip editor.
            div { class: "flex items-start gap-3 px-3 py-2",
                span { class: "w-24 shrink-0 pt-1 text-xs font-medium text-obsidian-text-muted", "Tags" }
                TagChipEditor {
                    tags,
                    on_add: move |t: String| {
                        props.write().tags.push(t);
                        on_change.call(());
                    },
                    on_remove: move |idx: usize| {
                        if idx < props.read().tags.len() {
                            props.write().tags.remove(idx);
                            on_change.call(());
                        }
                    },
                }
            }

            // Raw escape hatch for other / imported frontmatter.
            div { class: "px-3 py-2",
                button {
                    r#type: "button",
                    class: "flex items-center gap-1.5 text-xs font-medium text-obsidian-text-muted hover:text-obsidian-text transition-colors",
                    onclick: move |_| {
                        let v = *show_raw.peek();
                        show_raw.set(!v);
                    },
                    span { class: "text-[10px] w-2", if *show_raw.read() { "▾" } else { "▸" } }
                    "Raw properties"
                    if has_legacy {
                        span { class: "w-1 h-1 rounded-full bg-obsidian-accent" }
                    }
                }
                if *show_raw.read() {
                    textarea {
                        class: "mt-2 w-full bg-obsidian-bg/50 border border-white/5 rounded p-2 text-xs font-mono text-obsidian-text resize-y focus:outline-none focus:border-obsidian-accent/40",
                        rows: "3",
                        placeholder: "Other frontmatter, preserved verbatim",
                        value: "{props.read().legacy_raw}",
                        oninput: move |e| {
                            props.write().legacy_raw = e.value();
                            on_change.call(());
                        },
                    }
                }
            }
        }
    }
}
