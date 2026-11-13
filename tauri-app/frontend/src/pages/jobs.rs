//! Settings → Jobs: every job on the contract and what its runs did.
//!
//! The section, each job and each job's run history start collapsed, so the
//! first view is one line per job. `docs/src/agents.md` § The job contract.

use dioxus::prelude::*;

use super::settings::{format_relative_time, health_badge};
use crate::bridge;
use crate::types::{JobRunView, JobView, JobsView};

/// Dollars from millionths, to the precision a day of small calls needs.
fn dollars(micros: u64) -> String {
    format!("${:.4}", micros as f64 / 1_000_000.0)
}

fn run_line(run: &JobRunView) -> String {
    let n = |k: &str| run.counts.get(k).copied().unwrap_or(0);
    let what = match run.outcome.get("kind").and_then(|k| k.as_str()) {
        Some("success") => format!("{} fetched · {} appended", n("fetched"), n("appended")),
        Some("needs_user") => format!(
            "needs you: {}",
            run.outcome
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("")
        ),
        _ => format!(
            "failed: {}",
            run.outcome
                .get("error")
                .and_then(|r| r.as_str())
                .unwrap_or("")
        ),
    };
    let spend = if run.spend.unpriced_calls > 0 {
        format!(
            "{} + {} unpriced",
            dollars(run.spend.cost_micros),
            run.spend.unpriced_calls
        )
    } else {
        dollars(run.spend.cost_micros)
    };
    format!("{what} · {} calls · {spend}", run.spend.calls)
}

#[component]
pub fn JobsSection() -> Element {
    let mut open = use_signal(|| false);
    let mut view: Signal<Option<Result<JobsView, String>>> = use_signal(|| None);

    let load = move || {
        spawn(async move {
            view.set(Some(bridge::invoke_list_jobs().await));
        });
    };

    let headline = match &*view.read() {
        None => "Background work and what each run did.".to_string(),
        Some(Err(e)) => format!("Couldn't load jobs: {e}"),
        Some(Ok(v)) => {
            let attention = v.jobs.iter().filter(|j| j.health != "healthy").count();
            format!(
                "{} on the contract, {} not yet{}",
                v.jobs.len(),
                v.pending.len(),
                if attention > 0 {
                    format!(" · {attention} need a look")
                } else {
                    String::new()
                }
            )
        }
    };

    rsx! {
        div { class: "mb-10 space-y-4",
            div { class: "border-b border-obsidian-border/5 pb-2 mb-4 flex items-center justify-between",
                h2 { class: "text-lg font-bold text-obsidian-text", "Jobs" }
                button {
                    class: "text-xs text-obsidian-text-muted hover:text-obsidian-accent hover:underline",
                    onclick: move |_| {
                        let now_open = !*open.read();
                        open.set(now_open);
                        if now_open {
                            load();
                        }
                    },
                    if *open.read() { "Hide" } else { "Show" }
                }
            }
            p { class: "text-sm text-obsidian-text-muted", "{headline}" }
            if *open.read() {
                if let Some(Ok(v)) = &*view.read() {
                    for job in v.jobs.iter().cloned() {
                        JobRow { key: "{job.name}", job }
                    }
                    if !v.pending.is_empty() {
                        div { class: "text-xs text-obsidian-text-muted pt-2",
                            "Not on the contract yet: "
                            {v.pending.iter().map(|p| p.title.clone()).collect::<Vec<_>>().join(", ")}
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn JobRow(job: JobView) -> Element {
    let mut expanded = use_signal(|| false);
    let mut history: Signal<Option<Result<Vec<JobRunView>, String>>> = use_signal(|| None);
    let (label, badge) = health_badge(&job.health);
    let last = job
        .latest
        .iter()
        .max_by(|a, b| a.ended_at.cmp(&b.ended_at))
        .cloned();
    let last_at = format_relative_time(last.as_ref().map(|r| r.ended_at.as_str()));
    let last_line = last
        .as_ref()
        .map(run_line)
        .unwrap_or_else(|| "nothing recorded".into());
    let summary = format!(
        "{} · {} runs in 24 h · {} not clean · {}",
        job.host,
        job.runs_24h,
        job.failures_24h,
        dollars(job.cost_micros_24h)
    );
    let name = job.name.clone();

    rsx! {
        div { class: "p-3 bg-obsidian-sidebar/60 border border-obsidian-border/5 rounded-lg space-y-2",
            button {
                class: "w-full flex items-center justify-between gap-3 text-left",
                onclick: move |_| {
                    let now = !*expanded.read();
                    expanded.set(now);
                    if now && history.read().is_none() {
                        let name = name.clone();
                        spawn(async move {
                            history.set(Some(bridge::invoke_list_job_runs(&name).await));
                        });
                    }
                },
                div { class: "min-w-0",
                    div { class: "text-sm font-semibold text-obsidian-text", "{job.title}" }
                    div { class: "text-xs text-obsidian-text-muted truncate", "Last run {last_at}: {last_line}" }
                    div { class: "text-xs text-obsidian-text-muted truncate", "{summary}" }
                }
                span { class: "shrink-0 text-[10px] font-bold uppercase tracking-widest px-2 py-0.5 rounded border {badge}", "{label}" }
            }
            if *expanded.read() {
                div { class: "text-xs text-obsidian-text-muted space-y-1 pt-1",
                    div { "Output: {job.output.lands_in} ({job.output.tier})" }
                    div { "Off switch: {job.off_switch}" }
                    div { "On failure: {job.failure.retry}" }
                    if let Some(model) = &job.model {
                        div {
                            "Models: "
                            {model.seats.iter().map(|s| format!("{} ({})", s.role, s.purpose)).collect::<Vec<_>>().join(", ")}
                            " · may {model.permission} · budget: {model.budget}"
                        }
                    }
                    match &*history.read() {
                        None => rsx! { div { "Loading runs…" } },
                        Some(Err(e)) => rsx! { div { "Couldn't load runs: {e}" } },
                        Some(Ok(runs)) => rsx! {
                            div { class: "pt-2 font-bold uppercase tracking-widest text-[10px]", "Last {runs.len()} runs" }
                            for run in runs.iter() {
                                div { key: "h-{run.id}", class: "truncate",
                                    span { class: "font-mono", "{run.subject}" }
                                    " · {format_relative_time(Some(&run.ended_at))} · {run_line(run)}"
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}
