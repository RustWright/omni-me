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
}

/// Runs projections over events, tracking which events have been processed.
pub struct ProjectionRunner {
    db: Database,
    projections: Vec<Box<dyn Projection>>,
}

impl ProjectionRunner {
    pub fn new(db: Database, projections: Vec<Box<dyn Projection>>) -> Self {
        Self { db, projections }
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
            .map_err(|e| EventError::Projection(e.to_string()))?;

        for proj in &self.projections {
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
                .map_err(|e| EventError::Projection(e.to_string()))?;
        }

        Ok(())
    }

    /// Apply a batch of events through all matching projections.
    pub async fn apply_events(&self, events: &[Event]) -> Result<(), EventError> {
        for event in events {
            for proj in &self.projections {
                proj.apply(event, &self.db).await?;
            }

            // Update last_event_id for all projections
            let event_id = event.id.clone();
            for proj in &self.projections {
                let name = proj.name().to_string();
                self.db
                    .query(
                        "UPDATE projection_versions SET last_event_id = $event_id
                         WHERE name = $name",
                    )
                    .bind(("event_id", event_id.clone()))
                    .bind(("name", name))
                    .await
                    .map_err(|e| EventError::Projection(e.to_string()))?;
            }
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

        // Re-initialize schemas (clears read tables)
        for proj in &self.projections {
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
    }

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
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
    }
}
