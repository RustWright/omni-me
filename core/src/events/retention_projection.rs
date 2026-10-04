//! SurrealDB projection over `DocumentRetentionSet` — how long a tag's documents
//! are kept.
//!
//! One table, `document_retention`, one row per tag. What reads it, and why an
//! absent rule means kept rather than unconfigured: [`crate::retention`].

use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::DocumentRetentionSetPayload;

pub struct RetentionProjection;

impl RetentionProjection {
    pub const NAME: &'static str = "document_retention";
}

#[async_trait]
impl Projection for RetentionProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS document_retention SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS tag ON document_retention TYPE string;
             DEFINE FIELD IF NOT EXISTS keep_days ON document_retention TYPE option<int>;
             DEFINE FIELD IF NOT EXISTS updated_at ON document_retention TYPE datetime;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query("DELETE FROM document_retention").await?.check()?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "document_retention_set" => self.on_set(event, db).await,
            _ => Ok(()),
        }
    }
}

impl RetentionProjection {
    async fn on_set(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // A skip, never an error, on `ConfigProjection`'s reasoning: a rule this
        // build cannot read was written by another one, and failing the batch
        // would take unrelated edits down with it to protect a setting.
        let parsed: DocumentRetentionSetPayload =
            match serde_json::from_value(event.payload.clone()) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        event_id = %event.id,
                        aggregate_id = %event.aggregate_id,
                        error = %e,
                        "skipping a retention rule this build cannot read"
                    );
                    return Ok(());
                }
            };
        let tag = parsed.tag.trim().to_lowercase();
        if tag.is_empty() {
            tracing::warn!(event_id = %event.id, "skipping a retention rule with no tag");
            return Ok(());
        }

        // Ordered by authoring time, not arrival: two devices must land on the
        // same rule whatever order sync delivered their events in. The same
        // argument `ConfigProjection` makes, and for a setting nobody edits again
        // it is the only thing that reconciles them.
        let mut existing = db
            .query(
                "SELECT <string> updated_at AS updated_at
                 FROM type::record('document_retention', $tag) LIMIT 1",
            )
            .bind(("tag", tag.clone()))
            .await?;
        let stored: Option<String> = existing.take("updated_at").unwrap_or(None);
        if let Some(stored) = stored
            && let Ok(stored_ts) = chrono::DateTime::parse_from_rfc3339(&stored)
            && stored_ts.with_timezone(&chrono::Utc) >= event.timestamp
        {
            return Ok(());
        }

        // A cleared rule keeps its row with `keep_days = NONE`, for the reason
        // `app_config` keeps a cleared key: deleting loses the timestamp this
        // guard compares against, so a stale set arriving afterwards would find
        // nothing and resurrect the old number.
        db.query(
            "UPSERT type::record('document_retention', $tag) SET
                tag = $tag,
                keep_days = $keep_days,
                updated_at = type::datetime($ts)",
        )
        .bind(("tag", tag))
        .bind(("keep_days", parsed.keep_days.map(i64::from)))
        .bind(("ts", event.timestamp.to_rfc3339()))
        .await?
        .check()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;
    use crate::events::{EventStore, types::DocumentRetentionSetPayload};
    use crate::events::{NewEvent, ProjectionRunner, SurrealEventStore};

    use surrealdb::types::SurrealValue;

    #[derive(Debug, SurrealValue)]
    struct Row {
        keep_days: Option<i64>,
    }

    async fn rule(db: &Database, tag: &str) -> Option<Option<i64>> {
        let mut resp = db
            .query("SELECT keep_days FROM type::record('document_retention', $tag)")
            .bind(("tag", tag.to_string()))
            .await
            .unwrap();
        let rows: Vec<Row> = resp.take(0).unwrap();
        rows.into_iter().next().map(|r| r.keep_days)
    }

    async fn apply(db: &Database, runner: &ProjectionRunner, tag: &str, keep_days: Option<u32>) {
        let store = SurrealEventStore::new(db.clone());
        let payload = DocumentRetentionSetPayload {
            tag: tag.to_string(),
            keep_days,
        };
        let e = store
            .append(NewEvent::document_retention_set("d1", &payload).unwrap())
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();
    }

    #[tokio::test]
    async fn a_rule_lands_and_clearing_it_keeps_the_row() {
        let db = test_db().await;
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RetentionProjection)]);
        runner.init_all().await.unwrap();

        assert_eq!(rule(&db, "newsletter").await, None, "ungoverned has no row");

        apply(&db, &runner, "newsletter", Some(90)).await;
        assert_eq!(rule(&db, "newsletter").await, Some(Some(90)));

        // ⛔ The row survives the clear. Deleting it would lose the timestamp the
        // ordering guard reads, and a stale `set` would then resurrect 90 days.
        apply(&db, &runner, "newsletter", None).await;
        assert_eq!(
            rule(&db, "newsletter").await,
            Some(None),
            "cleared keeps a row with no number"
        );
    }

    #[tokio::test]
    async fn a_tag_is_normalized_so_two_spellings_are_one_rule() {
        let db = test_db().await;
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RetentionProjection)]);
        runner.init_all().await.unwrap();

        apply(&db, &runner, "  Newsletter ", Some(30)).await;
        assert_eq!(
            rule(&db, "newsletter").await,
            Some(Some(30)),
            "trimmed and lowercased, like the tags themselves"
        );
    }

    #[tokio::test]
    async fn an_older_rule_arriving_late_does_not_win() {
        let db = test_db().await;
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RetentionProjection)]);
        runner.init_all().await.unwrap();
        let store = SurrealEventStore::new(db.clone());

        let payload = DocumentRetentionSetPayload {
            tag: "newsletter".into(),
            keep_days: Some(90),
        };
        let mut newer = NewEvent::document_retention_set("d1", &payload).unwrap();
        newer.timestamp = chrono::Utc::now();
        let newer = store.append(newer).await.unwrap();

        let stale_payload = DocumentRetentionSetPayload {
            tag: "newsletter".into(),
            keep_days: Some(7),
        };
        let mut older = NewEvent::document_retention_set("d2", &stale_payload).unwrap();
        older.timestamp = chrono::Utc::now() - chrono::Duration::hours(1);
        let older = store.append(older).await.unwrap();

        // Applied newest-first on purpose: sync order is arrival order, and the
        // rule has to be the same on every device regardless.
        runner.apply_events(&[newer]).await.unwrap();
        runner.apply_events(&[older]).await.unwrap();
        assert_eq!(
            rule(&db, "newsletter").await,
            Some(Some(90)),
            "the later authoring time wins, not the later arrival"
        );
    }
}
