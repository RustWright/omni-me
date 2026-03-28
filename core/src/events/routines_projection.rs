use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};

/// Projection that maintains routine_groups, routine_items, and routine_completions tables.
pub struct RoutinesProjection;

#[async_trait]
impl Projection for RoutinesProjection {
    fn name(&self) -> &str {
        "routines"
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS routine_groups SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS name ON routine_groups TYPE string;
             DEFINE FIELD IF NOT EXISTS frequency ON routine_groups TYPE string;
             DEFINE FIELD IF NOT EXISTS time_of_day ON routine_groups TYPE string;
             DEFINE FIELD IF NOT EXISTS created_at ON routine_groups TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON routine_groups TYPE datetime;

             DEFINE TABLE IF NOT EXISTS routine_items SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS group_id ON routine_items TYPE string;
             DEFINE FIELD IF NOT EXISTS name ON routine_items TYPE string;
             DEFINE FIELD IF NOT EXISTS estimated_duration_min ON routine_items TYPE int;
             DEFINE FIELD IF NOT EXISTS order_num ON routine_items TYPE int;

             DEFINE TABLE IF NOT EXISTS routine_completions SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS item_id ON routine_completions TYPE string;
             DEFINE FIELD IF NOT EXISTS group_id ON routine_completions TYPE string;
             DEFINE FIELD IF NOT EXISTS date ON routine_completions TYPE string;
             DEFINE FIELD IF NOT EXISTS completed_at ON routine_completions TYPE datetime;
             DEFINE FIELD IF NOT EXISTS skipped ON routine_completions TYPE bool;
             DEFINE FIELD IF NOT EXISTS reason ON routine_completions TYPE option<string>;",
        )
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "routine_group_created" => self.on_group_created(event, db).await,
            "routine_item_added" => self.on_item_added(event, db).await,
            "routine_item_completed" => self.on_item_completed(event, db).await,
            "routine_item_skipped" => self.on_item_skipped(event, db).await,
            "routine_group_modified" => self.on_group_modified(event, db).await,
            _ => Ok(()),
        }
    }
}

impl RoutinesProjection {
    async fn on_group_created(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let name = event.payload["name"].as_str().unwrap_or_default().to_string();
        let frequency = event.payload["frequency"].as_str().unwrap_or_default().to_string();
        let time_of_day = event.payload["time_of_day"].as_str().unwrap_or_default().to_string();
        let group_id = event.aggregate_id.clone();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "CREATE type::record('routine_groups', $group_id) CONTENT {
                name: $name,
                frequency: $frequency,
                time_of_day: $time_of_day,
                created_at: type::datetime($ts),
                updated_at: type::datetime($ts)
            }",
        )
        .bind(("group_id", group_id))
        .bind(("name", name))
        .bind(("frequency", frequency))
        .bind(("time_of_day", time_of_day))
        .bind(("ts", ts))
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }

    async fn on_item_added(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let group_id = event.payload["group_id"].as_str().unwrap_or_default().to_string();
        let name = event.payload["name"].as_str().unwrap_or_default().to_string();
        let duration = event.payload["estimated_duration_min"]
            .as_u64()
            .unwrap_or(0) as u32;
        let order = event.payload["order"].as_u64().unwrap_or(0) as u32;
        let item_id = event.aggregate_id.clone();

        db.query(
            "CREATE type::record('routine_items', $item_id) CONTENT {
                group_id: $group_id,
                name: $name,
                estimated_duration_min: $duration,
                order_num: $order_num
            }",
        )
        .bind(("item_id", item_id))
        .bind(("group_id", group_id))
        .bind(("name", name))
        .bind(("duration", duration))
        .bind(("order_num", order))
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }

    async fn on_item_completed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let item_id = event.payload["item_id"].as_str().unwrap_or_default().to_string();
        let group_id = event.payload["group_id"].as_str().unwrap_or_default().to_string();
        let date = event.payload["date"].as_str().unwrap_or_default().to_string();
        let completed_at = event.payload["completed_at"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| event.timestamp.to_rfc3339());
        let completion_id = ulid::Ulid::new().to_string();

        db.query(
            "CREATE type::record('routine_completions', $completion_id) CONTENT {
                item_id: $item_id,
                group_id: $group_id,
                date: $date,
                completed_at: type::datetime($completed_at),
                skipped: false,
                reason: NONE
            }",
        )
        .bind(("completion_id", completion_id))
        .bind(("item_id", item_id))
        .bind(("group_id", group_id))
        .bind(("date", date))
        .bind(("completed_at", completed_at))
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }

    async fn on_item_skipped(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let item_id = event.payload["item_id"].as_str().unwrap_or_default().to_string();
        let group_id = event.payload["group_id"].as_str().unwrap_or_default().to_string();
        let date = event.payload["date"].as_str().unwrap_or_default().to_string();
        let reason = event.payload["reason"].as_str().map(String::from);
        let completion_id = ulid::Ulid::new().to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "CREATE type::record('routine_completions', $completion_id) CONTENT {
                item_id: $item_id,
                group_id: $group_id,
                date: $date,
                completed_at: type::datetime($ts),
                skipped: true,
                reason: $reason
            }",
        )
        .bind(("completion_id", completion_id))
        .bind(("item_id", item_id))
        .bind(("group_id", group_id))
        .bind(("date", date))
        .bind(("ts", ts))
        .bind(("reason", reason))
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }

    async fn on_group_modified(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let group_id = event.payload["group_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        // Apply each field from changes to the group
        let changes = &event.payload["changes"];
        if let Some(name) = changes.get("name").and_then(|v| v.as_str()) {
            let name = name.to_string();
            db.query(
                "UPDATE type::record('routine_groups', $group_id) SET name = $name",
            )
            .bind(("group_id", group_id.clone()))
            .bind(("name", name))
            .await
            .map_err(|e| EventError::Projection(e.to_string()))?;
        }
        if let Some(frequency) = changes.get("frequency").and_then(|v| v.as_str()) {
            let frequency = frequency.to_string();
            db.query(
                "UPDATE type::record('routine_groups', $group_id) SET frequency = $frequency",
            )
            .bind(("group_id", group_id.clone()))
            .bind(("frequency", frequency))
            .await
            .map_err(|e| EventError::Projection(e.to_string()))?;
        }
        if let Some(time_of_day) = changes.get("time_of_day").and_then(|v| v.as_str()) {
            let time_of_day = time_of_day.to_string();
            db.query(
                "UPDATE type::record('routine_groups', $group_id) SET time_of_day = $time_of_day",
            )
            .bind(("group_id", group_id.clone()))
            .bind(("time_of_day", time_of_day))
            .await
            .map_err(|e| EventError::Projection(e.to_string()))?;
        }

        // Always update the updated_at timestamp
        db.query(
            "UPDATE type::record('routine_groups', $group_id) SET updated_at = type::datetime($ts)",
        )
        .bind(("group_id", group_id))
        .bind(("ts", ts))
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::projection::ProjectionRunner;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use chrono::Utc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    #[tokio::test]
    async fn routine_group_created_and_item_added() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        let e1 = store
            .append(NewEvent {
                event_type: "routine_group_created".into(),
                aggregate_id: "morning".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "name": "Morning Routine",
                    "frequency": "daily",
                    "time_of_day": "morning"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                event_type: "routine_item_added".into(),
                aggregate_id: "item-1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "group_id": "morning",
                    "name": "Meditation",
                    "estimated_duration_min": 10,
                    "order": 1
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('routine_groups', 'morning')")
            .await
            .unwrap();
        let name: Option<String> = resp.take("name").unwrap();
        assert_eq!(name.as_deref(), Some("Morning Routine"));

        let mut resp = db
            .query("SELECT * FROM type::record('routine_items', 'item-1')")
            .await
            .unwrap();
        let item_name: Option<String> = resp.take("name").unwrap();
        assert_eq!(item_name.as_deref(), Some("Meditation"));
    }

    #[tokio::test]
    async fn routine_item_completed_and_skipped() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        let e1 = store
            .append(NewEvent {
                event_type: "routine_item_completed".into(),
                aggregate_id: "completion-1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "item_id": "item-1",
                    "group_id": "morning",
                    "date": "2026-03-27",
                    "completed_at": "2026-03-27T07:30:00Z"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                event_type: "routine_item_skipped".into(),
                aggregate_id: "completion-2".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "item_id": "item-2",
                    "group_id": "morning",
                    "date": "2026-03-27",
                    "reason": "Feeling sick"
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT count() AS total FROM routine_completions GROUP ALL")
            .await
            .unwrap();
        let count: Option<u32> = resp.take("total").unwrap();
        assert_eq!(count, Some(2));

        let mut resp = db
            .query("SELECT count() AS total FROM routine_completions WHERE skipped = true GROUP ALL")
            .await
            .unwrap();
        let skipped: Option<u32> = resp.take("total").unwrap();
        assert_eq!(skipped, Some(1));
    }

    #[tokio::test]
    async fn routine_group_modified() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        let e1 = store
            .append(NewEvent {
                event_type: "routine_group_created".into(),
                aggregate_id: "evening".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "name": "Evening Routine",
                    "frequency": "daily",
                    "time_of_day": "evening"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                event_type: "routine_group_modified".into(),
                aggregate_id: "evening".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "group_id": "evening",
                    "changes": { "name": "Night Routine" },
                    "justification": "Renamed for clarity"
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('routine_groups', 'evening')")
            .await
            .unwrap();
        let name: Option<String> = resp.take("name").unwrap();
        assert_eq!(name.as_deref(), Some("Night Routine"));
    }
}
