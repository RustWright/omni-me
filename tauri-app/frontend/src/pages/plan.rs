//! The read-only view beside chat (ruling 110): what is left of today, then the rest of the week
//! as planned-against-free rows. Read-only: plans are made and changed in chat. The month's
//! goals join when the monthly review produces them.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::plan_card::{Agenda, DayLabels, Due, Fit};
use crate::components::primitives::PageHeader;
use crate::sync_refresh::use_sync_epoch;
use crate::types::{TodayView, WeekRow};

#[component]
pub fn PlanPage() -> Element {
    let sync_epoch = use_sync_epoch();
    let view = use_resource(move || {
        let _ = sync_epoch.read();
        async move { bridge::invoke_plan_view().await }
    });

    let body = match view.read().clone() {
        None => rsx! { p { class: "text-sm text-obsidian-text-muted", "Reading the day…" } },
        Some(Err(e)) => rsx! { p { class: "text-sm text-error", "Couldn't read the plan: {e}" } },
        Some(Ok(v)) => rsx! {
            Today { today: v.today.clone() }
            Horizon { title: v.week_title.clone(), note: String::new() }
            div { class: "flex flex-col gap-1.5",
                for row in v.week.iter() {
                    WeekLine { key: "{row.day}", row: row.clone() }
                }
            }
        },
    };

    rsx! {
        div { class: "animate-in fade-in duration-300 max-w-2xl",
            PageHeader { title: "Plan" }
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
fn Today(today: TodayView) -> Element {
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
                Agenda { slots: t.slots.clone(), now_at: Some(t.now_at), now: Some(t.now.clone()) }
            }
            if !tail.is_empty() {
                div { class: "text-xs text-obsidian-text-muted", "{tail}" }
            }
            if !t.due.is_empty() {
                Due { items: t.due.clone() }
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
    rsx! {
        div { class: "flex items-center gap-2 text-xs",
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
