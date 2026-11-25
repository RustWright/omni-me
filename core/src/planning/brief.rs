//! What the planning specialist is told (ruling 112): its rules, and each day it may plan, worked
//! out in code. Free time is stated, never left for the model to add up.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use surrealdb::types::SurrealValue;

use super::day::DayContext;
use super::{goals, plan};
use crate::config::{Feature, ResolvedConfig};
use crate::db::Database;

/// The rules the planning brief adds to the system prompt. Names each planning action and its
/// argument shapes: planning is reached only through this brief, so this is how the model finds
/// them.
pub const RULES: &str = "\n\n## Planning work\n\
This request is about planning the user's day. The days below were worked out in code: \
never add up minutes yourself, and never move or shorten a calendar event. A plan is not a \
record type: the days, any plan already accepted and the open tasks due are all below, so never \
list or describe `plan`; search only for something the user names that is not listed.\n\
- Propose a plan with `propose`, action `plan.set`, args: `date` (YYYY-MM-DD), `blocks` (a \
list, each \"HH:MM-HH:MM Title\" in local time, inside the free time) and `untimed` (a list, \
each \"Title (N min)\"). Deep work goes in blocks, small things in the untimed list. A plan \
needs at least one of the two.\n\
- Plan from what they asked for and their open tasks. Leave slack: do not fill every free \
minute, and keep blocks to what they can start.\n\
- If `propose` refuses a block, fix it and propose the whole plan again.\n\
- To change a plan, propose the whole day again; it replaces the earlier one. Say what changed.\n\
- Say the fit `propose` returns in one line. A plan over the free time is still proposed: say \
so, never refuse.\n\
- If a clash is listed, ask which event they are going to in one line.\n\
- Set a month's goals with `propose`, action `goal.set`, args: `month` (YYYY-MM) and `goals` (a \
list of at most 5 short lines). Use the user's own words. It replaces any goals already set for \
that month, so propose the whole set.";

/// The context block: today, while any of it is left, then tomorrow.
pub async fn context(db: &Database, config: &ResolvedConfig, now: DateTime<Utc>) -> String {
    let tz = config.user_zone();
    let today = now.with_timezone(&tz).date_naive();
    let mut out = String::new();
    for date in [today, today + Duration::days(1)] {
        match DayContext::load(db, config, date).await {
            Ok(ctx) => {
                if date == today && ctx.window(now).is_none() {
                    continue;
                }
                out.push_str(&ctx.describe(now));
                if let Ok(Some(p)) = plan::accepted(db, date).await {
                    out.push_str(&accepted_lines(&p, tz));
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, %date, "could not read the day for the planning brief");
                out.push_str(&format!(
                    "\n\n### {date}\nThe day could not be read ({e}); do not propose a plan for it."
                ));
            }
        }
    }
    out.push_str(&goal_lines(db, today).await);
    if config.enabled(Feature::Tasks) {
        out.push_str(&open_tasks(db, today).await);
    }
    out
}

/// This month's goals, so a plan can serve them and the monthly review can see what was set.
async fn goal_lines(db: &Database, today: NaiveDate) -> String {
    let month = goals::month_of(today);
    match goals::for_month(db, &month).await {
        Ok(Some(g)) => {
            let mut out = format!("\n\nGoals for {month}:");
            for goal in &g.goals {
                out.push_str(&format!("\n- {goal}"));
            }
            out
        }
        Ok(None) => format!("\n\nNo goals are set for {month}."),
        Err(e) => {
            tracing::warn!(error = %e, "could not read the month's goals for the planning brief");
            String::new()
        }
    }
}

fn accepted_lines(p: &plan::AcceptedPlan, tz: chrono_tz::Tz) -> String {
    let hm = |t: DateTime<Utc>| t.with_timezone(&tz).format("%H:%M").to_string();
    let mut out = String::from("\nThe plan already accepted for this day:");
    for b in &p.blocks {
        out.push_str(&format!("\n- {}-{} {}", hm(b.start), hm(b.end), b.title));
    }
    for u in &p.untimed {
        out.push_str(&format!("\n- {} ({} min)", u.title, u.minutes));
    }
    out
}

#[derive(SurrealValue)]
struct TaskRow {
    title: String,
    estimate_min: Option<i64>,
    due_date: Option<String>,
    scheduled: Option<String>,
}

/// How far ahead a due date pulls a task into the brief.
const DUE_WITHIN_DAYS: i64 = 7;
/// Most tasks the brief lists; past this it says how many more.
const MAX_TASKS: usize = 30;

async fn task_rows(db: &Database, today: NaiveDate) -> Result<Vec<TaskRow>, String> {
    let soon = today + Duration::days(DUE_WITHIN_DAYS);
    let mut resp = db
        .query(
            "SELECT title, estimate_min, due_date, scheduled FROM tasks
             WHERE status = 'todo'
               AND ((scheduled IS NOT NONE AND scheduled <= $today)
                    OR (due_date IS NOT NONE AND due_date <= $soon))
             ORDER BY due_date ASC, scheduled ASC",
        )
        .bind(("today", today.to_string()))
        .bind(("soon", soon.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    resp.take(0).map_err(|e| e.to_string())
}

/// Open tasks meant for today or earlier, and those due within a week. A passed scheduled day
/// is listed, not dropped: it comes back to him in the next plan (ruling 130).
async fn open_tasks(db: &Database, today: NaiveDate) -> String {
    let rows = match task_rows(db, today).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "could not read open tasks for the planning brief");
            return "\n\n### Open tasks\nThe task list could not be read.".to_string();
        }
    };
    if rows.is_empty() {
        return "\n\n### Open tasks\nNone are scheduled for today or due this week.".to_string();
    }
    let mut out = String::from("\n\n### Open tasks (scheduled for today or earlier, or due soon)");
    for t in rows.iter().take(MAX_TASKS) {
        let mut line = format!("\n- {}", t.title);
        if let Some(m) = t.estimate_min {
            line.push_str(&format!(", his estimate {m} min"));
        }
        if let Some(d) = &t.due_date {
            line.push_str(&format!(", due {d}"));
        }
        if let Some(s) = t
            .scheduled
            .as_deref()
            .filter(|s| *s < today.to_string().as_str())
        {
            line.push_str(&format!(", meant for {s} and not done"));
        }
        out.push_str(&line);
    }
    if rows.len() > MAX_TASKS {
        out.push_str(&format!("\n({} more not listed.)", rows.len() - MAX_TASKS));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Planning is not in the read catalogue, so `describe_type` never lists its actions: this
    /// brief is the only place the model learns their names.
    #[test]
    fn the_brief_names_every_planning_action() {
        for action in crate::assistant::actions::ALL_ACTIONS
            .iter()
            .filter(|a| a.feature == Feature::Planning)
        {
            assert!(
                RULES.contains(&format!("`{}`", action.name)),
                "{} is never named to the model, so it can never be proposed",
                action.name
            );
        }
    }
}
