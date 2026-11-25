//! Block reminders (ruling 96) and the Start they carry (ruling 93). Rationale:
//! `docs/src/planning.md` § Reminders.
//!
//! The app schedules these on the phone and reconciles them against `upcoming`, never trusting
//! what it scheduled before: a force-stop cancels every scheduled reminder.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use surrealdb::types::SurrealValue;

use super::plan::{self, AcceptedPlan};
use crate::config::ResolvedConfig;
use crate::db::Database;
use crate::events::{BlockTimerPayload, EventType, EventWriter, NewEvent};

/// Prefix of a reminder's action id. The plugin reports only the action id on a tap, so the id
/// carries the block: `start:<block start, unix seconds>`.
pub const START_PREFIX: &str = "start:";

/// One reminder the phone should hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockReminder {
    /// The notification id: the block's start in minutes since the epoch. Blocks never
    /// overlap, so two reminders never share one.
    pub id: i32,
    /// The action type id and the Start action's id, both `start:<unix>`.
    pub action: String,
    pub at: DateTime<Utc>,
    pub title: String,
    /// `14:15–15:45`, local.
    pub body: String,
}

/// The notification id for a block starting at `start`.
pub fn notification_id(start: DateTime<Utc>) -> i32 {
    i32::try_from(start.timestamp().div_euclid(60)).unwrap_or(i32::MAX)
}

pub fn start_action(start: DateTime<Utc>) -> String {
    format!("{START_PREFIX}{}", start.timestamp())
}

/// The block start a Start action names, or `None` for any other action (a plain tap is `tap`).
pub fn parse_start_action(action: &str) -> Option<DateTime<Utc>> {
    let secs: i64 = action.strip_prefix(START_PREFIX)?.parse().ok()?;
    DateTime::from_timestamp(secs, 0)
}

/// Reminders for every block in `plans` that has not started by `now`.
pub fn wanted(plans: &[AcceptedPlan], tz: Tz, now: DateTime<Utc>) -> Vec<BlockReminder> {
    let mut out: Vec<BlockReminder> = plans
        .iter()
        .flat_map(|p| p.blocks.iter())
        .filter(|b| b.start > now)
        .map(|b| BlockReminder {
            id: notification_id(b.start),
            action: start_action(b.start),
            at: b.start,
            title: b.title.clone(),
            body: format!(
                "{}\u{2013}{}",
                b.start.with_timezone(&tz).format("%H:%M"),
                b.end.with_timezone(&tz).format("%H:%M")
            ),
        })
        .collect();
    out.sort_by_key(|r| r.at);
    out.dedup_by_key(|r| r.id);
    out
}

/// The dates whose plans can hold a block still ahead: yesterday's can run past midnight, and
/// tomorrow's is the furthest a plan is made for.
fn plan_dates(now: DateTime<Utc>, tz: Tz) -> [NaiveDate; 3] {
    let today = now.with_timezone(&tz).date_naive();
    [today - Duration::days(1), today, today + Duration::days(1)]
}

/// The reminders the phone should hold now, from the accepted plans.
pub async fn upcoming(
    db: &Database,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> Result<Vec<BlockReminder>, String> {
    let tz = config.user_zone();
    let mut plans = Vec::new();
    for date in plan_dates(now, tz) {
        if let Some(p) = plan::accepted(db, date).await? {
            plans.push(p);
        }
    }
    Ok(wanted(&plans, tz, now))
}

/// A block a Start tap began.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartedBlock {
    pub title: String,
    /// `14:15–15:45`, local.
    pub when: String,
}

/// Starts the timer on the accepted block that begins at `block_start`. Refused when no plan in
/// force has that block, as after a re-plan moved it: a stale reminder never times a block
/// that is not in the plan.
pub async fn start_block(
    db: &Database,
    writer: &EventWriter,
    config: &ResolvedConfig,
    block_start: DateTime<Utc>,
) -> Result<StartedBlock, String> {
    let tz = config.user_zone();
    for date in plan_dates(block_start, tz) {
        let Some(p) = plan::accepted(db, date).await? else {
            continue;
        };
        let Some(b) = p.blocks.iter().find(|b| b.start == block_start) else {
            continue;
        };
        let payload = BlockTimerPayload {
            date,
            block_start,
            title: b.title.clone(),
        };
        writer
            .append_new(NewEvent {
                id: None,
                event_type: EventType::BlockTimerStarted.to_string(),
                aggregate_id: payload.key(),
                timestamp: Utc::now(),
                device_id: writer.device_id().to_string(),
                payload: serde_json::to_value(&payload).map_err(|e| e.to_string())?,
            })
            .await
            .map_err(|e| e.to_string())?;
        return Ok(StartedBlock {
            title: b.title.clone(),
            when: format!(
                "{}\u{2013}{}",
                b.start.with_timezone(&tz).format("%H:%M"),
                b.end.with_timezone(&tz).format("%H:%M")
            ),
        });
    }
    Err("That block is no longer in the plan.".to_string())
}

/// The first time a block was started, as the `block_timers` row holds it.
pub async fn started_at(
    db: &Database,
    date: NaiveDate,
    block_start: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, String> {
    #[derive(SurrealValue)]
    struct Row {
        started_at: String,
    }
    let key = BlockTimerPayload {
        date,
        block_start,
        title: String::new(),
    }
    .key();
    let mut resp = db
        .query("SELECT started_at FROM type::record('block_timers', $key)")
        .bind(("key", key))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let rows: Vec<Row> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|r| r.started_at.parse().ok()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::{ALL_FEATURES, ConfigKey, ConfigValue};
    use crate::events::{PlanningProjection, ProjectionRunner, SurrealEventStore};
    use crate::planning::fit::Block;

    const TZ: Tz = chrono_tz::America::Toronto;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn block(start: &str, end: &str, title: &str) -> Block {
        Block {
            start: at(start),
            end: at(end),
            title: title.into(),
        }
    }

    fn plan(date: &str, blocks: Vec<Block>) -> AcceptedPlan {
        AcceptedPlan {
            date: date.parse().unwrap(),
            blocks,
            untimed: vec![],
        }
    }

    #[test]
    fn only_blocks_still_ahead_are_reminded() {
        let plans = [
            plan(
                "2026-11-25",
                vec![
                    block("2026-11-25T14:00:00Z", "2026-11-25T15:30:00Z", "Essay"),
                    block("2026-11-25T19:15:00Z", "2026-11-25T20:45:00Z", "Thesis"),
                ],
            ),
            plan(
                "2026-11-26",
                vec![block("2026-11-26T14:00:00Z", "2026-11-26T15:00:00Z", "Gym")],
            ),
        ];
        let got = wanted(&plans, TZ, at("2026-11-25T14:00:00Z"));
        let titles: Vec<&str> = got.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Thesis", "Gym"],
            "a block starting now is under way"
        );
        assert_eq!(got[0].body, "14:15\u{2013}15:45");
        assert_eq!(got[0].at, at("2026-11-25T19:15:00Z"));
    }

    #[test]
    fn the_action_names_its_block_and_nothing_else_parses() {
        let start = at("2026-11-25T19:15:00Z");
        let action = start_action(start);
        assert_eq!(parse_start_action(&action), Some(start));
        assert_eq!(parse_start_action("tap"), None);
        assert_eq!(parse_start_action("start:"), None);
        assert_eq!(parse_start_action("start:x"), None);
        assert_ne!(
            notification_id(start),
            notification_id(start + Duration::minutes(1)),
            "blocks a minute apart get their own reminders"
        );
    }

    async fn harness() -> (Database, EventWriter, ResolvedConfig) {
        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(PlanningProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(
            store,
            runner,
            ALL_FEATURES.iter().copied().collect(),
            "phone",
        );
        let mut config = ResolvedConfig::default();
        config.global.insert(
            ConfigKey::AssistantCheckInTimezone,
            ConfigValue::Text("America/Toronto".into()),
        );
        (db, writer, config)
    }

    async fn accept(writer: &EventWriter, p: &AcceptedPlan) {
        writer
            .append_new(NewEvent {
                id: None,
                event_type: EventType::DayPlanAccepted.to_string(),
                aggregate_id: p.date.to_string(),
                timestamp: Utc::now(),
                device_id: "phone".into(),
                payload: serde_json::to_value(p).unwrap(),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn start_times_the_block_in_force_and_refuses_a_moved_one() {
        let (db, writer, config) = harness().await;
        let first = plan(
            "2026-11-25",
            vec![block(
                "2026-11-25T19:15:00Z",
                "2026-11-25T20:45:00Z",
                "Thesis",
            )],
        );
        accept(&writer, &first).await;
        let start = at("2026-11-25T19:15:00Z");

        let started = start_block(&db, &writer, &config, start).await.unwrap();
        assert_eq!(started.title, "Thesis");
        assert_eq!(started.when, "14:15\u{2013}15:45");
        let first_at = started_at(&db, first.date, start).await.unwrap();
        assert!(first_at.is_some());

        start_block(&db, &writer, &config, start).await.unwrap();
        assert_eq!(
            started_at(&db, first.date, start).await.unwrap(),
            first_at,
            "a second tap keeps the first start"
        );

        let replan = plan(
            "2026-11-25",
            vec![block(
                "2026-11-25T20:00:00Z",
                "2026-11-25T21:00:00Z",
                "Thesis",
            )],
        );
        accept(&writer, &replan).await;
        assert!(start_block(&db, &writer, &config, start).await.is_err());

        let due = upcoming(&db, &config, at("2026-11-25T15:00:00Z"))
            .await
            .unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].at, at("2026-11-25T20:00:00Z"), "the re-plan's block");
    }
}
