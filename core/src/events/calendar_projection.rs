use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::calendar::{CalendarEvent, EventTime};
use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::{CalendarAnsweredPayload, CalendarChangedPayload};

/// Projection over the calendar as read in, and his answers about it.
///
/// Each row keeps the whole value as JSON in `body`, read back as it was written; the other
/// columns are for filtering and order.
pub struct CalendarProjection;

impl CalendarProjection {
    pub const NAME: &'static str = "calendar";
}

#[async_trait]
impl Projection for CalendarProjection {
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
             DEFINE FIELD IF NOT EXISTS answered_at ON calendar_answers TYPE datetime;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query("DELETE FROM calendar_events; DELETE FROM calendar_answers;")
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
