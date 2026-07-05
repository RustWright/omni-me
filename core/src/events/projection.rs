use async_trait::async_trait;

use crate::db::Database;

use super::store::{Event, EventError, EventStore, SurrealEventStore};

/// A projection transforms events into read-optimized views.
#[async_trait]
pub trait Projection: Send + Sync {
    /// Human-readable name for this projection.
    fn name(&self) -> &str;

    /// Schema version — bump when the projection logic changes.
    fn version(&self) -> u32;

    /// Apply a single event to this projection's read tables.
    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError>;

    /// Initialize any tables/indexes this projection requires.
    async fn init_schema(&self, db: &Database) -> Result<(), EventError>;

    /// Delete all data from this projection's tables (used before rebuild).
    async fn clear_tables(&self, db: &Database) -> Result<(), EventError>;
}

/// Runs projections over events, tracking which events have been processed.
///
/// Cheaply cloneable — the projection list is shared via `Arc`, and the
/// `Database` handle is its own `Clone`-able connection pool wrapper.
#[derive(Clone)]
pub struct ProjectionRunner {
    db: Database,
    projections: std::sync::Arc<Vec<Box<dyn Projection>>>,
}

impl ProjectionRunner {
    pub fn new(db: Database, projections: Vec<Box<dyn Projection>>) -> Self {
        Self {
            db,
            projections: std::sync::Arc::new(projections),
        }
    }

    /// Initialize all projection schemas and the projection_versions tracking table.
    pub async fn init_all(&self) -> Result<(), EventError> {
        self.db
            .query(
                "DEFINE TABLE IF NOT EXISTS projection_versions SCHEMAFULL;
                 DEFINE FIELD IF NOT EXISTS name ON projection_versions TYPE string;
                 DEFINE FIELD IF NOT EXISTS version ON projection_versions TYPE int;
                 DEFINE FIELD IF NOT EXISTS last_event_id ON projection_versions TYPE string;
                 DEFINE INDEX IF NOT EXISTS idx_pv_name ON projection_versions FIELDS name UNIQUE;",
            )
            .await
?;

        for proj in self.projections.iter() {
            proj.init_schema(&self.db).await?;

            let name = proj.name().to_string();
            let version = proj.version();

            // Upsert the version record
            self.db
                .query(
                    "UPSERT projection_versions SET
                        name = $name,
                        version = $version,
                        last_event_id = last_event_id ?? ''
                     WHERE name = $name",
                )
                .bind(("name", name))
                .bind(("version", version))
                .await
    ?;
        }

        Ok(())
    }

    /// Apply a batch of events through all matching projections, **fail-fast**:
    /// the first projection error aborts the batch and propagates.
    ///
    /// This is the right semantics for **local single-event commands** (via
    /// `commands::shared::append_and_apply`): the caller is the user whose
    /// action just failed, so surfacing the error — and refusing to advance —
    /// is correct. For the **sync-pull path** use [`apply_events_resilient`]
    /// instead: there, one malformed/colliding *remote* event must not strand
    /// every later event in the batch.
    pub async fn apply_events(&self, events: &[Event]) -> Result<(), EventError> {
        for event in events {
            for proj in self.projections.iter() {
                proj.apply(event, &self.db).await?;
            }
        }

        // Update last_event_id once after all events are applied
        if let Some(last) = events.last() {
            self.advance_last_event_id(&last.id).await?;
        }

        Ok(())
    }

    /// Apply a batch **best-effort**: a single failing event is logged and
    /// skipped rather than aborting the batch. Returns the count of events that
    /// failed to apply (`0` = all clean).
    ///
    /// This is the sync-pull apply path. Two properties make it safe:
    /// - The pulled events are already **durably appended** to the event store
    ///   before this runs, so a skipped event is never *lost* — a `rebuild()`
    ///   (full replay) always recovers it, and with idempotent projections a
    ///   replay is a no-op for the events that did apply.
    /// - Not aborting means the pull cursor (already advanced past this batch)
    ///   stays consistent with "we applied everything we could," instead of the
    ///   old fail-fast behaviour where one bad event silently dropped every
    ///   later event in the batch **and** they were never re-pulled.
    pub async fn apply_events_resilient(&self, events: &[Event]) -> usize {
        let mut failed = 0usize;
        let mut last_applied: Option<&str> = None;

        for event in events {
            let mut event_ok = true;
            for proj in self.projections.iter() {
                if let Err(e) = proj.apply(event, &self.db).await {
                    tracing::warn!(
                        event_id = %event.id,
                        event_type = %event.event_type,
                        aggregate_id = %event.aggregate_id,
                        projection = proj.name(),
                        error = %e,
                        "projection apply failed during sync; skipping event (best-effort)"
                    );
                    event_ok = false;
                    // Keep going — sibling projections are independent and may
                    // still apply this event cleanly.
                }
            }
            if event_ok {
                last_applied = Some(&event.id);
            } else {
                failed += 1;
            }
        }

        // Bookkeeping only (not the sync cursor): point at the last fully-applied
        // event. Best-effort — a failure here shouldn't fail the whole apply.
        if let Some(id) = last_applied
            && let Err(e) = self.advance_last_event_id(id).await
        {
            tracing::warn!(error = %e, "failed to advance projection last_event_id after sync apply");
        }

        failed
    }

    /// Point every projection's `last_event_id` bookmark at `event_id`.
    async fn advance_last_event_id(&self, event_id: &str) -> Result<(), EventError> {
        for proj in self.projections.iter() {
            let name = proj.name().to_string();
            self.db
                .query(
                    "UPDATE projection_versions SET last_event_id = $event_id
                     WHERE name = $name",
                )
                .bind(("event_id", event_id.to_string()))
                .bind(("name", name))
                .await?;
        }
        Ok(())
    }

    /// Rebuild all projections from scratch by replaying all events.
    pub async fn rebuild(&self) -> Result<(), EventError> {
        let store = SurrealEventStore::new(self.db.clone());

        // Get all events from the beginning of time
        let epoch = chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let events = store.get_since(epoch, None).await?;

        // Clear all projection tables, then re-initialize schemas
        for proj in self.projections.iter() {
            proj.clear_tables(&self.db).await?;
            proj.init_schema(&self.db).await?;
        }

        // Replay all events
        self.apply_events(&events).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct CountingProjection {
        applied: Arc<AtomicU32>,
    }

    #[async_trait]
    impl Projection for CountingProjection {
        fn name(&self) -> &str {
            "counting"
        }

        fn version(&self) -> u32 {
            1
        }

        async fn apply(&self, _event: &Event, _db: &Database) -> Result<(), EventError> {
            self.applied.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn init_schema(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }

        async fn clear_tables(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }
    }

    /// Counts good applies but returns an error for any event whose
    /// `event_type` is `"boom"` — models a single malformed/colliding event in
    /// a pulled batch.
    struct FlakyProjection {
        applied: Arc<AtomicU32>,
    }

    #[async_trait]
    impl Projection for FlakyProjection {
        fn name(&self) -> &str {
            "flaky"
        }
        fn version(&self) -> u32 {
            1
        }
        async fn apply(&self, event: &Event, _db: &Database) -> Result<(), EventError> {
            if event.event_type == "boom" {
                return Err(EventError::Validation("boom".into()));
            }
            self.applied.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn init_schema(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }
        async fn clear_tables(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }
    }

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn ev(id: &str, event_type: &str) -> Event {
        Event {
            id: id.into(),
            event_type: event_type.into(),
            aggregate_id: "agg".into(),
            timestamp: Utc::now(),
            device_id: "d1".into(),
            payload: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn init_all_creates_version_table() {
        let db = test_db().await;
        let counter = Arc::new(AtomicU32::new(0));

        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(CountingProjection {
                applied: counter.clone(),
            })],
        );

        runner.init_all().await.unwrap();

        // Check version record exists
        let mut resp = db
            .query("SELECT * FROM projection_versions WHERE name = 'counting'")
            .await
            .unwrap();
        let name: Option<String> = resp.take("name").unwrap();
        assert_eq!(name.as_deref(), Some("counting"));
    }

    #[tokio::test]
    async fn apply_events_runs_projections() {
        let db = test_db().await;
        let counter = Arc::new(AtomicU32::new(0));

        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(CountingProjection {
                applied: counter.clone(),
            })],
        );

        runner.init_all().await.unwrap();

        let events = vec![
            Event {
                id: "e1".into(),
                event_type: "note_created".into(),
                aggregate_id: "n1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({}),
            },
            Event {
                id: "e2".into(),
                event_type: "note_updated".into(),
                aggregate_id: "n1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({}),
            },
        ];

        runner.apply_events(&events).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);

        // Verify last_event_id points to the final event
        let mut resp = db
            .query("SELECT last_event_id FROM projection_versions WHERE name = 'counting'")
            .await
            .unwrap();
        let last_id: Option<String> = resp.take("last_event_id").unwrap();
        assert_eq!(last_id.as_deref(), Some("e2"));
    }

    #[tokio::test]
    async fn resilient_apply_skips_bad_event_and_continues() {
        let db = test_db().await;
        let applied = Arc::new(AtomicU32::new(0));
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(FlakyProjection {
                applied: applied.clone(),
            })],
        );
        runner.init_all().await.unwrap();

        // Middle event errors; the two good ones on either side must still apply.
        let events = vec![ev("g1", "note_created"), ev("bad", "boom"), ev("g2", "note_updated")];
        let failed = runner.apply_events_resilient(&events).await;

        assert_eq!(failed, 1, "one event should fail");
        assert_eq!(applied.load(Ordering::SeqCst), 2, "both good events apply despite the bad one");

        // last_event_id parks on the last *successfully* applied event, not the failing tail-1.
        let mut resp = db
            .query("SELECT last_event_id FROM projection_versions WHERE name = 'flaky'")
            .await
            .unwrap();
        let last_id: Option<String> = resp.take("last_event_id").unwrap();
        assert_eq!(last_id.as_deref(), Some("g2"));
    }
}
