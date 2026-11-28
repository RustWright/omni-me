//! The day plan in chat (ruling 117) and the timeline it shares with the Plan view (110).
//! Everything shown is worked out in core's `planning::view`; this only lays it out.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::primitives::{Button, ButtonSize, ButtonVariant};
use crate::sync_refresh::{bump_sync_epoch, use_sync_epoch};
use crate::types::{
    AssistantProposal, DueItem, FitLine, PlanSlot, PlanStatus, SlotKind, SlotTimer, TimerShown,
    UntimedItem,
};

/// The decision reason core reads as "Skip today" (`planning::plan::REASON_SKIPPED`).
const REASON_SKIPPED: &str = "skip today";

/// A `plan.set` proposal. Until its card loads, or if it cannot, the plain proposal card stands
/// in, so a plan is never undecidable.
#[component]
pub fn PlanProposalCard(proposal: AssistantProposal, on_decided: EventHandler<()>) -> Element {
    let sync_epoch = use_sync_epoch();
    let busy = use_signal(|| false);
    let error_msg = use_signal(|| None::<String>);
    let id = use_signal(|| proposal.proposal_id.clone());
    // Reread on every sync, which is how a re-plan reaches this card and supersedes it.
    let card = use_resource(move || {
        let _ = sync_epoch.read();
        let id = id.read().clone();
        async move { bridge::invoke_plan_card(&id).await }
    });

    fn decide(
        mut busy: Signal<bool>,
        mut error_msg: Signal<Option<String>>,
        sync_epoch: Signal<u64>,
        id: String,
        approve: bool,
        on_decided: EventHandler<()>,
    ) {
        busy.set(true);
        spawn(async move {
            let reason = (!approve).then_some(REASON_SKIPPED);
            match bridge::invoke_decide_assistant_proposal(&id, approve, reason).await {
                Ok(_) => {
                    error_msg.set(None);
                    bump_sync_epoch(sync_epoch);
                    on_decided.call(());
                }
                Err(e) => error_msg.set(Some(e)),
            }
            busy.set(false);
        });
    }

    let loaded = card.read().clone();
    let c = match loaded {
        Some(Ok(Some(c))) => c,
        None => {
            return rsx! {
                div { class: "rounded-md border border-obsidian-border/20 px-3 py-2 text-xs text-obsidian-text-muted",
                    "Reading the plan…"
                }
            };
        }
        // Not a plan core can read yet, or an error: the plain card decides it.
        _ => {
            return rsx! {
                crate::components::proposal_card::GenericProposalCard { proposal, on_decided }
            };
        }
    };

    if c.status == PlanStatus::Superseded {
        let replaced = c
            .replaced_at
            .as_deref()
            .map(|t| format!("replaced by {t}"))
            .unwrap_or_else(|| "replaced".into());
        return rsx! {
            div { class: "rounded-md border border-obsidian-border/20 px-3 py-2 flex justify-between text-xs text-obsidian-text-muted",
                span { "Plan for {c.day.weekday}, {c.drafted}" }
                span { "{replaced}" }
            }
        };
    }

    let right = match c.status {
        PlanStatus::Proposed => c
            .until
            .as_deref()
            .map(|t| format!("until {t}"))
            .unwrap_or_default(),
        PlanStatus::Accepted => "accepted".into(),
        PlanStatus::Skipped => "skipped".into(),
        PlanStatus::Lapsed => "lapsed".into(),
        _ => String::new(),
    };

    rsx! {
        div { class: "rounded-md border border-obsidian-border/20 bg-obsidian-sidebar px-3 py-3 flex flex-col gap-2",
            div { class: "flex justify-between items-baseline",
                span { class: "text-obsidian-text text-sm font-medium", "Plan for {c.day.weekday}" }
                span { class: "text-obsidian-text-muted text-xs", "{right}" }
            }
            if let Some(w) = c.day.window.as_ref() {
                div { class: "text-obsidian-text-muted text-xs", "{w}" }
            }
            DayLabels { all_day: c.day.all_day.clone(), notices: c.day.notices.clone() }
            if let Some(f) = c.fit.clone() {
                Fit { fit: f }
            }
            if !c.slots.is_empty() {
                Section { title: "Blocks" }
                Agenda { slots: c.slots.clone(), now_at: None, now: None }
            }
            if !c.untimed.is_empty() {
                Section { title: "Today, any time" }
                Untimed { items: c.untimed.clone() }
            }
            if !c.coming_due.is_empty() {
                Section { title: "Coming due" }
                Due { items: c.coming_due.clone() }
            }
            for line in c.unread.iter() {
                div { key: "{line}", class: "text-obsidian-text-muted text-xs", "{line}" }
            }
            if let Some(e) = error_msg.read().clone() {
                div { class: "text-error text-xs", "{e}" }
            }
            match c.status {
                PlanStatus::Proposed => rsx! {
                    div { class: "flex flex-wrap gap-2 pt-1 items-center",
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Sm,
                            disabled: *busy.read(),
                            onclick: move |_| decide(busy, error_msg, sync_epoch, id.read().clone(), true, on_decided),
                            "Accept plan"
                        }
                        Button {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            disabled: *busy.read(),
                            onclick: move |_| decide(busy, error_msg, sync_epoch, id.read().clone(), false, on_decided),
                            "Skip today"
                        }
                        span { class: "text-obsidian-text-muted text-xs", "or tell me what to change" }
                    }
                },
                PlanStatus::Lapsed => rsx! {
                    div { class: "text-obsidian-text-muted text-xs",
                        "It lapsed when its first block was due. Ask for a fresh plan any time."
                    }
                },
                _ => rsx! {},
            }
        }
    }
}

#[component]
pub fn Section(title: &'static str) -> Element {
    rsx! {
        div { class: "text-obsidian-text-muted text-[11px] uppercase tracking-wide pt-1", "{title}" }
    }
}

/// All-day labels on one line, then a line per notice.
#[component]
pub fn DayLabels(all_day: Vec<String>, notices: Vec<String>) -> Element {
    rsx! {
        if !all_day.is_empty() {
            div { class: "text-obsidian-text-muted text-xs", "All day: {all_day.join(\", \")}" }
        }
        for n in notices.iter() {
            div { key: "{n}", class: "text-warn text-xs", "{n}" }
        }
    }
}

/// The one coloured line (117): amber over, green within. Never blocks anything.
#[component]
pub fn Fit(fit: FitLine) -> Element {
    let colour = if fit.over {
        "text-warn"
    } else {
        "text-success"
    };
    rsx! {
        div { class: "text-xs {colour}", "{fit.text}" }
    }
}

/// Blocks solid, calendar events and travel greyed, with a now line before slot `now_at`.
#[component]
pub fn Agenda(
    slots: Vec<PlanSlot>,
    now_at: Option<usize>,
    now: Option<String>,
    /// Set by the Plan view, where a tapped row opens chat about it; the chat card leaves it.
    on_tap: Option<EventHandler<PlanSlot>>,
    /// The Plan view's timer controls (ruling 93): the block's start and `start`, `pause` or
    /// `stop`. The view's one exception to read-only.
    on_timer: Option<EventHandler<(i64, &'static str)>>,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-1",
            for (i, s) in slots.iter().enumerate() {
                div { key: "{s.from}-{s.title}", class: "flex flex-col gap-1",
                    if now_at == Some(i) {
                        NowLine { now: now.clone().unwrap_or_default() }
                    }
                    SlotRow { slot: s.clone(), on_tap, on_timer }
                }
            }
            if now_at.is_some_and(|n| n >= slots.len()) {
                NowLine { now: now.clone().unwrap_or_default() }
            }
        }
    }
}

#[component]
fn NowLine(now: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2",
            span { class: "w-[5.5rem] shrink-0 text-right text-[11px] text-obsidian-accent", "{now}" }
            div { class: "flex-1 border-t border-obsidian-accent/60" }
        }
    }
}

#[component]
fn SlotRow(
    slot: PlanSlot,
    on_tap: Option<EventHandler<PlanSlot>>,
    on_timer: Option<EventHandler<(i64, &'static str)>>,
) -> Element {
    let tap = tap_class(on_tap.is_some());
    let tapped = slot.clone();
    let (box_class, sub) = match slot.kind {
        SlotKind::Block => (
            "bg-obsidian-accent/10 border-l-2 border-obsidian-accent text-obsidian-text",
            None,
        ),
        SlotKind::Travel => (
            "bg-obsidian-border/10 border-l-2 border-obsidian-border/30 text-obsidian-text-muted",
            None,
        ),
        _ => (
            "bg-obsidian-border/10 border-l-2 border-obsidian-border/30 text-obsidian-text-muted",
            Some(match &slot.place {
                Some(p) => format!("calendar · {p}"),
                None => "calendar".to_string(),
            }),
        ),
    };
    rsx! {
        div {
            class: "flex gap-2 items-start {tap}",
            onclick: move |_| {
                if let Some(h) = on_tap {
                    h.call(tapped.clone());
                }
            },
            span { class: "w-[5.5rem] shrink-0 text-right text-xs text-obsidian-text-muted tabular-nums pt-1",
                "{slot.from}–{slot.to}"
            }
            div { class: "flex-1 min-w-0 rounded px-2 py-1 text-xs {box_class}",
                div { class: "flex gap-2 items-baseline",
                    span { class: "min-w-0 break-words", "{slot.title}" }
                    if slot.moved {
                        span { class: "text-[10px] text-obsidian-accent shrink-0", "moved" }
                    }
                }
                if let Some(s) = sub {
                    div { class: "text-[11px] opacity-80", "{s}" }
                }
                if let (Some(t), Some(h)) = (slot.timer.clone(), on_timer) {
                    TimerControls { timer: t, on_timer: h }
                }
            }
        }
    }
}

/// The running block's line and its buttons (ruling 93, `design/0102/` C).
#[component]
fn TimerControls(timer: SlotTimer, on_timer: EventHandler<(i64, &'static str)>) -> Element {
    let at = timer.block_start;
    let buttons: &[(&str, &'static str)] = match timer.state {
        TimerShown::Ready => &[("Start", "start")],
        TimerShown::Running => &[("Pause", "pause"), ("Stop", "stop")],
        TimerShown::Paused => &[("Resume", "start"), ("Stop", "stop")],
        _ => &[],
    };
    rsx! {
        if let Some(line) = timer.line.clone() {
            div { class: "text-[11px] opacity-80 tabular-nums", "{line}" }
        }
        if !buttons.is_empty() {
            div { class: "flex gap-2 mt-1",
                for (label, step) in buttons.iter().copied() {
                    button {
                        key: "{step}",
                        r#type: "button",
                        class: "px-2 py-0.5 rounded border border-obsidian-accent/40 text-[11px] text-obsidian-accent",
                        // A tap on the row opens chat; a tap here must not.
                        onclick: move |e| {
                            e.stop_propagation();
                            on_timer.call((at, step));
                        },
                        "{label}"
                    }
                }
            }
        }
    }
}

#[component]
pub fn Untimed(items: Vec<UntimedItem>) -> Element {
    rsx! {
        ul { class: "flex flex-col gap-0.5",
            for u in items.iter() {
                li { key: "{u.title}", class: "flex justify-between gap-2 text-xs text-obsidian-text",
                    span { class: "min-w-0", "{u.title}" }
                    span { class: "text-obsidian-text-muted shrink-0", "{u.minutes} min" }
                }
            }
        }
    }
}

#[component]
pub fn Due(items: Vec<DueItem>, on_tap: Option<EventHandler<DueItem>>) -> Element {
    let tap = tap_class(on_tap.is_some());
    rsx! {
        ul { class: "flex flex-col gap-0.5",
            for d in items.iter().cloned() {
                li {
                    key: "{d.title}",
                    class: "flex justify-between gap-2 text-xs {tap}",
                    onclick: {
                        let d = d.clone();
                        move |_| {
                            if let Some(h) = on_tap {
                                h.call(d.clone());
                            }
                        }
                    },
                    span { class: if d.overdue { "min-w-0 text-warn" } else { "min-w-0 text-obsidian-text" }, "{d.title}" }
                    span { class: "text-obsidian-text-muted shrink-0", "{d.when}" }
                }
            }
        }
    }
}

/// A row that opens chat when tapped looks pressable; one that does not, does not.
fn tap_class(tappable: bool) -> &'static str {
    if tappable {
        "cursor-pointer active:opacity-60"
    } else {
        ""
    }
}
