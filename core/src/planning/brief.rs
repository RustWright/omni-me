//! What the planning specialist is told (ruling 112): its rules, and each day it may plan, worked
//! out in code. Free time is stated, never left for the model to add up.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use surrealdb::types::SurrealValue;

use super::day::DayContext;
use super::{goals, plan};
use crate::config::{ConfigKey, Feature, ResolvedConfig};
use crate::db::Database;

/// The rules the planning brief adds to the system prompt. Names each planning action and its
/// argument shapes: planning is reached only through this brief, so this is how the model finds
/// them.
pub const RULES: &str = "\n\n## Planning work\n\
This request is about planning the user's day. The days below were worked out in code: \
never add up minutes yourself, and never move or shorten a calendar event. Plans and goals are \
not record types: the days, any plan already accepted, the last 7 days' plans, last month's and \
this month's goals and the open tasks due are all below, so never list, describe or search for \
`plan` or `goal`. Their actions, `plan.set` and \
`goal.set`, are proposed directly as described here. Search only for something the user names \
that is not listed.\n\
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
- \"Waiting on the user\" is what surfaced on its own. The weekly plan asks about each item, \
with its other decisions; anywhere else, mention one only if the user asks. Never plan, attach, \
drop or pull one in without their answer.\n\
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
    out.push_str(&past_week(db, tz, today).await);
    out.push_str(&week_ahead(db, config, now, today).await);
    out.push_str(&goal_lines(db, today).await);
    if config.enabled(Feature::Tasks) {
        out.push_str(&open_tasks(db, today).await);
        out.push_str(&waiting_lines(db, config, today).await);
    }
    out
}

/// Most lines each part of "Waiting on the user" lists; past this it says how many more.
const MAX_WAITING: usize = 10;

/// What surfaces on its own (rulings 91, 92), for a ritual to ask about.
async fn waiting_lines(db: &Database, config: &ResolvedConfig, today: NaiveDate) -> String {
    let w = match crate::tasks::queries::waiting(
        db,
        today,
        config.int_of(ConfigKey::PlanningIndexStaleDays),
        config.int_of(ConfigKey::PlanningTaskStaleDays),
    )
    .await
    {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "could not read what is waiting for the planning brief");
            return "\n\n### Waiting on the user\nThis could not be read.".to_string();
        }
    };
    let mut lines: Vec<String> = Vec::new();
    let mut part = |items: Vec<String>| {
        let more = items.len().saturating_sub(MAX_WAITING);
        lines.extend(items.into_iter().take(MAX_WAITING));
        if more > 0 {
            lines.push(format!("({more} more not listed.)"));
        }
    };
    part(
        w.hub
            .iter()
            .map(|h| match &h.due {
                Some(d) => format!("From the {} task list: {}, due {d}", h.project, h.title),
                None => format!("From the {} task list: {}", h.project, h.title),
            })
            .collect(),
    );
    part(
        w.stale_indexes
            .iter()
            .map(|p| format!("The {p} task list has not changed in a while: ask once whether it is still current."))
            .collect(),
    );
    part(
        w.deadlines
            .iter()
            .map(|d| {
                format!(
                    "Deadline with no task yet: {} {}, due {}",
                    d.course, d.title, d.due_date
                )
            })
            .collect(),
    );
    part(
        w.untouched
            .iter()
            .map(|t| format!("Untouched for weeks, offer to drop in the weekly plan only: {t}"))
            .collect(),
    );
    if lines.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\n### Waiting on the user");
    for l in lines {
        out.push_str(&format!("\n- {l}"));
    }
    out
}

/// Last month's goals and this month's, so a plan can serve them and the monthly review can see
/// what was set without searching.
async fn goal_lines(db: &Database, today: NaiveDate) -> String {
    let this = goals::month_of(today);
    let last = goals::month_of(today.with_day(1).unwrap_or(today) - Duration::days(1));
    let mut out = String::new();
    for (month, verb) in [(last, "were"), (this, "are")] {
        match goals::for_month(db, &month).await {
            Ok(Some(g)) => {
                out.push_str(&format!("\n\nGoals for {month}:"));
                for goal in &g.goals {
                    out.push_str(&format!("\n- {goal}"));
                }
            }
            Ok(None) => out.push_str(&format!("\n\nNo goals {verb} set for {month}.")),
            Err(e) => {
                tracing::warn!(error = %e, "could not read a month's goals for the planning brief");
            }
        }
    }
    out
}

/// The next seven days' free time, worked out as the Plan view does it, so the weekly plan can
/// say what the coming week has.
async fn week_ahead(
    db: &Database,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
    today: NaiveDate,
) -> String {
    let ctx = match DayContext::load(db, config, today).await {
        Ok(ctx) => ctx,
        Err(e) => {
            return format!("\n\n### Free time, the next 7 days\nIt could not be read ({e}).");
        }
    };
    let mut out = String::from("\n\n### Free time, the next 7 days");
    let mut total = 0;
    for ahead in 1..=7 {
        let date = today + Duration::days(ahead);
        let day = DayContext {
            date,
            first_activity: None,
            ..ctx.clone()
        };
        let free = day.free_time(now).map_or(0, |f| f.free_min());
        total += free;
        out.push_str(&format!(
            "\n- {}: {} free",
            date.format("%a %Y-%m-%d"),
            super::view::duration(free)
        ));
    }
    out.push_str(&format!("\nIn all: {} free.", super::view::duration(total)));
    out
}

/// The last seven days' accepted plans, with any time a block's timer recorded, so the weekly
/// plan can say how the week went without searching for it.
async fn past_week(db: &Database, tz: chrono_tz::Tz, today: NaiveDate) -> String {
    let hm = |t: DateTime<Utc>| t.with_timezone(&tz).format("%H:%M").to_string();
    let mut out = String::from("\n\n### The last 7 days' plans");
    for back in (1..=7).rev() {
        let date = today - Duration::days(back);
        let day = date.format("%a %Y-%m-%d");
        let p = match plan::accepted(db, date).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                out.push_str(&format!("\n- {day}: no plan"));
                continue;
            }
            Err(e) => {
                out.push_str(&format!("\n- {day}: could not be read ({e})"));
                continue;
            }
        };
        let mut items = Vec::new();
        for b in &p.blocks {
            let mut item = format!("{}-{} {}", hm(b.start), hm(b.end), b.title);
            if let Ok(Some((_, t))) = super::reminders::timer(db, date, b.start).await {
                let secs = t.active_total + t.active_secs;
                item.push_str(&format!(" (timed {})", super::view::duration(secs / 60)));
            }
            items.push(item);
        }
        items.extend(
            p.untimed
                .iter()
                .map(|u| format!("{} ({} min)", u.title, u.minutes)),
        );
        out.push_str(&format!("\n- {day}: {}", items.join("; ")));
    }
    out
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
