use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};

/// Projection over routine events.
///
/// Tables:
/// - `routine_groups` — user-ordered list. `removed` tristates: false = active,
///   true = soft-deleted (hidden but history preserved).
/// - `routine_items` — group-owned checklist items, also soft-deleted via `removed`.
/// - `routine_completions` — one row per completion/skip. Undo deletes the row
///   outright (leaves no ghost record), keeping the "completed today?" check
///   trivial: any row with matching (item_id, date) means done.
pub struct RoutinesProjection;

#[async_trait]
impl Projection for RoutinesProjection {
    fn name(&self) -> &str {
        "routines"
    }

    fn version(&self) -> u32 {
        2
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS routine_groups SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS name ON routine_groups TYPE string;
             DEFINE FIELD IF NOT EXISTS frequency ON routine_groups TYPE string;
             DEFINE FIELD IF NOT EXISTS order_num ON routine_groups TYPE int;
             DEFINE FIELD IF NOT EXISTS removed ON routine_groups TYPE bool;
             DEFINE FIELD IF NOT EXISTS created_at ON routine_groups TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON routine_groups TYPE datetime;

             DEFINE TABLE IF NOT EXISTS routine_items SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS group_id ON routine_items TYPE string;
             DEFINE FIELD IF NOT EXISTS name ON routine_items TYPE string;
             DEFINE FIELD IF NOT EXISTS estimated_duration_min ON routine_items TYPE int;
             DEFINE FIELD IF NOT EXISTS order_num ON routine_items TYPE int;
             DEFINE FIELD IF NOT EXISTS removed ON routine_items TYPE bool;

             DEFINE TABLE IF NOT EXISTS routine_completions SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS item_id ON routine_completions TYPE string;
             DEFINE FIELD IF NOT EXISTS group_id ON routine_completions TYPE string;
             DEFINE FIELD IF NOT EXISTS date ON routine_completions TYPE string;
             DEFINE FIELD IF NOT EXISTS completed_at ON routine_completions TYPE datetime;
             DEFINE FIELD IF NOT EXISTS skipped ON routine_completions TYPE bool;
             DEFINE FIELD IF NOT EXISTS reason ON routine_completions TYPE option<string>;",
        )
        .await?;

        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DELETE FROM routine_groups;
             DELETE FROM routine_items;
             DELETE FROM routine_completions;",
        )
        .await?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "routine_group_created" => self.on_group_created(event, db).await,
            "routine_group_reordered" => self.on_group_reordered(event, db).await,
            "routine_group_removed" => self.on_group_removed(event, db).await,
            "routine_item_added" => self.on_item_added(event, db).await,
            "routine_item_modified" => self.on_item_modified(event, db).await,
            "routine_item_removed" => self.on_item_removed(event, db).await,
            "routine_item_completed" => self.on_item_completed(event, db).await,
            "routine_item_completion_undone" => self.on_completion_undone(event, db, false).await,
            "routine_item_skipped" => self.on_item_skipped(event, db).await,
            "routine_item_skip_undone" => self.on_completion_undone(event, db, true).await,
            _ => Ok(()),
        }
    }
}

impl RoutinesProjection {
    async fn on_group_created(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let name = event.payload["name"].as_str().unwrap_or_default().to_string();
        let frequency = event.payload["frequency"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let order = event.payload["order"].as_u64().unwrap_or(0) as i64;
        let group_id = event.aggregate_id.clone();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "CREATE type::record('routine_groups', $group_id) CONTENT {
                name: $name,
                frequency: $frequency,
                order_num: $order_num,
                removed: false,
                created_at: type::datetime($ts),
                updated_at: type::datetime($ts)
            }",
        )
        .bind(("group_id", group_id))
        .bind(("name", name))
        .bind(("frequency", frequency))
        .bind(("order_num", order))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_group_reordered(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let ts = event.timestamp.to_rfc3339();
        let empty = Vec::new();
        let orderings = event.payload["orderings"].as_array().unwrap_or(&empty);

        for entry in orderings {
            let group_id = entry["group_id"].as_str().unwrap_or_default().to_string();
            let order = entry["order"].as_u64().unwrap_or(0) as i64;

            db.query(
                "UPDATE type::record('routine_groups', $group_id) SET
                    order_num = $order_num,
                    updated_at = type::datetime($ts)",
            )
            .bind(("group_id", group_id))
            .bind(("order_num", order))
            .bind(("ts", ts.clone()))
            .await?;
        }

        Ok(())
    }

    async fn on_group_removed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let group_id = event.payload["group_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('routine_groups', $group_id) SET
                removed = true,
                updated_at = type::datetime($ts)",
        )
        .bind(("group_id", group_id))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_item_added(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let group_id = event.payload["group_id"].as_str().unwrap_or_default().to_string();
        let name = event.payload["name"].as_str().unwrap_or_default().to_string();
        let duration = event.payload["estimated_duration_min"]
            .as_u64()
            .unwrap_or(0) as i64;
        let order = event.payload["order"].as_u64().unwrap_or(0) as i64;
        let item_id = event.aggregate_id.clone();

        db.query(
            "CREATE type::record('routine_items', $item_id) CONTENT {
                group_id: $group_id,
                name: $name,
                estimated_duration_min: $duration,
                order_num: $order_num,
                removed: false
            }",
        )
        .bind(("item_id", item_id))
        .bind(("group_id", group_id))
        .bind(("name", name))
        .bind(("duration", duration))
        .bind(("order_num", order))
        .await?;

        Ok(())
    }

    async fn on_item_modified(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let item_id = event.payload["item_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let changes = &event.payload["changes"];

        if let Some(name) = changes.get("name").and_then(|v| v.as_str()) {
            db.query("UPDATE type::record('routine_items', $item_id) SET name = $name")
                .bind(("item_id", item_id.clone()))
                .bind(("name", name.to_string()))
                .await?;
        }
        if let Some(duration) = changes
            .get("estimated_duration_min")
            .and_then(|v| v.as_u64())
        {
            db.query(
                "UPDATE type::record('routine_items', $item_id) SET estimated_duration_min = $duration",
            )
            .bind(("item_id", item_id.clone()))
            .bind(("duration", duration as i64))
            .await?;
        }
        if let Some(order) = changes.get("order").and_then(|v| v.as_u64()) {
            db.query("UPDATE type::record('routine_items', $item_id) SET order_num = $order_num")
                .bind(("item_id", item_id.clone()))
                .bind(("order_num", order as i64))
                .await?;
        }

        Ok(())
    }

    async fn on_item_removed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let item_id = event.payload["item_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();

        db.query("UPDATE type::record('routine_items', $item_id) SET removed = true")
            .bind(("item_id", item_id))
            .await?;

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
        let completion_id = completion_key(&item_id, &date, false);

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
        .await?;

        Ok(())
    }

    async fn on_item_skipped(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let item_id = event.payload["item_id"].as_str().unwrap_or_default().to_string();
        let group_id = event.payload["group_id"].as_str().unwrap_or_default().to_string();
        let date = event.payload["date"].as_str().unwrap_or_default().to_string();
        let reason = event.payload["reason"].as_str().map(String::from);
        let completion_id = completion_key(&item_id, &date, true);
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
        .await?;

        Ok(())
    }

    async fn on_completion_undone(
        &self,
        event: &Event,
        db: &Database,
        skipped: bool,
    ) -> Result<(), EventError> {
        let item_id = event.payload["item_id"].as_str().unwrap_or_default().to_string();
        let date = event.payload["date"].as_str().unwrap_or_default().to_string();
        let completion_id = completion_key(&item_id, &date, skipped);

        db.query("DELETE type::record('routine_completions', $completion_id)")
            .bind(("completion_id", completion_id))
            .await?;

        Ok(())
    }
}

/// Deterministic record id for a completion — lets undo delete the exact row
/// without scanning. One complete row + one skip row per (item, date) maximum.
fn completion_key(item_id: &str, date: &str, skipped: bool) -> String {
    let kind = if skipped { "skip" } else { "done" };
    format!("{item_id}-{date}-{kind}")
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
    async fn group_created_with_order_no_time_of_day() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        let e = store
            .append(NewEvent {
                id: None,
                event_type: "routine_group_created".into(),
                aggregate_id: "morning".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "name": "Morning",
                    "frequency": "daily",
                    "order": 0
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e]).await.unwrap();

        let mut resp = db
            .query("SELECT order_num, removed FROM type::record('routine_groups', 'morning')")
            .await
            .unwrap();
        let order_num: Option<i64> = resp.take("order_num").unwrap();
        let removed: Option<bool> = resp.take("removed").unwrap();
        assert_eq!(order_num, Some(0));
        assert_eq!(removed, Some(false));
    }

    #[tokio::test]
    async fn group_reordered_updates_multiple_groups() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        for (name, order) in [("a", 0), ("b", 1)] {
            let e = store
                .append(NewEvent {
                    id: None,
                    event_type: "routine_group_created".into(),
                    aggregate_id: name.into(),
                    timestamp: Utc::now(),
                    device_id: "d1".into(),
                    payload: serde_json::json!({
                        "name": name, "frequency": "daily", "order": order
                    }),
                })
                .await
                .unwrap();
            runner.apply_events(&[e]).await.unwrap();
        }

        let e = store
            .append(NewEvent {
                id: None,
                event_type: "routine_group_reordered".into(),
                aggregate_id: "reorder".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "orderings": [
                        { "group_id": "a", "order": 1 },
                        { "group_id": "b", "order": 0 }
                    ]
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();

        let mut resp = db
            .query("SELECT order_num FROM type::record('routine_groups', 'a')")
            .await
            .unwrap();
        let order_a: Option<i64> = resp.take("order_num").unwrap();
        assert_eq!(order_a, Some(1));

        let mut resp = db
            .query("SELECT order_num FROM type::record('routine_groups', 'b')")
            .await
            .unwrap();
        let order_b: Option<i64> = resp.take("order_num").unwrap();
        assert_eq!(order_b, Some(0));
    }

    #[tokio::test]
    async fn item_modified_partial_changes() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "routine_item_added".into(),
                aggregate_id: "i1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "group_id": "g1", "name": "Stretch", "estimated_duration_min": 5, "order": 0
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "routine_item_modified".into(),
                aggregate_id: "i1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "item_id": "i1",
                    "changes": { "name": "Stretch Deeper", "estimated_duration_min": 10 }
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT name, estimated_duration_min FROM type::record('routine_items', 'i1')")
            .await
            .unwrap();
        let name: Option<String> = resp.take("name").unwrap();
        let dur: Option<i64> = resp.take("estimated_duration_min").unwrap();
        assert_eq!(name.as_deref(), Some("Stretch Deeper"));
        assert_eq!(dur, Some(10));
    }

    #[tokio::test]
    async fn completion_and_undo() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        let complete = store
            .append(NewEvent {
                id: None,
                event_type: "routine_item_completed".into(),
                aggregate_id: "completion-1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "item_id": "i1", "group_id": "g1",
                    "date": "2026-04-19", "completed_at": "2026-04-19T09:00:00Z"
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[complete]).await.unwrap();

        let mut resp = db
            .query("SELECT count() AS total FROM routine_completions GROUP ALL")
            .await
            .unwrap();
        let before: Option<u32> = resp.take("total").unwrap();
        assert_eq!(before, Some(1));

        let undo = store
            .append(NewEvent {
                id: None,
                event_type: "routine_item_completion_undone".into(),
                aggregate_id: "undo-1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "item_id": "i1", "date": "2026-04-19"
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[undo]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM routine_completions")
            .await
            .unwrap();
        let rows: Vec<serde_json::Value> = resp.take(0).unwrap();
        assert!(rows.is_empty(), "undo removes the completion row entirely, got: {rows:?}");
    }

    #[tokio::test]
    async fn completion_and_skip_coexist_for_same_item_date() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        let c = store
            .append(NewEvent {
                id: None,
                event_type: "routine_item_completed".into(),
                aggregate_id: "c-1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "item_id": "i1", "group_id": "g1",
                    "date": "2026-04-19", "completed_at": "2026-04-19T09:00:00Z"
                }),
            })
            .await
            .unwrap();
        let s = store
            .append(NewEvent {
                id: None,
                event_type: "routine_item_skipped".into(),
                aggregate_id: "s-1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "item_id": "i1", "group_id": "g1",
                    "date": "2026-04-19"
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[c, s]).await.unwrap();

        let mut resp = db
            .query("SELECT count() AS total FROM routine_completions GROUP ALL")
            .await
            .unwrap();
        let total: Option<u32> = resp.take("total").unwrap();
        assert_eq!(total, Some(2), "complete + skip rows live under separate keys");
    }
}
