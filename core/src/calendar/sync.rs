//! One read of a source: fetch the window, diff it against what is recorded, append the change.
//!
//! The server runs this every 15 minutes and before every plan (ruling 113).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use super::day::{Answers, pair_key};
use super::{CalendarEvent, CalendarSource, Window};
use crate::db::Database;
use crate::events::{
    CalendarAnswer, CalendarAnsweredPayload, CalendarChangedPayload, EventType, EventWriter,
    NewEvent,
};

/// What a snapshot changes against what is recorded.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Changes {
    /// New occurrences, and ones whose fields changed, whole.
    pub upserted: Vec<CalendarEvent>,
    /// Uids recorded inside the window and missing from the snapshot.
    pub removed: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.upserted.is_empty() && self.removed.is_empty()
    }
}

/// Compares a snapshot of `window` with what is recorded for the same source.
///
/// Recorded occurrences outside the window are left alone: the past stays as history, and the
/// window moving forward is not a deletion.
pub fn diff(
    known: &[CalendarEvent],
    fetched: &[CalendarEvent],
    window: &Window,
    tz: Tz,
) -> Changes {
    let by_uid: HashMap<&str, &CalendarEvent> = known.iter().map(|e| (e.uid.as_str(), e)).collect();
    let mut fetched_uids: HashMap<&str, &CalendarEvent> = HashMap::new();
    for e in fetched {
        // A source returning one uid twice keeps its last copy.
        fetched_uids.insert(e.uid.as_str(), e);
    }
    let mut upserted: Vec<CalendarEvent> = fetched_uids
        .values()
        .filter(|e| by_uid.get(e.uid.as_str()) != Some(*e))
        .map(|e| (*e).clone())
        .collect();
    upserted.sort_by(|a, b| a.uid.cmp(&b.uid));
    let mut removed: Vec<String> = known
        .iter()
        .filter(|e| window.overlaps(e, tz) && !fetched_uids.contains_key(e.uid.as_str()))
        .map(|e| e.uid.clone())
        .collect();
    removed.sort();
    Changes { upserted, removed }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncOutcome {
    Unchanged,
    Recorded { upserted: usize, removed: usize },
}

/// Reads `source` once at `now` and records what changed. An error leaves the record as it was.
pub async fn sync_once(
    db: &Database,
    writer: &EventWriter,
    source: &dyn CalendarSource,
    now: DateTime<Utc>,
) -> Result<SyncOutcome, String> {
    let config = crate::events::load_shared(db)
        .await
        .map_err(|e| e.to_string())?;
    let tz = crate::assistant::scheduled::user_timezone(&config);
    let window = Window::around(now, tz);
    let fetched = source
        .fetch(&window)
        .await
        .map_err(|e| format!("{}: {e}", source.name()))?;
    let known = recorded(db, Some(source.name())).await?;
    let changes = diff(&known, &fetched, &window, tz);
    if changes.is_empty() {
        return Ok(SyncOutcome::Unchanged);
    }
    let outcome = SyncOutcome::Recorded {
        upserted: changes.upserted.len(),
        removed: changes.removed.len(),
    };
    let payload = CalendarChangedPayload {
        source: source.name().to_string(),
        upserted: changes.upserted,
        removed: changes.removed,
    };
    append(
        writer,
        EventType::CalendarChanged,
        payload.source.clone(),
        serde_json::to_value(&payload).map_err(|e| e.to_string())?,
    )
    .await?;
    Ok(outcome)
}

/// Records one of his answers. Later answers to the same key replace earlier ones.
pub async fn record_answer(writer: &EventWriter, answer: CalendarAnswer) -> Result<(), String> {
    let payload = CalendarAnsweredPayload { answer };
    let key = payload.answer.key();
    append(
        writer,
        EventType::CalendarAnswered,
        key,
        serde_json::to_value(&payload).map_err(|e| e.to_string())?,
    )
    .await
}

#[derive(SurrealValue)]
struct EventRow {
    body: String,
}

/// Every recorded occurrence, for one source or all of them.
pub async fn recorded(db: &Database, source: Option<&str>) -> Result<Vec<CalendarEvent>, String> {
    let mut resp = match source {
        Some(s) => {
            db.query("SELECT body FROM calendar_events WHERE source = $s")
                .bind(("s", s.to_string()))
                .await
        }
        None => db.query("SELECT body FROM calendar_events").await,
    }
    .map_err(|e| e.to_string())?;
    let rows: Vec<EventRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|r| serde_json::from_str(&r.body).ok())
        .collect())
}

#[derive(SurrealValue)]
struct AnswerRow {
    body: String,
}

/// Every answer he has given, folded into lookups.
pub async fn answers(db: &Database) -> Result<Answers, String> {
    let mut resp = db
        .query("SELECT body FROM calendar_answers")
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<AnswerRow> = resp.take(0).map_err(|e| e.to_string())?;
    let mut out = Answers::default();
    for row in rows {
        match serde_json::from_str::<CalendarAnswer>(&row.body) {
            Ok(CalendarAnswer::TakesDay { series, takes_day }) => {
                out.takes_day.insert(series, takes_day);
            }
            Ok(CalendarAnswer::Place { series, place }) => {
                out.place.insert(series, place);
            }
            Ok(CalendarAnswer::Clash {
                day,
                a,
                b,
                going_to,
            }) => {
                out.clash.insert((day, pair_key(&a, &b)), going_to);
            }
            Err(e) => {
                tracing::warn!(error = %e, "skipping a calendar answer this build cannot read")
            }
        }
    }
    Ok(out)
}

async fn append(
    writer: &EventWriter,
    kind: EventType,
    aggregate_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    writer
        .append_new(NewEvent {
            id: None,
            event_type: kind.to_string(),
            aggregate_id,
            timestamp: Utc::now(),
            device_id: writer.device_id().to_string(),
            payload,
        })
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::super::test_support::*;
    use super::*;
    use crate::config::ALL_FEATURES;
    use crate::events::{CalendarProjection, ProjectionRunner, SurrealEventStore};

    const TZ: Tz = Tz::UTC;

    fn window() -> Window {
        Window {
            from: "2026-11-21T00:00:00Z".parse().unwrap(),
            to: "2026-12-01T00:00:00Z".parse().unwrap(),
        }
    }

    #[test]
    fn diff_reports_only_what_changed() {
        let a = timed(
            "a",
            "Lecture",
            "2026-11-23T10:00:00Z",
            "2026-11-23T11:00:00Z",
        );
        let b = timed("b", "Lab", "2026-11-24T10:00:00Z", "2026-11-24T11:00:00Z");
        let mut b_moved = b.clone();
        b_moved.title = "Lab (moved)".into();
        let c = timed("c", "New", "2026-11-25T10:00:00Z", "2026-11-25T11:00:00Z");
        let past = timed("p", "Old", "2026-11-01T10:00:00Z", "2026-11-01T11:00:00Z");

        let known = vec![a.clone(), b, past];
        let fetched = vec![a, b_moved.clone(), c.clone()];
        let changes = diff(&known, &fetched, &window(), TZ);
        assert_eq!(changes.upserted, vec![b_moved, c]);
        assert!(
            changes.removed.is_empty(),
            "the past event is outside the window"
        );
    }

    #[test]
    fn a_vanished_event_inside_the_window_is_removed() {
        let a = timed(
            "a",
            "Lecture",
            "2026-11-23T10:00:00Z",
            "2026-11-23T11:00:00Z",
        );
        let changes = diff(&[a], &[], &window(), TZ);
        assert_eq!(changes.removed, vec!["a".to_string()]);
        assert!(changes.upserted.is_empty());
    }

    #[test]
    fn an_unchanged_snapshot_is_empty() {
        let a = timed(
            "a",
            "Lecture",
            "2026-11-23T10:00:00Z",
            "2026-11-23T11:00:00Z",
        );
        let same = [a];
        assert!(diff(&same, &same, &window(), TZ).is_empty());
    }

    struct Fake {
        events: Mutex<Result<Vec<CalendarEvent>, String>>,
    }

    #[async_trait]
    impl CalendarSource for Fake {
        fn name(&self) -> &str {
            "fake"
        }
        async fn fetch(&self, _window: &Window) -> Result<Vec<CalendarEvent>, String> {
            self.events.lock().unwrap().clone()
        }
    }

    async fn harness() -> (Database, EventWriter) {
        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(CalendarProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(
            store,
            runner,
            ALL_FEATURES.iter().copied().collect(),
            "server",
        );
        (db, writer)
    }

    #[tokio::test]
    async fn sync_records_changes_once_and_survives_a_failed_read() {
        let (db, writer) = harness().await;
        let now: DateTime<Utc> = "2026-11-22T12:00:00Z".parse().unwrap();
        let start = (now + chrono::Duration::days(1)).to_rfc3339();
        let end = (now + chrono::Duration::days(1) + chrono::Duration::hours(1)).to_rfc3339();
        let lecture = timed("a", "Lecture", &start, &end);
        let source = Fake {
            events: Mutex::new(Ok(vec![lecture.clone()])),
        };

        let first = sync_once(&db, &writer, &source, now).await.unwrap();
        assert_eq!(
            first,
            SyncOutcome::Recorded {
                upserted: 1,
                removed: 0
            }
        );
        let again = sync_once(&db, &writer, &source, now).await.unwrap();
        assert_eq!(again, SyncOutcome::Unchanged);
        assert_eq!(recorded(&db, None).await.unwrap(), vec![lecture.clone()]);

        *source.events.lock().unwrap() = Err("offline".into());
        assert!(sync_once(&db, &writer, &source, now).await.is_err());
        assert_eq!(recorded(&db, None).await.unwrap(), vec![lecture]);

        *source.events.lock().unwrap() = Ok(vec![]);
        let gone = sync_once(&db, &writer, &source, now).await.unwrap();
        assert_eq!(
            gone,
            SyncOutcome::Recorded {
                upserted: 0,
                removed: 1
            }
        );
        assert!(recorded(&db, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_latest_answer_wins() {
        let (db, writer) = harness().await;
        for place in ["Home", "Campus"] {
            record_answer(
                &writer,
                CalendarAnswer::Place {
                    series: "club".into(),
                    place: place.into(),
                },
            )
            .await
            .unwrap();
        }
        let day = "2026-11-23".parse().unwrap();
        record_answer(
            &writer,
            CalendarAnswer::Clash {
                day,
                a: "x".into(),
                b: "y".into(),
                going_to: "y".into(),
            },
        )
        .await
        .unwrap();
        let answers = answers(&db).await.unwrap();
        assert_eq!(
            answers.place.get("club").map(String::as_str),
            Some("Campus")
        );
        assert_eq!(
            answers
                .clash
                .get(&(day, pair_key("y", "x")))
                .map(String::as_str),
            Some("y")
        );
    }
}
