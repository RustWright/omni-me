use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::calendar::{CalendarEvent, EventTime};
use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::{
    CalendarAnsweredPayload, CalendarChangedPayload, PlaceSetPayload, TravelTimeSetPayload,
};

/// Projection over the calendar as read in, his answers about it, and his places.
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

    fn version(&self) -> u32 {
        1
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
             DEFINE FIELD IF NOT EXISTS minutes ON planning_travel TYPE int;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DELETE FROM calendar_events; DELETE FROM calendar_answers;
             DELETE FROM planning_places; DELETE FROM planning_travel;",
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
            _ => Ok(()),
        }
    }
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
