use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use surrealdb::types::SurrealValue;

use crate::calendar::{CalendarEvent, EventTime};
use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::tasks_projection::{TimerState, TimerStep};
use super::types::{
    BlockTimerPayload, CalendarAnsweredPayload, CalendarChangedPayload, CalendarReadPayload,
    DayPlanAcceptedPayload, MonthGoalsSetPayload, PlaceSetPayload, TravelTimeSetPayload,
};

/// Projection over the calendar as read in, his answers about it, his places and his accepted
/// day plans.
///
/// Each row keeps the whole value as JSON in `body`, read back as it was written; the other
/// columns are for filtering and order.
pub struct PlanningProjection;

impl PlanningProjection {
    pub const NAME: &'static str = "planning";
}

#[async_trait]
impl Projection for PlanningProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    /// 2 added `day_plans`, 3 `block_timers`, 4 `month_goals`, 5 the timer on `block_timers`.
    /// `calendar_reads` took no bump: no event predates it, and a bump rebuilds every device.
    fn version(&self) -> u32 {
        5
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS calendar_events SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS source ON calendar_events TYPE string;
             DEFINE FIELD IF NOT EXISTS uid ON calendar_events TYPE string;
             DEFINE FIELD IF NOT EXISTS title ON calendar_events TYPE string;
             -- RFC 3339 UTC for a timed event, YYYY-MM-DD for an all-day one.
             DEFINE FIELD IF NOT EXISTS start ON calendar_events TYPE string;
             DEFINE FIELD IF NOT EXISTS all_day ON calendar_events TYPE bool;
             DEFINE FIELD IF NOT EXISTS series_id ON calendar_events TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS body ON calendar_events TYPE string;
             DEFINE INDEX IF NOT EXISTS idx_calendar_event_source ON calendar_events
                 FIELDS source;

             DEFINE TABLE IF NOT EXISTS calendar_answers SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS key ON calendar_answers TYPE string;
             DEFINE FIELD IF NOT EXISTS body ON calendar_answers TYPE string;
             DEFINE FIELD IF NOT EXISTS answered_at ON calendar_answers TYPE datetime;

             DEFINE TABLE IF NOT EXISTS planning_places SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS name ON planning_places TYPE string;
             -- Authoritative over the flag inside body: setting a new home clears the others.
             DEFINE FIELD IF NOT EXISTS home ON planning_places TYPE bool;
             DEFINE FIELD IF NOT EXISTS body ON planning_places TYPE string;

             DEFINE TABLE IF NOT EXISTS planning_travel SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS a ON planning_travel TYPE string;
             DEFINE FIELD IF NOT EXISTS b ON planning_travel TYPE string;
             DEFINE FIELD IF NOT EXISTS minutes ON planning_travel TYPE int;

             -- One row per date: the plan last accepted for it, which supersedes any before.
             DEFINE TABLE IF NOT EXISTS day_plans SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS date ON day_plans TYPE string;
             DEFINE FIELD IF NOT EXISTS body ON day_plans TYPE string;
             DEFINE FIELD IF NOT EXISTS accepted_at ON day_plans TYPE datetime;

             -- One row per plan block ever started (ruling 93), keyed by BlockTimerPayload::key.
             DEFINE TABLE IF NOT EXISTS block_timers SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS date ON block_timers TYPE string;
             DEFINE FIELD IF NOT EXISTS block_start ON block_timers TYPE string;
             DEFINE FIELD IF NOT EXISTS title ON block_timers TYPE string;
             -- RFC 3339, the first start: a second tap does not move it.
             DEFINE FIELD IF NOT EXISTS started_at ON block_timers TYPE string;
             -- The timer, as TimerState holds it; times RFC 3339, durations in seconds.
             DEFINE FIELD IF NOT EXISTS running ON block_timers TYPE option<bool>;
             DEFINE FIELD IF NOT EXISTS session_start ON block_timers TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS running_since ON block_timers TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS active_secs ON block_timers TYPE option<int>;
             DEFINE FIELD IF NOT EXISTS active_total ON block_timers TYPE option<int>;
             DEFINE FIELD IF NOT EXISTS elapsed_total ON block_timers TYPE option<int>;

             -- One row per month, YYYY-MM: the goals last set for it.
             DEFINE TABLE IF NOT EXISTS month_goals SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS month ON month_goals TYPE string;
             DEFINE FIELD IF NOT EXISTS body ON month_goals TYPE string;
             DEFINE FIELD IF NOT EXISTS set_at ON month_goals TYPE datetime;

             -- One row per calendar source: its latest read status, written only on a change.
             DEFINE TABLE IF NOT EXISTS calendar_reads SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS source ON calendar_reads TYPE string;
             DEFINE FIELD IF NOT EXISTS ok ON calendar_reads TYPE bool;
             DEFINE FIELD IF NOT EXISTS error ON calendar_reads TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS changed_at ON calendar_reads TYPE datetime;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DELETE FROM calendar_events; DELETE FROM calendar_answers;
             DELETE FROM planning_places; DELETE FROM planning_travel; DELETE FROM day_plans;
             DELETE FROM block_timers; DELETE FROM month_goals; DELETE FROM calendar_reads;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "calendar_changed" => match payload::<CalendarChangedPayload>(event) {
                Some(p) => on_changed(&p, db).await,
                None => Ok(()),
            },
            "calendar_answered" => match payload::<CalendarAnsweredPayload>(event) {
                Some(p) => {
                    let body = serde_json::to_string(&p.answer)
                        .map_err(|e| EventError::Validation(e.to_string()))?;
                    db.query(
                        "UPSERT type::record('calendar_answers', $key) SET
                            key = $key, body = $body, answered_at = type::datetime($ts)",
                    )
                    .bind(("key", p.answer.key()))
                    .bind(("body", body))
                    .bind(("ts", event.timestamp.to_rfc3339()))
                    .await?
                    .check()?;
                    Ok(())
                }
                None => Ok(()),
            },
            "calendar_read" => match payload::<CalendarReadPayload>(event) {
                Some(p) => {
                    db.query(
                        "UPSERT type::record('calendar_reads', $source) SET
                            source = $source, ok = $ok, error = $error,
                            changed_at = type::datetime($ts)",
                    )
                    .bind(("source", p.source))
                    .bind(("ok", p.ok))
                    .bind(("error", p.error))
                    .bind(("ts", event.timestamp.to_rfc3339()))
                    .await?
                    .check()?;
                    Ok(())
                }
                None => Ok(()),
            },
            "place_set" => match payload::<PlaceSetPayload>(event) {
                Some(p) => {
                    let body = serde_json::to_string(&p)
                        .map_err(|e| EventError::Validation(e.to_string()))?;
                    if p.home {
                        db.query("UPDATE planning_places SET home = false WHERE name != $name")
                            .bind(("name", p.name.clone()))
                            .await?
                            .check()?;
                    }
                    db.query(
                        "UPSERT type::record('planning_places', $name) SET
                            name = $name, home = $home, body = $body",
                    )
                    .bind(("name", p.name.clone()))
                    .bind(("home", p.home))
                    .bind(("body", body))
                    .await?
                    .check()?;
                    Ok(())
                }
                None => Ok(()),
            },
            "travel_time_set" => match payload::<TravelTimeSetPayload>(event) {
                Some(p) => {
                    let (a, b) = crate::planning::places::pair(&p.a, &p.b);
                    db.query(
                        "UPSERT type::record('planning_travel', $key) SET
                            a = $a, b = $b, minutes = $minutes",
                    )
                    .bind(("key", format!("{a}/{b}")))
                    .bind(("a", a))
                    .bind(("b", b))
                    .bind(("minutes", i64::from(p.minutes)))
                    .await?
                    .check()?;
                    Ok(())
                }
                None => Ok(()),
            },
            "day_plan_accepted" => match payload::<DayPlanAcceptedPayload>(event) {
                Some(p) => {
                    let body = serde_json::to_string(&p)
                        .map_err(|e| EventError::Validation(e.to_string()))?;
                    db.query(
                        "UPSERT type::record('day_plans', $date) SET
                            date = $date, body = $body, accepted_at = type::datetime($ts)",
                    )
                    .bind(("date", p.date.to_string()))
                    .bind(("body", body))
                    .bind(("ts", event.timestamp.to_rfc3339()))
                    .await?
                    .check()?;
                    Ok(())
                }
                None => Ok(()),
            },
            "month_goals_set" => match payload::<MonthGoalsSetPayload>(event) {
                Some(p) => {
                    let body = serde_json::to_string(&p)
                        .map_err(|e| EventError::Validation(e.to_string()))?;
                    db.query(
                        "UPSERT type::record('month_goals', $month) SET
                            month = $month, body = $body, set_at = type::datetime($ts)",
                    )
                    .bind(("month", p.month.clone()))
                    .bind(("body", body))
                    .bind(("ts", event.timestamp.to_rfc3339()))
                    .await?
                    .check()?;
                    Ok(())
                }
                None => Ok(()),
            },
            kind @ ("block_timer_started" | "block_timer_paused" | "block_timer_stopped") => {
                match (payload::<BlockTimerPayload>(event), TimerStep::of(kind)) {
                    (Some(p), Some(step)) => on_block_timer(step, &p, event, db).await,
                    _ => Ok(()),
                }
            }
            _ => Ok(()),
        }
    }
}

/// One block's timer row, as stored.
#[derive(Debug, SurrealValue)]
pub(crate) struct BlockTimerRow {
    pub started_at: String,
    pub running: Option<bool>,
    pub session_start: Option<String>,
    pub running_since: Option<String>,
    pub active_secs: Option<i64>,
    pub active_total: Option<i64>,
    pub elapsed_total: Option<i64>,
}

impl BlockTimerRow {
    pub(crate) fn state(&self) -> TimerState {
        let ts = |s: &Option<String>| {
            s.as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&Utc))
        };
        TimerState {
            running: self.running.unwrap_or(false),
            session_start: ts(&self.session_start),
            running_since: ts(&self.running_since),
            active_secs: self.active_secs.unwrap_or(0),
            active_total: self.active_total.unwrap_or(0),
            elapsed_total: self.elapsed_total.unwrap_or(0),
        }
    }
}

pub(crate) const BLOCK_TIMER_COLUMNS: &str = "SELECT started_at, running, session_start, \
     running_since, active_secs, active_total, elapsed_total FROM type::record('block_timers', $key)";

/// A pause or stop on a block never started is dropped: there is no timer to change.
async fn on_block_timer(
    step: TimerStep,
    p: &BlockTimerPayload,
    event: &Event,
    db: &Database,
) -> Result<(), EventError> {
    let mut resp = db
        .query(BLOCK_TIMER_COLUMNS)
        .bind(("key", p.key()))
        .await?
        .check()?;
    let existing: Option<BlockTimerRow> = resp.take::<Vec<BlockTimerRow>>(0)?.into_iter().next();
    let (started_at, mut state) = match existing {
        Some(row) => (row.started_at.clone(), row.state()),
        None if step == TimerStep::Start => (event.timestamp.to_rfc3339(), TimerState::default()),
        None => return Ok(()),
    };
    state.step(step, event.timestamp);
    db.query(
        "UPSERT type::record('block_timers', $key) SET
            date = $date, block_start = $block_start, title = $title, started_at = $started_at,
            running = $running, session_start = $session_start, running_since = $running_since,
            active_secs = $active_secs, active_total = $active_total,
            elapsed_total = $elapsed_total",
    )
    .bind(("key", p.key()))
    .bind(("date", p.date.to_string()))
    .bind(("block_start", p.block_start.to_rfc3339()))
    .bind(("title", p.title.clone()))
    .bind(("started_at", started_at))
    .bind(("running", state.running))
    .bind(("session_start", state.session_start.map(|t| t.to_rfc3339())))
    .bind(("running_since", state.running_since.map(|t| t.to_rfc3339())))
    .bind(("active_secs", state.active_secs))
    .bind(("active_total", state.active_total))
    .bind(("elapsed_total", state.elapsed_total))
    .await?
    .check()?;
    Ok(())
}

/// Row id: the source and the uid, so two sources never collide.
fn row_key(source: &str, uid: &str) -> String {
    format!("{source}/{uid}")
}

async fn on_changed(p: &CalendarChangedPayload, db: &Database) -> Result<(), EventError> {
    for uid in &p.removed {
        db.query("DELETE type::record('calendar_events', $id)")
            .bind(("id", row_key(&p.source, uid)))
            .await?
            .check()?;
    }
    for e in &p.upserted {
        upsert(&p.source, e, db).await?;
    }
    Ok(())
}

async fn upsert(source: &str, e: &CalendarEvent, db: &Database) -> Result<(), EventError> {
    let (start, all_day) = match &e.when {
        EventTime::Timed { start, .. } => (start.to_rfc3339(), false),
        EventTime::AllDay { start, .. } => (start.format("%Y-%m-%d").to_string(), true),
    };
    let body = serde_json::to_string(e).map_err(|err| EventError::Validation(err.to_string()))?;
    db.query(
        "UPSERT type::record('calendar_events', $id) SET
            source = $source, uid = $uid, title = $title, start = $start,
            all_day = $all_day, series_id = $series_id, body = $body",
    )
    .bind(("id", row_key(source, &e.uid)))
    .bind(("source", source.to_string()))
    .bind(("uid", e.uid.clone()))
    .bind(("title", e.title.clone()))
    .bind(("start", start))
    .bind(("all_day", all_day))
    .bind(("series_id", e.series.as_ref().map(|s| s.id.clone())))
    .bind(("body", body))
    .await?
    .check()?;
    Ok(())
}

/// A payload this build cannot read is skipped with a warning, never fatal: a newer build's
/// event must not stop every later one from applying.
fn payload<T: DeserializeOwned>(event: &Event) -> Option<T> {
    match serde_json::from_value(event.payload.clone()) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!(event_id = %event.id, kind = %event.event_type, error = %e,
                "skipping a calendar event this build cannot read");
            None
        }
    }
}
