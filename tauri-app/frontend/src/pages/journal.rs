use std::collections::HashSet;

use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::bridge;
use crate::components::editor::Editor;
use crate::journal_template;
use crate::timer::sleep_ms;
use crate::types::JournalEntryItem;
use crate::user_date::UserDate;

/// How long the editor must be quiet before an auto-save fires. Matches
/// Cycle 2's "1s local debounce" decision (project.md: Obsidian-equivalent).
const AUTOSAVE_DEBOUNCE_MS: i32 = 1000;

/// Second-level tabs inside the Journal feature.
///
/// `Today` shows the entry for whichever date is currently `selected_date`
/// (defaults to today; calendar clicks can redirect it). `Calendar` renders
/// the month grid with per-day entry dots.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JournalSubTab {
    Today,
    Calendar,
}

#[component]
pub fn JournalPage() -> Element {
    let mut sub_tab = use_signal(|| JournalSubTab::Today);
    let tz_signal: Signal<Tz> = use_context();
    // `&*signal.read()` is explicit on purpose: makes it clear we're
    // borrowing through a signal guard, not coercing the guard itself.
    #[allow(clippy::explicit_auto_deref)]
    let today = UserDate::today(&*tz_signal.read()).to_date_string();

    let mut selected_date = use_signal(|| today.clone());

    rsx! {
        div { class: "max-w-3xl mx-auto w-full",
            JournalSubNav { active: *sub_tab.read(), on_switch: move |t| sub_tab.set(t) }

            match *sub_tab.read() {
                JournalSubTab::Today => rsx! {
                    DayView {
                        key: "{selected_date.read()}",
                        date: selected_date.read().clone(),
                        today: today.clone(),
                        on_back_to_today: {
                            let today = today.clone();
                            move |_| selected_date.set(today.clone())
                        },
                    }
                },
                JournalSubTab::Calendar => rsx! {
                    CalendarView {
                        today: today.clone(),
                        selected: selected_date.read().clone(),
                        on_select: move |d: String| {
                            selected_date.set(d);
                            sub_tab.set(JournalSubTab::Today);
                        },
                    }
                },
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

// ---------------------------------------------------------------------------
// DayView — shows the entry for a specific date. Used for today + past days.
// ---------------------------------------------------------------------------

/// Day entry view, parameterized on date. Parent keys by `date`, so navigating
/// between days remounts this component and resets its load state cleanly.
#[component]
fn DayView(date: String, today: String, on_back_to_today: EventHandler<()>) -> Element {
    let mut entry = use_signal(|| None::<JournalEntryItem>);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut processing = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut save_status = use_signal(|| None::<String>);
    let mut llm_result = use_signal(|| None::<crate::types::LlmResult>);
    let mut llm_error = use_signal(|| None::<String>);
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

    let is_today_view = date == today;

    let date_for_load = date.clone();
    let _load = use_future(move || {
        let d = date_for_load.clone();
        async move {
            match bridge::invoke_get_journal_by_date(&d).await {
                Ok(Some(e)) => {
                    let raw = e.raw_text.clone();
                    last_saved_content.set(raw.clone());
                    content.set(raw);
                    entry.set(Some(e));
                    error_msg.set(None);
                }
                Ok(None) => {
                    // New entry: prime both signals with the default template so
                    // an immediate Save without keystrokes still persists it,
                    // and so auto-save doesn't treat the template-vs-empty
                    // diff as user input.
                    let template = journal_template::render(&d);
                    last_saved_content.set(template.clone());
                    content.set(template);
                    entry.set(None);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
            loading.set(false);
        }
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
                let result = if let Some(id) = jid {
                    bridge::invoke_update_journal_entry(&id, &snapshot)
                        .await
                        .map(|_| None)
                } else {
                    bridge::invoke_create_journal_entry(&date, &snapshot)
                        .await
                        .map(Some)
                };
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
                        save_status.set(Some(format!("Auto-save failed: {e}")));
                    }
                }
            });
        });
    }

    rsx! {
        div { class: "animate-in fade-in duration-200",
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
                        journal_template::render(&date)
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
                                journal_mode: true,
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

// ---------------------------------------------------------------------------
// CalendarView — month grid with per-day dots for dates that have entries.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct MonthCell {
    date: NaiveDate,
    in_current_month: bool,
}

/// Build the list of calendar cells for a month view.
///
/// The grid is always 5 rows × 7 cols (35 cells) so its height stays constant
/// as the user navigates months. Week starts on Monday (matches Obsidian's
/// daily-note plugin default).
///
/// Cells outside the anchor's month carry `in_current_month: false` so the
/// renderer can grey them out (spillover style) instead of showing blanks.
///
/// `anchor` is expected to be the first day of the target month.
fn build_month_cells(anchor: NaiveDate) -> Vec<MonthCell> {
    let anchor_month = anchor.month();
    let start_date = anchor - chrono::Days::new(anchor.weekday().num_days_from_monday() as u64);
    std::iter::successors(Some(start_date), |d| d.succ_opt())
        .take(35)
        .map(|date| MonthCell {
            date,
            in_current_month: date.month() == anchor_month,
        })
        .collect()
}

#[component]
fn CalendarView(today: String, selected: String, on_select: EventHandler<String>) -> Element {
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let today_month_first =
        NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1).unwrap();

    let mut anchor = use_signal(|| today_month_first);
    let mut dates_with_entries = use_signal(HashSet::<String>::new);
    let mut loading_dates = use_signal(|| true);
    let mut fetch_error = use_signal(|| None::<String>);

    use_effect(move || {
        let a = *anchor.read();
        let first = NaiveDate::from_ymd_opt(a.year(), a.month(), 1).unwrap();
        let last_day = days_in_month(a.year(), a.month());
        let last = NaiveDate::from_ymd_opt(a.year(), a.month(), last_day).unwrap();
        let from_s = first.format("%Y-%m-%d").to_string();
        let to_s = last.format("%Y-%m-%d").to_string();

        loading_dates.set(true);
        fetch_error.set(None);
        spawn(async move {
            match bridge::invoke_list_journal_dates(&from_s, &to_s).await {
                Ok(dates) => dates_with_entries.set(dates.into_iter().collect()),
                Err(e) => fetch_error.set(Some(e)),
            }
            loading_dates.set(false);
        });
    });

    let month_label = anchor.read().format("%B %Y").to_string();
    let cells = build_month_cells(*anchor.read());

    rsx! {
        div { class: "py-2 animate-in fade-in duration-200",
            // Month navigation header
            div { class: "flex items-center justify-between mb-4",
                button {
                    class: "p-2 text-obsidian-text-muted hover:text-obsidian-text rounded hover:bg-white/5 transition-colors",
                    onclick: move |_| {
                        let a = *anchor.read();
                        let (y, m) = if a.month() == 1 {
                            (a.year() - 1, 12)
                        } else {
                            (a.year(), a.month() - 1)
                        };
                        anchor.set(NaiveDate::from_ymd_opt(y, m, 1).unwrap());
                    },
                    "◀"
                }
                h2 { class: "text-lg font-semibold text-obsidian-text", "{month_label}" }
                button {
                    class: "p-2 text-obsidian-text-muted hover:text-obsidian-text rounded hover:bg-white/5 transition-colors",
                    onclick: move |_| {
                        let a = *anchor.read();
                        let (y, m) = if a.month() == 12 {
                            (a.year() + 1, 1)
                        } else {
                            (a.year(), a.month() + 1)
                        };
                        anchor.set(NaiveDate::from_ymd_opt(y, m, 1).unwrap());
                    },
                    "▶"
                }
            }

            // Weekday header (Mon-first)
            div { class: "grid grid-cols-7 gap-1 mb-2",
                for label in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
                    div { class: "text-center text-[10px] font-semibold uppercase tracking-wider text-obsidian-text-muted py-1",
                        "{label}"
                    }
                }
            }

            // Day grid
            div { class: "grid grid-cols-7 gap-1",
                for cell in cells {
                    {
                        let date_str = cell.date.format("%Y-%m-%d").to_string();
                        let has_entry = dates_with_entries.read().contains(&date_str);
                        let is_today = date_str == today;
                        let is_selected = date_str == selected;
                        let classes =
                            day_cell_class(is_today, is_selected, has_entry, cell.in_current_month);
                        let day_num = cell.date.day();
                        rsx! {
                            button {
                                class: "{classes}",
                                onclick: {
                                    let d = date_str.clone();
                                    move |_| on_select.call(d.clone())
                                },
                                div { class: "text-sm leading-none", "{day_num}" }
                                div {
                                    class: if has_entry {
                                        "w-1 h-1 rounded-full bg-obsidian-accent mt-1"
                                    } else {
                                        "w-1 h-1 mt-1"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(err) = &*fetch_error.read() {
                div { class: "mt-4 p-3 bg-red-900/20 text-red-400 rounded border border-red-900/50 text-sm",
                    "{err}"
                }
            }
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
        assert_eq!(cells.len(), 35, "always 5 full weeks");

        // First row should start on a Monday (Jan 26, 2026 is a Monday).
        assert_eq!(cells[0].date, NaiveDate::from_ymd_opt(2026, 1, 26).unwrap());
        assert!(!cells[0].in_current_month);

        // Feb 1 (Sunday) should be cell index 6.
        assert_eq!(cells[6].date, anchor);
        assert!(cells[6].in_current_month);
    }
}
