//! The evening shutdown, the weekly plan and the monthly review (ruling 98), and the lapse rule
//! every ritual shares (ruling 99). Rationale: `docs/src/planning.md` § Rituals.
//!
//! Each is a scheduled prompt the agent raises at the summary hour and answers under the planning
//! brief. None pushes on its own: each rides the 19:00 summary as a line built here.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use surrealdb::types::SurrealValue;

use super::goals;
use super::plan::{self, REASON_LAPSED, REASON_SUPERSEDED};
use super::ritual::MORNING_PLAN;
use crate::assistant::scheduled::{Cadence, ScheduledPrompt};
use crate::config::{ConfigKey, Feature, ResolvedConfig};
use crate::db::Database;
use crate::jobs::Tier;

/// Never rename these: a prompt's history is found by its name.
pub const EVENING: &str = "evening_shutdown";
pub const WEEKLY: &str = "weekly_plan";
pub const MONTHLY: &str = "monthly_review";

/// The weekly plan's day. Its evening has no shutdown of its own: the weekly plan covers it.
pub const WEEKLY_DAY: Weekday = Weekday::Sun;

/// Runs skipped in a row before a ritual pauses itself: ruling 99 (RULINGS.md).
pub const LAPSE_AFTER: usize = 5;

const EVENING_PROMPT: &str = "Run my evening shutdown. Say in one line what today's accepted \
plan held. For each block or small thing in it that I have not said is done, ask in one list \
whether it is done, moves to another day, or is dropped. Never move anything without my answer: \
to move one, propose `task.create` with the new `scheduled` day, or `task.update` if it is \
already a task. Do not plan tomorrow; that happens in the morning. End with one line on the free \
time tomorrow has.";

const WEEKLY_PROMPT: &str = "Run my weekly plan. Say in two lines how last week went against \
its plans. Then ask about at most 10 open tasks that need a decision, oldest first: a day to do \
it, keep for later, or drop. The rest wait for next week; say how many. Never decide one for me. \
End with one line on the free time this coming week has.";

const MONTHLY_PROMPT: &str = "Run my monthly review. Show the goals set for last month, if \
any, and ask in one line how each went. Then ask what matters this month and propose the \
month's goals with `goal.set`, in my words. For a goal with no progress last month, ask what got \
in the way and what I would do when it does again.";

/// The four rituals the planning specialist answers, morning plan included.
pub fn is_ritual(prompt_name: &str) -> bool {
    matches!(prompt_name, MORNING_PLAN | EVENING | WEEKLY | MONTHLY)
}

/// The evening shutdown, the weekly plan and the monthly review as scheduled prompts.
pub fn prompts(config: &ResolvedConfig) -> Vec<ScheduledPrompt> {
    let enabled = config.enabled(Feature::Planning) && config.enabled(Feature::Llm);
    let hour = config.int_of(ConfigKey::NotifySummaryHour).clamp(0, 23) as u32;
    let prompt = |name, title, text: &str, cadence, keep_days| ScheduledPrompt {
        name,
        title,
        text: text.to_string(),
        enabled,
        cadence,
        hour,
        timezone: config.user_zone(),
        tier: Tier::Review,
        keep_days,
    };
    vec![
        prompt(
            EVENING,
            "Evening shutdown",
            EVENING_PROMPT,
            Cadence::Daily,
            14,
        ),
        prompt(
            WEEKLY,
            "Weekly plan",
            WEEKLY_PROMPT,
            Cadence::Weekly(WEEKLY_DAY),
            56,
        ),
        prompt(
            MONTHLY,
            "Monthly review",
            MONTHLY_PROMPT,
            Cadence::Monthly(1),
            200,
        ),
    ]
}

/// Whether `prompt` runs on `day` at all. Only the evening shutdown has a day off.
pub fn runs_on(prompt_name: &str, day: NaiveDate) -> bool {
    prompt_name != EVENING || day.weekday() != WEEKLY_DAY
}

/// [`runs_on`] for today in the prompt's zone, so the agent takes no `chrono` dependency.
pub fn runs_today(prompt: &ScheduledPrompt) -> bool {
    runs_on(
        prompt.name,
        Utc::now().with_timezone(&prompt.timezone).date_naive(),
    )
}

/// One time a ritual was raised, and whether he took part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub thread_id: String,
    pub raised_at: DateTime<Utc>,
    /// He wrote in the thread, or decided a card in it himself. A card the server closed as
    /// lapsed or superseded is not his decision.
    pub engaged: bool,
}

#[derive(SurrealValue)]
struct RaisedRow {
    thread_id: String,
    created_at: String,
}

/// The ritual's most recent runs, newest first.
pub async fn runs(db: &Database, prompt_name: &str, limit: usize) -> Result<Vec<Run>, String> {
    let mut resp = db
        .query(
            "SELECT thread_id, <string> created_at AS created_at FROM assistant_messages
             WHERE prompt_name = $prompt AND scheduled = true
             ORDER BY created_at DESC LIMIT $limit;",
        )
        .bind(("prompt", prompt_name.to_string()))
        .bind(("limit", limit as i64))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let raised: Vec<RaisedRow> = resp.take(0).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(raised.len());
    for r in raised {
        let Ok(at) = DateTime::parse_from_rfc3339(&r.created_at) else {
            continue;
        };
        let engaged = engaged(db, &r.thread_id).await?;
        out.push(Run {
            thread_id: r.thread_id,
            raised_at: at.with_timezone(&Utc),
            engaged,
        });
    }
    Ok(out)
}

async fn engaged(db: &Database, thread_id: &str) -> Result<bool, String> {
    let mut resp = db
        .query(
            "SELECT count() AS n FROM assistant_messages
             WHERE thread_id = $thread AND role = 'user' AND scheduled != true GROUP ALL;
             SELECT count() AS n FROM assistant_proposals
             WHERE thread_id = $thread AND decision IS NOT NONE
               AND (decided_reason IS NONE OR decided_reason NOT IN [$lapsed, $superseded])
             GROUP ALL;",
        )
        .bind(("thread", thread_id.to_string()))
        .bind(("lapsed", REASON_LAPSED))
        .bind(("superseded", REASON_SUPERSEDED))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let written: Vec<i64> = resp.take((0, "n")).map_err(|e| e.to_string())?;
    let decided: Vec<i64> = resp.take((1, "n")).map_err(|e| e.to_string())?;
    Ok(written.first().copied().unwrap_or(0) + decided.first().copied().unwrap_or(0) > 0)
}

/// Paused when its last [`LAPSE_AFTER`] runs all went untouched. Answering in any of their
/// threads resumes it, with nothing else to switch.
pub fn paused_by(runs: &[Run]) -> bool {
    runs.len() >= LAPSE_AFTER && runs.iter().take(LAPSE_AFTER).all(|r| !r.engaged)
}

pub async fn paused(db: &Database, prompt_name: &str) -> Result<bool, String> {
    Ok(paused_by(&runs(db, prompt_name, LAPSE_AFTER).await?))
}

/// A ritual's title as the summary names it.
pub fn title(prompt_name: &str) -> &'static str {
    match prompt_name {
        MORNING_PLAN => "Morning plan",
        EVENING => "Evening shutdown",
        WEEKLY => "Weekly plan",
        MONTHLY => "Monthly review",
        _ => "Ritual",
    }
}

/// What the summary adds for the rituals.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SummaryLines {
    /// Rituals raised today, keyed `<name>:<date>` so each day's is new.
    pub due: Vec<(String, String)>,
    /// Rituals that have paused, keyed `<name>:<newest thread>` so each pause is said once.
    pub paused: Vec<(String, String)>,
}

/// The ritual lines for the summary at `now`, built by code from the plans, tasks and goals.
/// A line does not wait on the agent: it says the ritual is due, and its thread follows.
pub async fn summary_lines(
    db: &Database,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> Result<SummaryLines, String> {
    let mut out = SummaryLines::default();
    if !config.enabled(Feature::Planning) || !config.enabled(Feature::Llm) {
        return Ok(out);
    }
    let today = now.with_timezone(&config.user_zone()).date_naive();
    for name in [MORNING_PLAN, EVENING, WEEKLY, MONTHLY] {
        let recent = runs(db, name, LAPSE_AFTER).await?;
        if paused_by(&recent) {
            let key = format!("{name}:{}", recent[0].thread_id);
            out.paused.push((
                key,
                format!(
                    "{} paused after {LAPSE_AFTER} skips; reply in its last thread to resume",
                    title(name)
                ),
            ));
            continue;
        }
        let due_today = match name {
            EVENING => runs_on(EVENING, today),
            WEEKLY => today.weekday() == WEEKLY_DAY,
            MONTHLY => today.day() == 1,
            _ => false,
        };
        if due_today {
            let line = match name {
                EVENING => evening_line(db, today).await?,
                WEEKLY => weekly_line(db, config, today).await?,
                _ => monthly_line(db, today).await?,
            };
            out.due.push((format!("{name}:{today}"), line));
        }
    }
    // Once a week, beside the weekly plan's line.
    if today.weekday() == WEEKLY_DAY {
        for (i, line) in load_lines(db, now).await?.into_iter().enumerate() {
            out.due.push((format!("load:{today}:{i}"), line));
        }
    }
    Ok(out)
}

/// Ruling 99's aims for a week of rituals, measured and never enforced.
pub const WEEKLY_MINUTES_AIM: i64 = 30;
pub const WEEKLY_DECISIONS_AIM: usize = 10;
/// Added to each run he took part in, for reading it.
const READ_MIN: i64 = 2;
/// No run counts for more: a reply hours later is a gap, not an hour of ritual.
const RUN_CAP_MIN: i64 = 60;

/// Estimated minutes he spent in one ritual thread: from his first reply or decision to his
/// last, plus reading time, capped. Zero when he never took part.
pub fn run_minutes(mut at: Vec<DateTime<Utc>>) -> i64 {
    at.sort();
    match (at.first(), at.last()) {
        (Some(first), Some(last)) => ((*last - *first).num_minutes() + READ_MIN).min(RUN_CAP_MIN),
        _ => 0,
    }
}

async fn activity(db: &Database, thread_id: &str) -> Result<(Vec<DateTime<Utc>>, usize), String> {
    let mut resp = db
        .query(
            "SELECT <string> created_at AS at FROM assistant_messages
             WHERE thread_id = $thread AND role = 'user' AND scheduled != true;
             SELECT <string> decided_at AS at FROM assistant_proposals
             WHERE thread_id = $thread AND decided_at IS NOT NONE
               AND (decided_reason IS NONE OR decided_reason NOT IN [$lapsed, $superseded]);
             SELECT count() AS n FROM assistant_proposals WHERE thread_id = $thread GROUP ALL;",
        )
        .bind(("thread", thread_id.to_string()))
        .bind(("lapsed", REASON_LAPSED))
        .bind(("superseded", REASON_SUPERSEDED))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let written: Vec<String> = resp.take((0, "at")).map_err(|e| e.to_string())?;
    let decided: Vec<String> = resp.take((1, "at")).map_err(|e| e.to_string())?;
    let asked: Vec<i64> = resp.take((2, "n")).map_err(|e| e.to_string())?;
    let at = written
        .iter()
        .chain(&decided)
        .filter_map(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc))
        .collect();
    Ok((at, asked.first().copied().unwrap_or(0).max(0) as usize))
}

/// The week's ritual load, said only when over an aim (ruling 99, measured as 55 is).
async fn load_lines(db: &Database, now: DateTime<Utc>) -> Result<Vec<String>, String> {
    let since = now - Duration::days(7);
    let mut minutes = 0;
    for name in [MORNING_PLAN, EVENING, WEEKLY, MONTHLY] {
        for run in runs(db, name, 8).await? {
            if run.raised_at >= since && run.engaged {
                minutes += run_minutes(activity(db, &run.thread_id).await?.0);
            }
        }
    }
    let mut out = Vec::new();
    if minutes > WEEKLY_MINUTES_AIM {
        out.push(format!(
            "Rituals took about {minutes} min this week, over the {WEEKLY_MINUTES_AIM} min aim"
        ));
    }
    // The weekly plan before today's, which has not been answered yet.
    let earlier = now - Duration::hours(12);
    if let Some(last) = runs(db, WEEKLY, 2)
        .await?
        .into_iter()
        .find(|r| r.raised_at < earlier)
    {
        let (_, asked) = activity(db, &last.thread_id).await?;
        if asked > WEEKLY_DECISIONS_AIM {
            out.push(format!(
                "Last weekly plan asked {asked} decisions, over the {WEEKLY_DECISIONS_AIM} aim"
            ));
        }
    }
    Ok(out)
}

async fn evening_line(db: &Database, today: NaiveDate) -> Result<String, String> {
    let line = match plan::accepted(db, today).await? {
        Some(p) => {
            let n = p.blocks.len() + p.untimed.len();
            format!(
                "Evening shutdown: {n} planned {} to settle",
                plural(n, "item")
            )
        }
        None => "Evening shutdown: no plan today, tomorrow's free time".to_string(),
    };
    Ok(line)
}

async fn weekly_line(
    db: &Database,
    config: &ResolvedConfig,
    today: NaiveDate,
) -> Result<String, String> {
    let mut planned = 0;
    for back in 1..=7 {
        if plan::accepted(db, today - Duration::days(back))
            .await?
            .is_some()
        {
            planned += 1;
        }
    }
    let mut line = format!("Weekly plan: {planned} of 7 days planned last week");
    if config.enabled(Feature::Tasks) {
        let due = tasks_due_within(db, today, 7).await?;
        line.push_str(&format!(", {due} {} due this week", plural(due, "task")));
    }
    Ok(line)
}

async fn monthly_line(db: &Database, today: NaiveDate) -> Result<String, String> {
    let month = today.format("%B");
    Ok(match goals::for_month(db, &goals::month_of(today)).await? {
        Some(g) => format!(
            "Monthly review: {} {} set for {month}",
            g.goals.len(),
            plural(g.goals.len(), "goal")
        ),
        None => format!("Monthly review: {month}'s goals to set"),
    })
}

async fn tasks_due_within(db: &Database, from: NaiveDate, days: i64) -> Result<usize, String> {
    let mut resp = db
        .query(
            "SELECT count() AS n FROM tasks
             WHERE status = 'todo' AND due_date IS NOT NONE AND due_date < $until GROUP ALL",
        )
        .bind(("until", (from + Duration::days(days)).to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let n: Vec<i64> = resp.take((0, "n")).map_err(|e| e.to_string())?;
    Ok(n.first().copied().unwrap_or(0).max(0) as usize)
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(engaged: bool) -> Run {
        Run {
            thread_id: "t".into(),
            raised_at: Utc::now(),
            engaged,
        }
    }

    #[test]
    fn five_untouched_runs_pause_and_one_answer_resumes() {
        let mut runs = vec![run(false); LAPSE_AFTER];
        assert!(paused_by(&runs));
        runs[3].engaged = true;
        assert!(!paused_by(&runs), "an answer in any of them resumes it");
        assert!(
            !paused_by(&vec![run(false); LAPSE_AFTER - 1]),
            "a new ritual is not paused"
        );
    }

    #[test]
    fn a_run_counts_his_active_span_plus_reading_capped() {
        let t = |m: i64| Utc::now() + Duration::minutes(m);
        assert_eq!(run_minutes(vec![]), 0);
        assert_eq!(run_minutes(vec![t(0)]), READ_MIN);
        assert_eq!(run_minutes(vec![t(9), t(0), t(4)]), 9 + READ_MIN);
        assert_eq!(
            run_minutes(vec![t(0), t(300)]),
            RUN_CAP_MIN,
            "a late reply is a gap"
        );
    }

    #[test]
    fn the_weekly_day_has_no_shutdown_of_its_own() {
        let sunday = NaiveDate::from_ymd_opt(2026, 11, 29).unwrap();
        assert_eq!(sunday.weekday(), WEEKLY_DAY);
        assert!(!runs_on(EVENING, sunday));
        assert!(runs_on(EVENING, sunday - Duration::days(1)));
        assert!(runs_on(WEEKLY, sunday));
    }

    #[tokio::test]
    async fn untouched_runs_pause_it_and_a_reply_in_one_resumes_it() {
        use std::sync::Arc;

        use crate::config::ALL_FEATURES;
        use crate::events::{
            AssistantProjection, AssistantQuestionAskedPayload, EventWriter, NewEvent,
            ProjectionRunner, SurrealEventStore,
        };

        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(AssistantProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(
            store,
            runner,
            ALL_FEATURES.iter().copied().collect(),
            "agent",
        );
        let ask = |thread: &str, scheduled: bool, day: u32| {
            let payload = AssistantQuestionAskedPayload {
                thread_id: thread.into(),
                message_id: format!("{thread}-{scheduled}"),
                text: "Run my evening shutdown.".into(),
                title: None,
                scheduled,
                prompt_name: scheduled.then(|| EVENING.to_string()),
            };
            let mut event = NewEvent::assistant_question_asked("agent", &payload).unwrap();
            event.timestamp = format!("2026-11-{day:02}T19:00:00Z").parse().unwrap();
            event
        };
        for day in 1..=LAPSE_AFTER as u32 {
            writer
                .append_new(ask(&format!("t{day}"), true, day))
                .await
                .unwrap();
        }
        let recent = runs(&db, EVENING, LAPSE_AFTER).await.unwrap();
        assert_eq!(recent[0].thread_id, "t5", "newest first");
        assert!(paused(&db, EVENING).await.unwrap());

        writer.append_new(ask("t5", false, 6)).await.unwrap();
        assert!(!paused(&db, EVENING).await.unwrap(), "his reply resumes it");
        assert!(!paused(&db, WEEKLY).await.unwrap(), "rituals lapse apart");
    }

    #[test]
    fn every_ritual_prompt_is_answered_by_planning() {
        let config = ResolvedConfig::default();
        for p in prompts(&config) {
            assert!(
                is_ritual(p.name),
                "{} would skip the planning brief",
                p.name
            );
        }
        assert!(is_ritual(MORNING_PLAN));
        assert!(!is_ritual(crate::assistant::scheduled::CHECK_IN));
    }
}
