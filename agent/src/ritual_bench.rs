//! `--bench-rituals`: which seat should answer a ritual's opening turn ([0124]).
//!
//! Runs the evening shutdown, weekly plan and monthly review prompts through the path the
//! responder takes (Planning brief fixed, the same retrievers) on three arms: the interactive
//! seat, the batch seat, and the batch seat with reasoning off. Nothing is written: proposals
//! are read from the trace. Scored in code where the prompt makes a checkable demand; every
//! answer is printed in full after the scorecard, for reading.

use std::time::Duration;

use omni_me_core::assistant::specialist::Specialist;
use omni_me_core::assistant::{Outcome, Retrievers, Session, StopReason, proposals};
use omni_me_core::config::{ConfigKey, ResolvedConfig};
use omni_me_core::db::Database;
use omni_me_core::llm::LlmClient;
use omni_me_core::planning::rituals;

const REPEATS_ENV: &str = "OMNI_BENCH_RITUAL_REPEATS";

/// Most items the weekly plan is told to ask about.
const WEEKLY_MAX: usize = 10;

pub struct Arm<'a> {
    pub name: &'static str,
    pub llm: &'a dyn LlmClient,
    /// `None` leaves the provider's default.
    pub thinking: Option<bool>,
}

/// What one run did, reduced to what is compared.
struct Run {
    arm: &'static str,
    ritual: &'static str,
    answered: bool,
    failure: Option<String>,
    elapsed: Duration,
    turns: usize,
    prompt_tokens: u32,
    completion_tokens: u32,
    reasoning_tokens: u32,
    /// Proposals outside the ritual's own actions.
    wrong_actions: Vec<String>,
    /// Lookups for `plan` or `goal`, which the brief already states.
    wasted_lookups: usize,
    /// Weekly: items named out of those it should ask about. Monthly: last month named, 1 or 0.
    covered: Option<(usize, usize)>,
    answer: String,
}

/// The actions each ritual may propose; anything else is outside its question.
fn allowed(ritual: &str) -> &'static [&'static str] {
    match ritual {
        rituals::EVENING | rituals::WEEKLY => &["task.create", "task.update"],
        rituals::MONTHLY => &["goal.set"],
        _ => &[],
    }
}

/// A lookup the brief makes unnecessary: listing, describing or searching for plans or goals.
fn wasted(outcome: &Outcome) -> usize {
    outcome
        .trace
        .iter()
        .filter(|t| {
            matches!(
                t.verb.as_deref(),
                Some("list" | "describe_type" | "search" | "list_types")
            )
        })
        .filter(|t| {
            let args = t.arguments.to_string().to_lowercase();
            args.contains("\"plan") || args.contains("\"goal")
        })
        .count()
}

/// What the weekly plan should ask about: open tasks and everything waiting, at most ten.
async fn weekly_items(db: &Database, config: &ResolvedConfig) -> Vec<String> {
    let today = chrono::Utc::now()
        .with_timezone(&config.user_zone())
        .date_naive();
    let mut items: Vec<String> = Vec::new();
    if let Ok(mut resp) = db
        .query("SELECT VALUE title FROM tasks WHERE status = 'todo' ORDER BY created_at ASC")
        .await
    {
        let titles: Vec<String> = resp.take(0).unwrap_or_default();
        items.extend(titles);
    }
    if let Ok(w) = omni_me_core::tasks::queries::waiting(
        db,
        today,
        config.int_of(ConfigKey::PlanningIndexStaleDays),
        config.int_of(ConfigKey::PlanningTaskStaleDays),
    )
    .await
    {
        items.extend(w.hub.into_iter().map(|h| h.title));
        items.extend(w.deadlines.into_iter().map(|d| d.title));
    }
    items.dedup();
    items
}

fn coverage(answer: &str, items: &[String]) -> (usize, usize) {
    let lower = answer.to_lowercase();
    let named = items
        .iter()
        .filter(|i| lower.contains(&i.to_lowercase()))
        .count();
    (named.min(WEEKLY_MAX), items.len().min(WEEKLY_MAX))
}

fn last_month_named(answer: &str, config: &ResolvedConfig) -> bool {
    use chrono::Datelike;
    let today = chrono::Utc::now()
        .with_timezone(&config.user_zone())
        .date_naive();
    let last = today.with_day(1).unwrap_or(today) - chrono::Duration::days(1);
    let lower = answer.to_lowercase();
    lower.contains(&last.format("%B").to_string().to_lowercase())
        || lower.contains(&last.format("%Y-%m").to_string())
}

pub async fn run(
    db: &Database,
    config: &ResolvedConfig,
    arms: &[Arm<'_>],
    retrievers: Retrievers<'_>,
) {
    let repeats = crate::bench::repeats(REPEATS_ENV);
    let prompts: Vec<_> = rituals::prompts(config);
    if prompts.iter().any(|p| !p.enabled) {
        eprintln!("cannot bench: planning or the LLM feature is off");
        return;
    }
    let items = weekly_items(db, config).await;
    println!(
        "rituals × {} arms × {repeats} runs; the weekly should ask about {} of {} items",
        arms.len(),
        items.len().min(WEEKLY_MAX),
        items.len()
    );
    for a in arms {
        println!(
            "  arm {:<16} model {}  thinking {:?}",
            a.name,
            a.llm.model_name(),
            a.thinking
        );
    }
    println!();

    let mut runs: Vec<Run> = Vec::new();
    // Interleaved, so a slow spell at the provider lands on every arm rather than one.
    for rep in 0..repeats {
        for p in &prompts {
            for a in arms {
                let mut session = match Session::new(db, config, a.llm) {
                    Ok(s) => s.with_specialist(Specialist::Planning),
                    Err(e) => {
                        eprintln!("cannot bench: {e}");
                        return;
                    }
                };
                if let Some(on) = a.thinking {
                    session = session.with_thinking(on);
                }
                if let Some(s) = retrievers.semantic {
                    session = session.with_semantic_search(s);
                }
                if let Some(r) = retrievers.reranker {
                    session = session.with_reranker(r);
                }
                let outcome = session.ask(&p.text).await;
                let answer = outcome.answer.clone().unwrap_or_default();
                let wrong_actions = proposals(config, &outcome)
                    .into_iter()
                    .map(|x| x.action)
                    .filter(|act| !allowed(p.name).contains(&act.as_str()))
                    .collect();
                let covered = match p.name {
                    rituals::WEEKLY => Some(coverage(&answer, &items)),
                    rituals::MONTHLY => Some((usize::from(last_month_named(&answer, config)), 1)),
                    _ => None,
                };
                let r = Run {
                    arm: a.name,
                    ritual: p.name,
                    answered: matches!(outcome.stopped, StopReason::Answered),
                    failure: match &outcome.stopped {
                        StopReason::Answered => None,
                        other => Some(format!("{other:?}")),
                    },
                    elapsed: outcome.elapsed,
                    turns: outcome.trace.len(),
                    prompt_tokens: outcome.usage.prompt_tokens,
                    completion_tokens: outcome.usage.completion_tokens,
                    reasoning_tokens: outcome.usage.reasoning_tokens,
                    wrong_actions,
                    wasted_lookups: wasted(&outcome),
                    covered,
                    answer,
                };
                println!(
                    "  rep {rep} {:<17} {:<16} {:>5.0}s  turns {:>2}  tokens {:>6}/{:>5} (reasoning {:>5})  wrong {:?}  wasted {}  covered {}  {}",
                    r.ritual,
                    r.arm,
                    r.elapsed.as_secs_f64(),
                    r.turns,
                    r.prompt_tokens,
                    r.completion_tokens,
                    r.reasoning_tokens,
                    r.wrong_actions,
                    r.wasted_lookups,
                    r.covered
                        .map_or("-".to_string(), |(n, of)| format!("{n}/{of}")),
                    r.failure.as_deref().unwrap_or("answered"),
                );
                runs.push(r);
            }
        }
    }

    println!("\nper arm, all rituals:");
    for a in arms {
        let mine: Vec<&Run> = runs.iter().filter(|r| r.arm == a.name).collect();
        let answered = mine.iter().filter(|r| r.answered).count();
        let mut secs: Vec<f64> = mine.iter().map(|r| r.elapsed.as_secs_f64()).collect();
        secs.sort_by(|x, y| x.total_cmp(y));
        let median = secs.get(secs.len() / 2).copied().unwrap_or(0.0);
        let max = secs.last().copied().unwrap_or(0.0);
        let tokens: u64 = mine
            .iter()
            .map(|r| u64::from(r.prompt_tokens) + u64::from(r.completion_tokens))
            .sum();
        let wrong: usize = mine.iter().map(|r| r.wrong_actions.len()).sum();
        let wasted: usize = mine.iter().map(|r| r.wasted_lookups).sum();
        let (named, of) = mine
            .iter()
            .filter_map(|r| r.covered)
            .fold((0, 0), |(n, o), (a, b)| (n + a, o + b));
        println!(
            "  {:<16} answered {answered}/{}  median {median:.0}s  max {max:.0}s  tokens {tokens}  wrong actions {wrong}  wasted lookups {wasted}  covered {named}/{of}",
            a.name,
            mine.len(),
        );
    }

    println!("\n## Answers");
    for r in &runs {
        println!(
            "\n### {} · {} · {:.0}s\n{}",
            r.ritual,
            r.arm,
            r.elapsed.as_secs_f64(),
            if r.answer.is_empty() {
                r.failure.as_deref().unwrap_or("(empty)")
            } else {
                &r.answer
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_counts_named_items_and_caps_at_ten() {
        let items: Vec<String> = (0..12).map(|i| format!("Task {i:02}")).collect();
        let answer = "1. task 00\n2. Task 01\n3. Task 11";
        assert_eq!(coverage(answer, &items), (3, 10));
    }

    #[test]
    fn each_ritual_has_its_own_actions() {
        assert!(allowed(rituals::MONTHLY).contains(&"goal.set"));
        assert!(!allowed(rituals::WEEKLY).contains(&"goal.set"));
        assert!(allowed(rituals::EVENING).contains(&"task.create"));
    }
}
