//! The read-only view beside chat (ruling 110): what is left of today, the rest of the week as
//! planned-against-free rows, then the month's goals. Plans are made and changed in chat, so a
//! tapped row opens chat about it, in the thread that day's plan came from when there is one.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::plan_card::{Agenda, DayLabels, Due, Fit};
use crate::components::primitives::PageHeader;
use crate::sync_refresh::use_sync_epoch;
use crate::types::{DueItem, PlanSlot, StartedBlock, TodayView, WeekRow};
use crate::{NavIntent, NavTarget, Tab};

/// Opens chat with `draft` in the composer, left for him to finish and send.
fn ask(thread_id: Option<String>, draft: String) {
    crate::request_nav(NavTarget {
        tab: Tab::Assistant,
        intent: Some(NavIntent::Chat { thread_id, draft }),
    });
}

/// The last Start tap's outcome, shown at the top of the view until dismissed. Provided by `App`.
#[derive(Clone, Copy)]
pub struct BlockStartNote(pub Signal<Option<Result<StartedBlock, String>>>);

#[component]
pub fn PlanPage() -> Element {
    let sync_epoch = use_sync_epoch();
    // Bumped each minute, so the now line and a running timer's minutes move, and after a
    // timer tap.
    let mut reread = use_signal(|| 0u32);
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(60_000).await;
            reread += 1;
        }
    });
    let view = use_resource(move || {
        let _ = sync_epoch.read();
        let _ = reread.read();
        async move { bridge::invoke_plan_view().await }
    });

    // Start taps from a reminder land here, and so does a refused view timer tap.
    let mut note = use_context::<BlockStartNote>().0;
    let body = match view.read().clone() {
        None => rsx! { p { class: "text-sm text-obsidian-text-muted", "Reading the day…" } },
        Some(Err(e)) => rsx! { p { class: "text-sm text-error", "Couldn't read the plan: {e}" } },
        Some(Ok(v)) => rsx! {
            Today {
                today: v.today.clone(),
                on_timer: move |(at, step): (i64, &'static str)| {
                    spawn(async move {
                        if let Err(e) = bridge::invoke_block_timer(at, step).await {
                            note.set(Some(Err(e)));
                        }
                        reread += 1;
                    });
                },
            }
            Horizon { title: v.week_title.clone(), note: String::new() }
            div { class: "flex flex-col gap-1.5",
                for row in v.week.iter() {
                    WeekLine { key: "{row.day}", row: row.clone() }
                }
            }
            Horizon { title: v.goals_title.clone(), note: String::new() }
            if v.goals.is_empty() {
                p { class: "text-xs text-obsidian-text-muted",
                    "None set yet. The monthly review asks on the 1st."
                }
            } else {
                ul { class: "flex flex-col gap-1.5 text-sm text-obsidian-text",
                    for goal in v.goals.iter().cloned() {
                        li {
                            key: "{goal}",
                            class: "cursor-pointer active:opacity-60",
                            onclick: {
                                let draft = format!("About my goal \"{goal}\": ");
                                move |_| ask(None, draft.clone())
                            },
                            "{goal}"
                        }
                    }
                }
            }
        },
    };

    let started = match note.read().clone() {
        None => rsx! {},
        Some(outcome) => {
            let (class, text) = match outcome {
                Ok(b) => ("text-success", format!("Started {} · {}", b.title, b.when)),
                Err(e) => ("text-obsidian-text-muted", e),
            };
            rsx! {
                div { class: "flex justify-between items-baseline gap-2 text-sm mb-3",
                    span { class, "{text}" }
                    button {
                        class: "text-xs text-obsidian-text-muted",
                        "aria-label": "Dismiss",
                        onclick: move |_| note.set(None),
                        "×"
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "animate-in fade-in duration-300 max-w-2xl",
            PageHeader { title: "Plan" }
            {started}
            {body}
        }
    }
}

/// A timescale's heading, with its one-line summary on the right.
#[component]
fn Horizon(title: String, note: String) -> Element {
    rsx! {
        div { class: "flex justify-between items-baseline gap-2 border-b border-obsidian-border/20 pb-1 mt-6 mb-2",
            span { class: "text-sm font-medium text-obsidian-text", "{title}" }
            span { class: "text-xs text-obsidian-text-muted", "{note}" }
        }
    }
}

#[component]
fn Today(today: TodayView, on_timer: EventHandler<(i64, &'static str)>) -> Element {
    let t = today;
    let mut tail: Vec<String> = Vec::new();
    match t.blocks_done {
        0 => {}
        1 => tail.push("1 earlier block done".into()),
        n => tail.push(format!("{n} earlier blocks done")),
    }
    if !t.untimed.is_empty() {
        let names: Vec<&str> = t.untimed.iter().map(|u| u.title.as_str()).collect();
        tail.push(format!("any time: {}", names.join(", ")));
    }
    let tail = tail.join(" · ");
    let slot_thread = t.thread_id.clone();
    let due_thread = t.thread_id.clone();
    rsx! {
        div { class: "flex justify-between items-baseline gap-2 border-b border-obsidian-border/20 pb-1 mb-2",
            span { class: "text-sm font-medium text-obsidian-text", "Today" }
            if let Some(f) = t.fit.clone() {
                Fit { fit: f }
            } else if let Some(free) = t.free.as_ref() {
                span { class: "text-xs text-obsidian-text-muted", "{free}" }
            }
        }
        div { class: "flex flex-col gap-2",
            DayLabels { all_day: t.day.all_day.clone(), notices: t.day.notices.clone() }
            if t.pending {
                div { class: "text-xs text-obsidian-accent", "A plan for today is waiting in chat." }
            }
            if t.slots.is_empty() {
                div { class: "text-xs text-obsidian-text-muted", "Nothing else is timed today." }
            } else {
                Agenda {
                    slots: t.slots.clone(),
                    now_at: Some(t.now_at),
                    now: Some(t.now.clone()),
                    on_tap: move |s: PlanSlot| {
                        ask(slot_thread.clone(), format!("About {}–{} {}: ", s.from, s.to, s.title))
                    },
                    on_timer,
                }
            }
            if !tail.is_empty() {
                div { class: "text-xs text-obsidian-text-muted", "{tail}" }
            }
            if !t.due.is_empty() {
                Due {
                    items: t.due.clone(),
                    on_tap: move |d: DueItem| {
                        ask(due_thread.clone(), format!("About \"{}\", due {}: ", d.title, d.when))
                    },
                }
            }
        }
    }
}

/// One later day: how much is planned against the free time, and what is on it.
#[component]
fn WeekLine(row: WeekRow) -> Element {
    let (width, over) = match row.planned_min {
        Some(p) if row.free_min > 0 => (((p * 100) / row.free_min).min(100), p > row.free_min),
        Some(p) => (if p > 0 { 100 } else { 0 }, p > 0),
        None => (0, false),
    };
    let fill = if over {
        "bg-warn"
    } else {
        "bg-obsidian-accent/70"
    };
    let parts: Vec<(String, bool)> = row
        .due
        .iter()
        .map(|d| (format!("{d} due"), true))
        .chain(row.all_day.iter().map(|a| (a.clone(), false)))
        .collect();
    let draft = match row.date.as_str() {
        "" => format!("About {}: ", row.day),
        date => format!("About {} ({date}): ", row.day),
    };
    let thread = row.thread_id.clone();
    rsx! {
        div {
            class: "flex items-center gap-2 text-xs cursor-pointer active:opacity-60",
            onclick: move |_| ask(thread.clone(), draft.clone()),
            span { class: "w-8 shrink-0 text-obsidian-text", "{row.day}" }
            div { class: "w-16 h-1.5 shrink-0 rounded bg-obsidian-border/20 overflow-hidden",
                div { class: "h-full {fill}", style: "width: {width}%" }
            }
            span { class: "w-24 shrink-0 text-obsidian-text-muted tabular-nums", "{row.hours}" }
            span { class: "min-w-0 flex-1 truncate text-obsidian-text-muted",
                for (i, (text, warn)) in parts.iter().enumerate() {
                    span { key: "{i}",
                        if i > 0 {
                            " · "
                        }
                        span { class: if *warn { "text-warn" } else { "" }, "{text}" }
                    }
                }
            }
        }
    }
}
