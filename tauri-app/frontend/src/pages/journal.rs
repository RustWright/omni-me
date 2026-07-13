use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::autosave::{self, SaveIndicator, SaveState};
use crate::bridge;
use crate::components::editor::Editor;
use crate::components::tag_editor::TagChipEditor;
use crate::continuity::{use_continuity, ContinuityKey, EditSession};
use crate::journal_template;
use crate::note_frontmatter::{serialize_journal, split_journal, JournalProps};
use crate::timer::{sleep_ms, AUTOSAVE_DEBOUNCE_MS};
use crate::types::JournalEntryItem;
use crate::user_date::UserDate;

/// Start strip / travel thresholds (CSS px) for the calendar drawer's right-edge
/// swipe. Mirror of the app-shell nav swipe in `main.rs`, but anchored to the
/// *right* edge: swipe left from the edge to open, swipe right to close.
const CAL_EDGE_START_PX: f64 = 24.0;
const CAL_SWIPE_OPEN_PX: f64 = 48.0;
const CAL_SWIPE_CLOSE_PX: f64 = 48.0;

/// Current viewport width in CSS px (for right-edge swipe detection). Falls back
/// to 0 — which makes the "near the right edge" test never fire — if the window
/// is somehow unavailable, so the toolbar button always remains as the opener.
fn viewport_width() -> f64 {
    web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// Word + character count for a note body (the calendar drawer's footer stats,
/// Obsidian parity). Words are whitespace-delimited tokens (runs of whitespace
/// collapse); characters are Unicode scalar values, so a multi-byte glyph counts
/// once. Frontmatter is excluded by construction — `body` is already the prose
/// with the properties lifted into the panel (Phase 5).
fn body_stats(body: &str) -> (usize, usize) {
    let words = body.split_whitespace().count();
    let chars = body.chars().count();
    (words, chars)
}

#[component]
pub fn JournalPage() -> Element {
    let store = use_continuity();
    let tz_signal: Signal<Tz> = use_context();
    // `&*signal.read()` is explicit on purpose: makes it clear we're
    // borrowing through a signal guard, not coercing the guard itself.
    #[allow(clippy::explicit_auto_deref)]
    let today = UserDate::today(&*tz_signal.read()).to_date_string();

    let mut selected_date = use_signal(|| today.clone());
    // The calendar is a right-edge drawer overlaying the day view (opened by a
    // right-edge swipe or the toolbar button), not a separate sub-tab.
    let mut calendar_open = use_signal(|| false);
    // Right-edge swipe tracking for the calendar drawer (mirror of the app-shell
    // left-edge nav swipe): `Some(start_x)` once a tracked touch begins — near
    // the right edge when closed (candidate open-swipe), or anywhere when open
    // (candidate close-swipe). `peek` everywhere; the gesture mutates state but
    // nothing renders off this signal.
    let mut cal_swipe_start_x = use_signal(|| None::<f64>);

    // The viewed day's live editor body, mirrored up out of the keyed `DayView`
    // so the calendar drawer's footer (which lives here in the parent) can show
    // the note's word/char count. Written by `DayView`, read only here.
    let viewed_body = use_signal(String::new);

    // 1.8b nav restoration: re-open the day the user last had here. Gated on
    // `is_loaded` so it picks up the disk snapshot even when this page mounts
    // before the boot read finishes; re-applies on every remount for within-
    // session continuity (tab away to Notes and back keeps the viewed day).
    // One-shot per mount via `restored`.
    let mut restored = use_signal(|| false);
    use_effect(move || {
        if *restored.peek() || !store.is_loaded() {
            return;
        }
        let saved = store.nav_peek();
        if let Some(d) = saved.journal_date {
            selected_date.set(d);
        }
        restored.set(true);
    });

    // Write-through: mirror the viewed day into nav (and persist to disk). Gated
    // on `restored` so the empty default can't clobber the saved nav before the
    // restore above applies it.
    use_effect(move || {
        if !*restored.read() {
            return;
        }
        let date = selected_date.read().clone();
        store.update_nav(|n| n.journal_date = Some(date));
    });

    rsx! {
        // `min-h-full flex flex-col` establishes the height chain so DayView's
        // editor region (flex-1) can fill the screen (Phase 5). `max-w-3xl` keeps
        // a readable column on desktop; it's full-width below 768px.
        div {
            class: "max-w-3xl mx-auto w-full min-h-full flex flex-col",
            // Right-edge swipe to open the calendar drawer (mirror of the app-
            // shell left-edge nav swipe). No preventDefault — scroll/typing/
            // selection stay intact; we only act on a touch that starts near the
            // right edge while closed, or any touch while open (to swipe shut).
            ontouchstart: move |e| {
                let start = e.touches().first().map(|t| t.client_coordinates().x);
                if *calendar_open.peek() {
                    cal_swipe_start_x.set(start);
                } else {
                    let w = viewport_width();
                    cal_swipe_start_x.set(start.filter(|x| w - *x <= CAL_EDGE_START_PX));
                }
            },
            ontouchmove: move |e| {
                let Some(start) = *cal_swipe_start_x.peek() else {
                    return;
                };
                let Some(x) = e.touches().first().map(|t| t.client_coordinates().x) else {
                    return;
                };
                if *calendar_open.peek() {
                    // Open drawer: rightward travel past the threshold closes it.
                    if x - start >= CAL_SWIPE_CLOSE_PX {
                        calendar_open.set(false);
                        cal_swipe_start_x.set(None);
                    }
                } else if start - x >= CAL_SWIPE_OPEN_PX {
                    // Closed drawer: leftward travel from the right edge opens it.
                    calendar_open.set(true);
                    cal_swipe_start_x.set(None);
                }
            },
            ontouchend: move |_| cal_swipe_start_x.set(None),

            // Slim toolbar: the calendar opener (the button is desktop's opener,
            // since edge-swipe is touch-only).
            div { class: "flex justify-end mb-4",
                button {
                    class: "flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium rounded-md bg-obsidian-sidebar/40 border border-white/5 text-obsidian-text-muted hover:text-obsidian-text hover:bg-white/5 transition-colors",
                    "aria-label": "Open calendar",
                    onclick: move |_| calendar_open.set(true),
                    svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" }
                    }
                    "Calendar"
                }
            }

            // DayView is keyed by date so a day-jump remounts it (fresh entry
            // load + its own continuity slot). A bare `key` on a directly-
            // rendered component is a no-op in Dioxus — keys only drive
            // remounting inside a list — so we render it through a one-element
            // `for` to get list semantics. Without this the calendar drawer
            // (which, unlike the old sub-tab, never unmounts DayView) would just
            // swap the `date` prop and leave the previous day's content on screen.
            for day in std::iter::once(selected_date.read().clone()) {
                DayView {
                    key: "{day}",
                    date: day.clone(),
                    today: today.clone(),
                    viewed_body,
                    on_back_to_today: {
                        let today = today.clone();
                        move |_| selected_date.set(today.clone())
                    },
                }
            }

            {
                let (words, chars) = body_stats(&viewed_body.read());
                rsx! {
                    CalendarDrawer {
                        open: calendar_open,
                        today: today.clone(),
                        selected: selected_date.read().clone(),
                        words,
                        chars,
                        on_select: move |d: String| {
                            selected_date.set(d);
                            calendar_open.set(false);
                        },
                        on_close: move |_| calendar_open.set(false),
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DayView — shows the entry for a specific date. Used for today + past days.
// ---------------------------------------------------------------------------

/// Split a raw journal note into the properties panel's typed `props` + the
/// editor `body`. Called wherever raw text is loaded into the day (hydrate,
/// reopen, close). Never touches `content` — that stays the source of truth.
fn apply_raw(mut props: Signal<JournalProps>, mut body: Signal<String>, raw: &str) {
    let (p, b) = split_journal(raw);
    props.set(p);
    body.set(b);
}

/// Recombine the panel's `props` + the editor `body` back into the full note
/// `content` (the single source of truth for autosave / continuity / the
/// backend `is_complete`). Called from user edits only, so an untouched entry
/// never diverges from its loaded raw.
fn recombine(props: Signal<JournalProps>, body: Signal<String>, mut content: Signal<String>) {
    let p = props.read();
    let b = body.read();
    content.set(serialize_journal(&p, b.as_str()));
}

/// Day entry view, parameterized on date. Parent keys by `date`, so navigating
/// between days remounts this component and resets its load state cleanly.
#[component]
fn DayView(
    date: String,
    today: String,
    viewed_body: Signal<String>,
    on_back_to_today: EventHandler<()>,
) -> Element {
    let mut entry = use_signal(|| None::<JournalEntryItem>);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut save_status = use_signal(|| None::<String>);
    // True once an auto-save exhausts its retries (1.7). Drives the `Failed`
    // save-state pill; cleared when the next save starts or succeeds.
    let mut save_failed = use_signal(|| false);
    let mut content = use_signal(String::new);
    // Mirrors what's currently persisted to the backend. Auto-save compares
    // `content` against this to decide whether a save is needed; it's also
    // updated by the load future / manual save / reopen / close handlers so
    // those programmatic content changes don't trigger phantom saves.
    let mut last_saved_content = use_signal(String::new);
    // Generation counter so a newer keystroke can cancel an earlier pending
    // debounced save: each scheduled save captures its gen at schedule time
    // and bails out post-sleep if the counter has moved on.
    let mut save_generation = use_signal(|| 0u64);
    // Caret offset (1.8b). Tracked live via the editor's `on_cursor` callback,
    // mirrored into the session, and fed back as `initial_cursor` on remount so
    // returning to this day restores the caret + viewport.
    let mut cursor = use_signal(|| 0usize);
    // Properties panel (5.1/5.2): the typed frontmatter + the editor body are
    // derived views of `content` — populated from it on load via `apply_raw` and
    // recombined back into it on any user edit via `recombine`. `content` remains
    // the single source of truth, so autosave / continuity / persistence are
    // untouched. `body` is mutated in the editor's `on_change`, so it needs `mut`;
    // `props` is only ever mutated inside the panel (by value), so it does not.
    let props = use_signal(JournalProps::default);
    let mut body = use_signal(String::new);

    // Continuity store (1.2): this day's editing session is held at the app
    // root, so switching tabs (which unmounts DayView) no longer drops typed
    // text. Keyed by date — each day gets its own slot.
    let store = use_continuity();
    let continuity_key = ContinuityKey::Journal(date.clone());
    // Gate the write-through mirror below until the first hydrate finishes, so
    // the empty pre-load signals can't clobber an existing stored session.
    let mut hydrated = use_signal(|| false);

    let is_today_view = date == today;

    let date_for_load = date.clone();
    let key_for_load = continuity_key.clone();
    let _load = use_future(move || {
        let d = date_for_load.clone();
        let key = key_for_load.clone();
        async move {
            // 1.8b boot race: the store loads its disk snapshot asynchronously.
            // Wait for it so the *initially-open* day sees a restored session
            // instead of racing the read and falling back to the backend copy.
            // (Cheap local read — resolves in a tick or two; mirrors the editor
            // bundle's existing poll-until-ready pattern.)
            while !store.loaded_peek() {
                sleep_ms(20).await;
            }
            // Prefer an in-flight session (content the user left mid-edit when
            // they navigated away) over the persisted copy. Entry metadata
            // (id / closed / complete) always comes fresh from the backend.
            //
            // ...but ONLY when that session is DIRTY (holds unsaved edits). A
            // clean session (content == last_saved_content) has nothing to
            // preserve, so it must yield to the fresh backend copy — otherwise a
            // clean session persisted at first open permanently shadows edits
            // that synced in from another device (the "nothing syncs to desktop"
            // bug). The continuity guarantee (never lose typed-but-unsaved text)
            // is untouched: only clean sessions defer.
            let stored = store.get(&key).filter(|s| s.content != s.last_saved_content);
            match bridge::invoke_get_journal_by_date(&d).await {
                Ok(Some(e)) => {
                    if let Some(s) = stored {
                        apply_raw(props, body, &s.content);
                        content.set(s.content);
                        last_saved_content.set(s.last_saved_content);
                        save_generation.set(s.save_generation);
                        cursor.set(s.cursor);
                    } else {
                        let raw = e.raw_text.clone();
                        last_saved_content.set(raw.clone());
                        apply_raw(props, body, &raw);
                        content.set(raw);
                    }
                    entry.set(Some(e));
                    error_msg.set(None);
                    hydrated.set(true);
                }
                Ok(None) => {
                    if let Some(s) = stored {
                        apply_raw(props, body, &s.content);
                        content.set(s.content);
                        last_saved_content.set(s.last_saved_content);
                        save_generation.set(s.save_generation);
                        cursor.set(s.cursor);
                    } else {
                        // New entry: prime both signals with the default
                        // template so an immediate Save without keystrokes still
                        // persists it, and so auto-save doesn't treat the
                        // template-vs-empty diff as user input.
                        let template = journal_template::render(&d);
                        last_saved_content.set(template.clone());
                        apply_raw(props, body, &template);
                        content.set(template);
                    }
                    entry.set(None);
                    error_msg.set(None);
                    hydrated.set(true);
                }
                // On error, leave `hydrated` false: the mirror stays inert so a
                // transient fetch failure can't overwrite a good stored session.
                Err(e) => error_msg.set(Some(e)),
            }
            loading.set(false);
        }
    });

    // Write-through mirror (1.2): keep the root-held session current so a tab
    // switch (which unmounts this component) can't lose typed-but-unsaved text.
    // Runs only post-hydrate; re-fires on any content / save-state change.
    {
        let key_for_mirror = continuity_key.clone();
        use_effect(move || {
            if !*hydrated.read() {
                return;
            }
            let session = EditSession {
                // Journal entries have no title — keyed by date.
                title: String::new(),
                content: content.read().clone(),
                last_saved_content: last_saved_content.read().clone(),
                save_generation: *save_generation.read(),
                cursor: *cursor.read(),
            };
            store.put(key_for_mirror.clone(), session);
        });
    }

    // Mirror the live body up to `JournalPage` so the calendar drawer's footer
    // can show this note's word/char count. The drawer lives in the parent, but
    // the body lives here in the keyed `DayView`. Post-hydrate only, so the empty
    // pre-load body doesn't briefly flash "0 words".
    use_effect(move || {
        if !*hydrated.read() {
            return;
        }
        let mut viewed_body = viewed_body;
        viewed_body.set(body.read().clone());
    });

    // Auto-save: any divergence between `content` and `last_saved_content`
    // schedules a debounced save. The generation counter cancels older
    // pending saves when the user types again before the debounce expires.
    {
        let date_for_autosave = date.clone();
        use_effect(move || {
            let current = content.read().clone();
            // peek() avoids subscribing to last_saved_content — we only re-run
            // on user input (content changes), not on our own write-back when
            // a save resolves. That self-trigger would schedule a redundant
            // pass that gen-cancels itself one tick later.
            if current == *last_saved_content.peek() {
                return;
            }
            // Closed journals must not auto-save (the manual Save button is
            // also disabled in this state).
            if entry.read().as_ref().map(|e| e.closed).unwrap_or(false) {
                return;
            }

            let scheduled_gen = {
                let mut g = save_generation.write();
                *g += 1;
                *g
            };

            let date = date_for_autosave.clone();
            spawn(async move {
                sleep_ms(AUTOSAVE_DEBOUNCE_MS).await;

                // Stale check: a newer keystroke scheduled a fresher save
                // while we were waiting. Bail out and let it run instead.
                if *save_generation.peek() != scheduled_gen {
                    return;
                }
                // Re-confirm not-closed: a Close Day click could have landed
                // during the 1s wait.
                if entry.peek().as_ref().map(|e| e.closed).unwrap_or(false) {
                    return;
                }

                let snapshot = content.peek().clone();
                let jid = entry.peek().as_ref().map(|e| e.journal_id.clone());

                saving.set(true);
                save_failed.set(false);
                // Retry/backoff (1.7): each attempt re-issues the same create-or-
                // update with a fresh future (cloning the captured strings), so a
                // transient failure rides out per the backoff policy. `Some(entry)`
                // distinguishes a create (returns the new entry) from an update.
                let result = autosave::save_with_retry(|| {
                    let snapshot = snapshot.clone();
                    let jid = jid.clone();
                    let date = date.clone();
                    async move {
                        match jid {
                            Some(id) => bridge::invoke_update_journal_entry(&id, &snapshot)
                                .await
                                .map(|_| None),
                            None => bridge::invoke_create_journal_entry(&date, &snapshot)
                                .await
                                .map(Some),
                        }
                    }
                })
                .await;
                saving.set(false);

                match result {
                    Ok(maybe_created) => {
                        if let Some(created) = maybe_created {
                            entry.set(Some(created));
                        }
                        last_saved_content.set(snapshot.clone());
                        // Skip-if-stale: only flip the editor's dirty state to
                        // clean when the persisted snapshot still matches the
                        // live content. If the user typed during the save, a
                        // newer auto-save is already scheduled — let it clean.
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
    }

    // Live inbound refresh: when an auto-pull lands new events (`sync_epoch`
    // bumps) and this day is open and UNTOUCHED this session, adopt the fresh
    // backend copy and push it into the already-seeded editor. Only ONE DayView
    // is mounted at a time, so the editor targeted by `js_set_editor_content` is
    // this day's. The editor seeds once on mount and ignores `initial_content`
    // changes, so a live update must go through `js_set_editor_content`, not a
    // prop; a remount instead re-seeds from the (dirty-filtered) load future.
    //
    // Freeze guard: `js_editor_ever_dirty()` is a STICKY "the user typed in this
    // editor at least once this session" flag. We can't use `content ==
    // last_saved_content` alone — autosave sets them equal within ~1s of every
    // keystroke, so between autosaves the editor looks clean and an incoming
    // remote edit would clobber text the user is actively typing (the "clobbered
    // constantly" bug). Once the user has edited here, live-refresh is frozen
    // until they navigate away and back (which remounts → clears the flag).
    {
        let sync_epoch = crate::sync_refresh::use_sync_epoch();
        let date_for_sync = date.clone();
        use_effect(move || {
            // Subscribe to the sync epoch; `peek` the rest so we re-run only on
            // an applied pull, not on our own signal writes.
            let _ = sync_epoch.read();
            if !*hydrated.peek() {
                return;
            }
            if bridge::js_editor_ever_dirty().unwrap_or(false)
                || *content.peek() != *last_saved_content.peek()
            {
                return;
            }
            let d = date_for_sync.clone();
            spawn(async move {
                let Ok(Some(e)) = bridge::invoke_get_journal_by_date(&d).await else {
                    return;
                };
                // Re-check after the await: the user may have started typing while
                // the fetch was in flight.
                if bridge::js_editor_ever_dirty().unwrap_or(false)
                    || *content.peek() != *last_saved_content.peek()
                {
                    return;
                }
                let raw = e.raw_text.clone();
                let changed = raw != *content.peek();
                // Refresh metadata (closed/complete) regardless; only touch the
                // editor when the body actually changed.
                entry.set(Some(e));
                if changed {
                    last_saved_content.set(raw.clone());
                    // `apply_raw` splits the full raw (frontmatter + body) into the
                    // properties panel (`props`) + the editor `body`. The editor
                    // only ever holds the BODY — the frontmatter renders in the
                    // panel above — so push the split body, NOT the full raw, or
                    // the frontmatter gets dumped as plain text under the panel.
                    apply_raw(props, body, &raw);
                    content.set(raw.clone());
                    let editor_body = body.peek().clone();
                    bridge::js_set_editor_content(&editor_body);
                    bridge::js_mark_editor_clean();
                }
            });
        });
    }

    rsx! {
        // Fill-height flex column so the editor region (flex-1) can grow to fill
        // the screen instead of sitting as a fixed island (Phase 5 "editor feel").
        div { class: "animate-in fade-in duration-200 flex flex-col flex-1 min-h-0",
            if !is_today_view {
                div { class: "mb-3",
                    button {
                        class: "text-sm text-obsidian-text-muted hover:text-obsidian-accent transition-colors",
                        onclick: move |_| on_back_to_today.call(()),
                        "← Back to today"
                    }
                }
            }

            if let Some(err) = &*error_msg.read() {
                div { class: "bg-red-900/20 text-red-400 px-3 py-2 rounded border border-red-900/50 mb-4 text-sm",
                    "{err}"
                }
            }

            // Header: heading + date + status pills + action buttons
            div { class: "flex flex-wrap justify-between items-center gap-3 mb-6",
                div { class: "flex items-center gap-3",
                    h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                        if is_today_view { "Today" } else { "Entry" }
                    }
                    span { class: "text-sm font-mono text-obsidian-text-muted", "{date}" }
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
                    {
                        // Glanceable save state (1.7), derived from existing
                        // signals: in-flight > failed > dirty > clean.
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
                }

                div { class: "flex items-center gap-2",
                    {
                        let is_closed = entry.read().as_ref().map(|e| e.closed).unwrap_or(false);
                        let journal_id = entry.read().as_ref().map(|e| e.journal_id.clone());
                        let have_entry = entry.read().is_some();

                        rsx! {
                            if is_closed {
                                button {
                                    class: "px-3 py-1.5 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text text-sm transition-colors",
                                    onclick: {
                                        let jid = journal_id.clone();
                                        let date = date.clone();
                                        move |_| {
                                            let jid = jid.clone();
                                            let date = date.clone();
                                            spawn(async move {
                                                if let Some(id) = jid
                                                    && bridge::invoke_reopen_journal_entry(&id).await.is_ok()
                                                    && let Ok(Some(refreshed)) =
                                                        bridge::invoke_get_journal_by_date(&date).await
                                                {
                                                    let raw = refreshed.raw_text.clone();
                                                    last_saved_content.set(raw.clone());
                                                    apply_raw(props, body, &raw);
                                                    content.set(raw);
                                                    entry.set(Some(refreshed));
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
                                        let date = date.clone();
                                        move |_| {
                                            let jid = jid.clone();
                                            let date = date.clone();
                                            spawn(async move {
                                                if let Some(id) = jid
                                                    && bridge::invoke_close_journal_entry(&id, "manual").await.is_ok()
                                                    && let Ok(Some(refreshed)) =
                                                        bridge::invoke_get_journal_by_date(&date).await
                                                {
                                                    let raw = refreshed.raw_text.clone();
                                                    last_saved_content.set(raw.clone());
                                                    apply_raw(props, body, &raw);
                                                    content.set(raw);
                                                    entry.set(Some(refreshed));
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
                                    let date = date.clone();
                                    let jid = journal_id.clone();
                                    move |_| {
                                        let date = date.clone();
                                        let jid = jid.clone();
                                        saving.set(true);
                                        save_status.set(None);
                                        save_failed.set(false);
                                        spawn(async move {
                                            let text = content.read().clone();
                                            let result = if let Some(id) = jid {
                                                bridge::invoke_update_journal_entry(&id, &text)
                                                    .await
                                                    .map(|_| ())
                                            } else {
                                                match bridge::invoke_create_journal_entry(&date, &text).await {
                                                    Ok(created) => {
                                                        entry.set(Some(created));
                                                        Ok(())
                                                    }
                                                    Err(e) => Err(e),
                                                }
                                            };
                                            saving.set(false);
                                            match result {
                                                Ok(()) => {
                                                    last_saved_content.set(text.clone());
                                                    save_status.set(Some("Saved".into()));
                                                    if *content.read() == text {
                                                        bridge::js_mark_editor_clean();
                                                    }
                                                }
                                                Err(e) => {
                                                    save_failed.set(true);
                                                    save_status.set(Some(format!("Save failed: {e}")));
                                                }
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
                    // Seed CodeMirror from the hydrated *body* (frontmatter now
                    // lives in the properties panel above). `peek` so DayView
                    // doesn't re-subscribe here; the editor seeds once on mount
                    // and ignores later `initial_content` prop changes.
                    let initial = body.peek().clone();
                    // Full-bleed, fill-height writing surface (Phase 5): no card
                    // chrome — the editor blends into the page and fills the
                    // remaining column height. `opacity-60` keeps the closed-day
                    // read-only signal.
                    let editor_class = if is_closed {
                        "flex-1 flex flex-col min-h-0 opacity-60"
                    } else {
                        "flex-1 flex flex-col min-h-0"
                    };

                    rsx! {
                        JournalPropertiesPanel {
                            model: props,
                            read_only: is_closed,
                            on_change: move |_| recombine(props, body, content),
                        }
                        div { class: "{editor_class}",
                            Editor {
                                initial_content: initial,
                                on_change: move |new_body: String| {
                                    body.set(new_body);
                                    recombine(props, body, content);
                                },
                                read_only: is_closed,
                                journal_mode: true,
                                initial_cursor: *cursor.peek(),
                                on_cursor: move |p: usize| cursor.set(p),
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

        }
    }
}

// ---------------------------------------------------------------------------
// JournalPropertiesPanel — Obsidian-style typed frontmatter card shown above the
// editor body. Edits mutate the `props` signal and fire `on_change` so DayView
// recombines props + body back into the note `content`. See `note_frontmatter`.
// ---------------------------------------------------------------------------

#[component]
fn JournalPropertiesPanel(
    model: Signal<JournalProps>,
    #[props(default = false)] read_only: bool,
    on_change: EventHandler<()>,
) -> Element {
    // `model` is renamed to `props` locally — a component param literally named
    // `props` collides with the `#[component]` macro's generated binding.
    let mut props = model;
    // Expand the raw escape hatch by default only when it already holds content.
    let mut show_raw = use_signal(|| !props.peek().legacy_raw.is_empty());

    let date = props.read().date.clone();
    let tags = props.read().tags.clone();
    let has_legacy = !props.read().legacy_raw.is_empty();

    rsx! {
        div { class: "mb-4 rounded-lg border border-white/5 bg-obsidian-sidebar/30 divide-y divide-white/5 text-sm",
            // Date — read-only (it's the entry key).
            div { class: "flex items-center gap-3 px-3 py-2",
                span { class: "w-24 shrink-0 text-xs font-medium text-obsidian-text-muted", "Date" }
                span { class: "font-mono text-obsidian-text", "{date}" }
            }

            // Tags — chip editor.
            div { class: "flex items-start gap-3 px-3 py-2",
                span { class: "w-24 shrink-0 pt-1 text-xs font-medium text-obsidian-text-muted", "Tags" }
                TagChipEditor {
                    tags,
                    read_only,
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

            ReflectionField {
                label: "Homework for life",
                value: props.read().homework_for_life.clone(),
                read_only,
                on_input: move |v: String| {
                    props.write().homework_for_life = v;
                    on_change.call(());
                },
            }
            ReflectionField {
                label: "Grateful for",
                value: props.read().grateful_for.clone(),
                read_only,
                on_input: move |v: String| {
                    props.write().grateful_for = v;
                    on_change.call(());
                },
            }
            ReflectionField {
                label: "Learnt today",
                value: props.read().learnt_today.clone(),
                read_only,
                on_input: move |v: String| {
                    props.write().learnt_today = v;
                    on_change.call(());
                },
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
                        readonly: read_only,
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

/// One labeled reflection field in the properties panel. A borderless,
/// vertically-resizable textarea that reads as prose, not a form input.
#[component]
fn ReflectionField(
    label: String,
    value: String,
    #[props(default = false)] read_only: bool,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "flex items-start gap-3 px-3 py-2",
            span { class: "w-24 shrink-0 pt-1 text-xs font-medium text-obsidian-text-muted", "{label}" }
            textarea {
                // `field-sizing: content` auto-grows the textarea to fit its
                // text (Chromium/webview) so reflections read as prose and never
                // clip; `resize-y` still lets the user drag taller.
                class: "flex-1 bg-transparent text-obsidian-text placeholder:text-obsidian-text-muted/40 resize-y focus:outline-none leading-snug py-0.5",
                style: "field-sizing: content;",
                rows: "1",
                readonly: read_only,
                placeholder: if read_only { "" } else { "…" },
                value: "{value}",
                oninput: move |e| on_input.call(e.value()),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CalendarDrawer — right-edge slide-in month grid overlaying the day view, with
// a per-day activity dot (has-entry) + a day-complete check. Mirrors the left
// nav drawer: always mounted, class-toggled so it animates; scrim tap or the
// inverse swipe closes it. Selecting a day jumps the viewed date + closes.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct MonthCell {
    date: NaiveDate,
    in_current_month: bool,
}

/// Build the list of calendar cells for a month view.
///
/// The grid is always 6 rows × 7 cols (42 cells) so its height stays constant
/// as the user navigates months. Six rows are needed to cover every layout —
/// a 5-row grid truncates the last 1-2 days of long months that start late in
/// the week (e.g., a 31-day month starting Sunday). Week starts on Monday
/// (matches Obsidian's daily-note plugin default).
///
/// Cells outside the anchor's month carry `in_current_month: false` so the
/// renderer can grey them out (spillover style) instead of showing blanks.
///
/// `anchor` is expected to be the first day of the target month.
fn build_month_cells(anchor: NaiveDate) -> Vec<MonthCell> {
    let anchor_month = anchor.month();
    let start_date = anchor - chrono::Days::new(anchor.weekday().num_days_from_monday() as u64);
    std::iter::successors(Some(start_date), |d| d.succ_opt())
        .take(42)
        .map(|date| MonthCell {
            date,
            in_current_month: date.month() == anchor_month,
        })
        .collect()
}

#[component]
fn CalendarDrawer(
    open: Signal<bool>,
    today: String,
    selected: String,
    words: usize,
    chars: usize,
    on_select: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let today_month_first =
        NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1).unwrap();

    let mut anchor = use_signal(|| today_month_first);
    // date -> `complete`. Presence in the map = the day has an entry.
    let mut day_stats = use_signal(HashMap::<String, bool>::new);
    let mut fetch_error = use_signal(|| None::<String>);

    // Fetch the visible month's stats whenever the drawer is open (fresh data on
    // each open) or the user pages to another month. Reads `open` + `anchor` so
    // it re-runs on either; gated on `open` so a never-opened drawer does no work.
    // Also reads `sync_epoch` so an open drawer refreshes its dots when a
    // background pull lands (sync_refresh) — e.g. a journal entry synced from
    // another device flips a day to complete without reopening the calendar.
    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    use_effect(move || {
        let _ = sync_epoch.read(); // subscribe: re-run on inbound sync
        if !*open.read() {
            return;
        }
        let a = *anchor.read();
        let first = NaiveDate::from_ymd_opt(a.year(), a.month(), 1).unwrap();
        let last_day = days_in_month(a.year(), a.month());
        let last = NaiveDate::from_ymd_opt(a.year(), a.month(), last_day).unwrap();
        let from_s = first.format("%Y-%m-%d").to_string();
        let to_s = last.format("%Y-%m-%d").to_string();

        fetch_error.set(None);
        spawn(async move {
            match bridge::invoke_list_journal_day_stats(&from_s, &to_s).await {
                Ok(stats) => {
                    day_stats.set(stats.into_iter().map(|s| (s.date, s.complete)).collect());
                }
                Err(e) => fetch_error.set(Some(e)),
            }
        });
    });

    let is_open = *open.read();
    let month_label = anchor.read().format("%B %Y").to_string();
    let cells = build_month_cells(*anchor.read());
    let word_label = if words == 1 { "word" } else { "words" };
    let char_label = if chars == 1 { "character" } else { "characters" };

    // Scrim + panel are always rendered (class-toggled) so the slide animates.
    let scrim_class = if is_open {
        "fixed inset-0 z-[140] bg-black/50 transition-opacity duration-200 opacity-100"
    } else {
        "fixed inset-0 z-[140] bg-black/50 transition-opacity duration-200 opacity-0 pointer-events-none"
    };
    let panel_base = "fixed inset-y-0 right-0 z-[150] w-72 max-w-[85vw] bg-obsidian-sidebar border-l border-white/5 flex flex-col overflow-y-auto transition-transform duration-200 ease-out";
    let panel_class = if is_open {
        format!("{panel_base} translate-x-0")
    } else {
        format!("{panel_base} translate-x-full")
    };

    rsx! {
        div {
            class: "{scrim_class}",
            "aria-hidden": "true",
            onclick: move |_| on_close.call(()),
        }
        aside {
            class: "{panel_class}",
            // Clear the status bar / gesture bar on Android via the inset vars.
            style: "padding-top: calc(1rem + var(--safe-area-inset-top)); padding-bottom: calc(1rem + var(--safe-area-inset-bottom));",

            // Drawer header: title + close
            div { class: "flex items-center justify-between px-4 pb-3 mb-1 border-b border-white/5",
                h2 { class: "text-sm font-bold text-obsidian-accent uppercase tracking-wider", "Calendar" }
                button {
                    class: "p-1 -mr-1 text-obsidian-text-muted hover:text-obsidian-text rounded hover:bg-white/5 transition-colors",
                    "aria-label": "Close calendar",
                    onclick: move |_| on_close.call(()),
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M6 18L18 6M6 6l12 12" }
                    }
                }
            }

            div { class: "px-3 py-2",
                // Month navigation header
                div { class: "flex items-center justify-between mb-3",
                    button {
                        class: "p-1.5 text-obsidian-text-muted hover:text-obsidian-text rounded hover:bg-white/5 transition-colors",
                        "aria-label": "Previous month",
                        onclick: move |_| {
                            let a = *anchor.read();
                            let (y, m) = if a.month() == 1 {
                                (a.year() - 1, 12)
                            } else {
                                (a.year(), a.month() - 1)
                            };
                            anchor.set(NaiveDate::from_ymd_opt(y, m, 1).unwrap());
                        },
                        svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                        }
                    }
                    h3 { class: "text-sm font-semibold text-obsidian-text", "{month_label}" }
                    button {
                        class: "p-1.5 text-obsidian-text-muted hover:text-obsidian-text rounded hover:bg-white/5 transition-colors",
                        "aria-label": "Next month",
                        onclick: move |_| {
                            let a = *anchor.read();
                            let (y, m) = if a.month() == 12 {
                                (a.year() + 1, 1)
                            } else {
                                (a.year(), a.month() + 1)
                            };
                            anchor.set(NaiveDate::from_ymd_opt(y, m, 1).unwrap());
                        },
                        svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M9 5l7 7-7 7" }
                        }
                    }
                }

                // Weekday header (Mon-first). Single letters keep the columns
                // legible in the narrow drawer.
                div { class: "grid grid-cols-7 gap-1 mb-1",
                    for (i , label) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
                        div {
                            key: "{i}",
                            class: "text-center text-[10px] font-semibold uppercase tracking-wider text-obsidian-text-muted py-1",
                            "{label}"
                        }
                    }
                }

                // Day grid
                div { class: "grid grid-cols-7 gap-1",
                    for cell in cells {
                        {
                            let date_str = cell.date.format("%Y-%m-%d").to_string();
                            let stats = day_stats.read();
                            let has_entry = stats.contains_key(&date_str);
                            let is_complete = stats.get(&date_str).copied().unwrap_or(false);
                            let is_today = date_str == today;
                            let is_selected = date_str == selected;
                            let classes = day_cell_class(
                                is_today,
                                is_selected,
                                has_entry,
                                cell.in_current_month,
                            );
                            let day_num = cell.date.day();
                            rsx! {
                                button {
                                    class: "{classes}",
                                    onclick: {
                                        let d = date_str.clone();
                                        move |_| on_select.call(d.clone())
                                    },
                                    div { class: "text-xs leading-none", "{day_num}" }
                                    {day_marker(has_entry, is_complete)}
                                }
                            }
                        }
                    }
                }

                if let Some(err) = &*fetch_error.read() {
                    div { class: "mt-3 p-2 bg-red-900/20 text-red-400 rounded border border-red-900/50 text-xs",
                        "{err}"
                    }
                }
            }

            // Footer: the viewed note's word/char count (Obsidian calendar
            // parity). `mt-auto` pins it to the bottom of the drawer column.
            div { class: "mt-auto px-4 pt-3 border-t border-white/5 flex items-center justify-between text-[11px] text-obsidian-text-muted",
                span { "{words} {word_label}" }
                span { "{chars} {char_label}" }
            }
        }
    }
}

/// The per-day activity marker under the day number: a check when the entry is
/// complete, a filled dot when there's an entry, else an empty spacer (keeps
/// every cell the same height so the grid doesn't jitter).
fn day_marker(has_entry: bool, is_complete: bool) -> Element {
    if is_complete {
        rsx! {
            svg {
                class: "w-2.5 h-2.5 mt-0.5 text-obsidian-accent",
                fill: "none",
                stroke: "currentColor",
                view_box: "0 0 24 24",
                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
            }
        }
    } else if has_entry {
        rsx! {
            div { class: "w-1 h-1 rounded-full bg-obsidian-accent mt-1" }
        }
    } else {
        rsx! {
            div { class: "w-1 h-1 mt-1" }
        }
    }
}

/// Number of days in a given calendar month.
fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

/// Tailwind class composition for a single calendar-day cell.
fn day_cell_class(
    is_today: bool,
    is_selected: bool,
    has_entry: bool,
    in_current_month: bool,
) -> String {
    let base = "aspect-square flex flex-col items-center justify-center rounded-md text-center transition-colors cursor-pointer";
    let text_class = if !in_current_month {
        "text-obsidian-text-muted/40"
    } else if is_today {
        "text-obsidian-accent font-bold"
    } else if has_entry {
        "text-obsidian-text font-medium"
    } else {
        "text-obsidian-text-muted"
    };
    let bg_class = if is_selected {
        "bg-obsidian-accent/20 border border-obsidian-accent/40"
    } else if is_today {
        "bg-obsidian-sidebar border border-obsidian-accent/30"
    } else {
        "hover:bg-white/5 border border-transparent"
    };
    format!("{base} {text_class} {bg_class}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks in the grid contract: 42 cells (6 rows × 7 cols), Monday-first.
    #[test]
    fn build_month_cells_returns_42_monday_first_cells() {
        // February 2026: Feb 1 falls on a Sunday.
        let anchor = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        let cells = build_month_cells(anchor);
        assert_eq!(cells.len(), 42, "always 6 full weeks");

        // First row should start on a Monday (Jan 26, 2026 is a Monday).
        assert_eq!(cells[0].date, NaiveDate::from_ymd_opt(2026, 1, 26).unwrap());
        assert!(!cells[0].in_current_month);

        // Feb 1 (Sunday) should be cell index 6.
        assert_eq!(cells[6].date, anchor);
        assert!(cells[6].in_current_month);
    }

    #[test]
    fn body_stats_counts_words_and_chars() {
        // Empty and whitespace-only bodies have zero words.
        assert_eq!(body_stats(""), (0, 0));
        assert_eq!(body_stats("   \n\t "), (0, 6));
        // Plain prose: whitespace-delimited words, all chars counted.
        assert_eq!(body_stats("hello world"), (2, 11));
        // Runs of whitespace between words collapse to one separator.
        assert_eq!(body_stats("one   two\nthree"), (3, 15));
        // Characters are Unicode scalar values, not bytes ("é" is one char).
        assert_eq!(body_stats("café"), (1, 4));
    }
}
