//! Task reads and the hub intake, shared by the server routes and the tests.

use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use super::hub::{HubIndex, HubIndexError, parse_index};
use crate::db::Database;
use crate::events::{EventError, EventType, EventWriter, NewEvent};

/// An index unchanged this long shows as one "still current?" line instead of its items:
/// ruling 92 (RULINGS.md), which makes it a setting.
pub const INDEX_STALE_DAYS: i64 = 21;

#[derive(Debug)]
pub enum IngestError {
    /// The pushed file could not be read at all.
    Parse(HubIndexError),
    Store(String),
}

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IngestError::Parse(e) => write!(f, "{e}"),
            IngestError::Store(e) => write!(f, "could not record the index: {e}"),
        }
    }
}

/// The content hash of the last index recorded for `project`.
pub async fn last_hash(db: &Database, project: &str) -> Result<Option<String>, EventError> {
    let mut resp = db
        .query("SELECT VALUE content_hash FROM type::record('hub_indexes', $p)")
        .bind(("p", project.to_string()))
        .await?
        .check()?;
    let rows: Vec<String> = resp.take(0)?;
    Ok(rows.into_iter().next())
}

/// Read a pushed index and record it, unless it is byte-for-byte the last one recorded.
/// Returns the parsed index and whether it was recorded.
pub async fn ingest(
    db: &Database,
    writer: &EventWriter,
    text: &str,
) -> Result<(HubIndex, bool), IngestError> {
    let index = parse_index(text).map_err(IngestError::Parse)?;
    let last = last_hash(db, &index.project)
        .await
        .map_err(|e| IngestError::Store(e.to_string()))?;
    if last.as_deref() == Some(index.content_hash.as_str()) {
        return Ok((index, false));
    }
    let payload = serde_json::to_value(&index).map_err(|e| IngestError::Store(e.to_string()))?;
    writer
        .append_new(NewEvent {
            id: None,
            event_type: EventType::HubIndexReceived.to_string(),
            aggregate_id: index.project.clone(),
            timestamp: Utc::now(),
            device_id: writer.device_id().to_string(),
            payload,
        })
        .await
        .map_err(|e| IngestError::Store(e.to_string()))?;
    Ok((index, true))
}

/// A hub item taken over as a native task, still awaiting its `cut` in the index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TakenOver {
    pub hub_id: String,
    pub task_id: String,
    pub title: String,
}

/// The receipt SessionStart reads: ids taken over whose project's last index does not yet
/// show them cut. Ruling 121 (RULINGS.md). `project` narrows it to one slug's ids.
pub async fn taken_over(
    db: &Database,
    project: Option<&str>,
) -> Result<Vec<TakenOver>, EventError> {
    let mut resp = db
        .query(
            "SELECT hub_id, task_id, title FROM tasks WHERE hub_id != NONE;
             SELECT VALUE hub_id FROM hub_tasks WHERE status = 'cut';",
        )
        .await?
        .check()?;
    let rows: Vec<TakenOver> = resp.take(0)?;
    let cut: Vec<String> = resp.take(1)?;
    let prefix = project.map(|p| format!("{p}-"));
    Ok(rows
        .into_iter()
        .filter(|r| !cut.contains(&r.hub_id))
        .filter(|r| prefix.as_deref().is_none_or(|p| r.hub_id.starts_with(p)))
        .collect())
}

/// Projects whose index was last reconciled more than `days` before `today`, or never dated.
pub async fn stale_indexes(
    db: &Database,
    today: NaiveDate,
    days: i64,
) -> Result<Vec<String>, EventError> {
    #[derive(SurrealValue)]
    struct Row {
        project: String,
        updated: Option<String>,
    }
    let mut resp = db
        .query("SELECT project, updated FROM hub_indexes ORDER BY project")
        .await?
        .check()?;
    let rows: Vec<Row> = resp.take(0)?;
    let cutoff = today - Duration::days(days);
    Ok(rows
        .into_iter()
        .filter(|r| {
            r.updated
                .as_deref()
                .and_then(|u| NaiveDate::parse_from_str(u, "%Y-%m-%d").ok())
                .is_none_or(|u| u < cutoff)
        })
        .map(|r| r.project)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ALL_FEATURES;
    use crate::events::{ProjectionRunner, SurrealEventStore, TasksProjection};
    use std::sync::Arc;

    async fn harness() -> (Database, EventWriter) {
        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(TasksProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(
            store,
            runner,
            ALL_FEATURES.iter().copied().collect(),
            "server",
        );
        (db, writer)
    }

    const INDEX: &str = r#"
schema = 1
project = "life-health"
updated = 2026-09-02
next_id = 3

[[task]]
id = "life-health-0001"
title = "Book the physio"
status = "todo"
size = "USER"

[[task]]
id = "life-health-0002"
title = "Renew the card"
status = "todo"
size = "USER"
"#;

    #[tokio::test]
    async fn a_push_is_recorded_once_and_a_repeat_is_skipped() {
        let (db, writer) = harness().await;
        let (idx, recorded) = ingest(&db, &writer, INDEX).await.unwrap();
        assert!(recorded);
        assert_eq!(idx.tasks.len(), 2);
        let (_, again) = ingest(&db, &writer, INDEX).await.unwrap();
        assert!(!again, "an unchanged index must not grow the log");
        let changed = INDEX.replace("Book the physio", "Book the physio, Tuesday");
        let (_, recorded) = ingest(&db, &writer, &changed).await.unwrap();
        assert!(recorded);
    }

    #[tokio::test]
    async fn a_bad_file_is_refused_with_its_reason() {
        let (db, writer) = harness().await;
        let err = ingest(&db, &writer, "schema = 1").await.unwrap_err();
        assert!(err.to_string().contains("project"), "{err}");
    }

    /// The receipt holds an id from takeover until a pushed index shows it cut.
    #[tokio::test]
    async fn the_receipt_lasts_until_the_index_shows_the_cut() {
        let (db, writer) = harness().await;
        ingest(&db, &writer, INDEX).await.unwrap();
        writer
            .append_new(NewEvent {
                id: None,
                event_type: EventType::TaskCreated.to_string(),
                aggregate_id: "01NATIVE".into(),
                timestamp: Utc::now(),
                device_id: "phone".into(),
                payload: serde_json::json!({"task_id": "01NATIVE", "title": "Book the physio",
                                            "hub_id": "life-health-0001"}),
            })
            .await
            .unwrap();
        let held = taken_over(&db, Some("life-health")).await.unwrap();
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].hub_id, "life-health-0001");
        assert!(taken_over(&db, Some("omni-me")).await.unwrap().is_empty());

        let cut = INDEX.replacen("status = \"todo\"", "status = \"cut\"", 1);
        ingest(&db, &writer, &cut).await.unwrap();
        assert!(taken_over(&db, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_index_past_the_threshold_is_stale() {
        let (db, writer) = harness().await;
        ingest(&db, &writer, INDEX).await.unwrap();
        let day = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert!(
            stale_indexes(&db, day("2026-09-20"), INDEX_STALE_DAYS)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            stale_indexes(&db, day("2026-11-21"), INDEX_STALE_DAYS)
                .await
                .unwrap(),
            vec!["life-health".to_string()]
        );
    }
}
