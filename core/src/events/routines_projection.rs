use std::collections::HashMap;

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

        // UPSERT, not CREATE: a rebuild replays creates, and a create pulled
        // from another device must converge on the row rather than fail on a
        // duplicate key — the local path is fail-fast, so a duplicate surfaces
        // to the user as a raw DB error. `removed ?? false` preserves a removal
        // that arrived ahead of its create; group ids are per-creation ULIDs, so
        // the create genuinely precedes its own edits in a timestamp-ordered
        // pull and setting name/frequency here is safe.
        db.query(
            "UPSERT type::record('routine_groups', $group_id) SET
                name = $name,
                frequency = $frequency,
                order_num = $order_num,
                removed = removed ?? false,
                created_at = created_at ?? type::datetime($ts),
                updated_at = type::datetime($ts)",
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

        // Dedup on group_id (last-wins). Today's frontend reorder code can't
        // emit duplicates, but a future sync-merge strategy or external API
        // surface might — and silent overwrite would be a data-corruption bug.
        let mut deduped: HashMap<String, i64> = HashMap::new();
        for entry in orderings {
            let group_id = entry["group_id"].as_str().unwrap_or_default().to_string();
            if group_id.is_empty() {
                continue;
            }
            let order = entry["order"].as_u64().unwrap_or(0) as i64;
            deduped.insert(group_id, order);
        }
        if deduped.is_empty() {
            return Ok(());
        }

        // Wrap N updates in a single transaction so a mid-batch write failure
        // can never leave the projection partially applied.
        let mut parts = vec!["BEGIN TRANSACTION;".to_string()];
        for i in 0..deduped.len() {
            parts.push(format!(
                "UPSERT type::record('routine_groups', $group_id_{i}) SET
                    order_num = $order_num_{i},
                    updated_at = type::datetime($ts),
                    name = name ?? '',
                    frequency = frequency ?? '',
                    removed = removed ?? false,
                    created_at = created_at ?? type::datetime($ts);"
            ));
        }
        parts.push("COMMIT TRANSACTION;".to_string());
        let query_str = parts.join("\n");

        let mut q = db.query(query_str.as_str()).bind(("ts", ts));
        for (i, (group_id, order)) in deduped.iter().enumerate() {
            q = q
                .bind((format!("group_id_{i}"), group_id.clone()))
                .bind((format!("order_num_{i}"), *order));
        }
        q.await?;

        Ok(())
    }

    async fn on_group_removed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let group_id = event.payload["group_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        // UPSERT so a removal that outruns its create still lands. A bare
        // UPDATE silently matched nothing and the group stayed visible on that
        // device permanently, because nothing ever retries a no-op'd mutation.
        db.query(
            "UPSERT type::record('routine_groups', $group_id) SET
                removed = true,
                updated_at = type::datetime($ts),
                name = name ?? '',
                frequency = frequency ?? '',
                order_num = order_num ?? 0,
                created_at = created_at ?? type::datetime($ts)",
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

        // UPSERT for the same reason as `on_group_created`.
        db.query(
            "UPSERT type::record('routine_items', $item_id) SET
                group_id = $group_id,
                name = $name,
                estimated_duration_min = $duration,
                order_num = $order_num,
                removed = removed ?? false",
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

        let name = changes.get("name").and_then(|v| v.as_str()).map(String::from);
        let duration = changes
            .get("estimated_duration_min")
            .and_then(|v| v.as_u64())
            .map(|n| n as i64);
        let order = changes.get("order").and_then(|v| v.as_u64()).map(|n| n as i64);

        // Collapse the conditional UPDATEs into one statement so the projection
        // state can never be partially applied (single statements are atomic;
        // multi-statement coupled updates would need BEGIN/COMMIT — see
        // on_group_reordered for the multi-statement pattern).
        let mut sets: Vec<&str> = Vec::new();
        if name.is_some() {
            sets.push("name = $name");
        }
        if duration.is_some() {
            sets.push("estimated_duration_min = $duration");
        }
        if order.is_some() {
            sets.push("order_num = $order_num");
        }
        if sets.is_empty() {
            return Ok(());
        }

        // Backfills for the SCHEMAFULL columns this change bag does NOT touch.
        // They must be *disjoint* from `sets`: assignments in one SET clause are
        // applied in order, so a trailing `name = name ?? ''` would silently
        // overwrite the rename it was meant to sit beside.
        if name.is_none() {
            sets.push("name = name ?? ''");
        }
        if duration.is_none() {
            sets.push("estimated_duration_min = estimated_duration_min ?? 0");
        }
        if order.is_none() {
            sets.push("order_num = order_num ?? 0");
        }
        sets.push("group_id = group_id ?? ''");
        sets.push("removed = removed ?? false");

        // UPSERT-materialize: this is the handler that produced the reported
        // symptom — device A adds an item then renames it, device B skips or
        // loses the create, and the rename UPDATE matches nothing. When the
        // create finally lands, B shows the **pre-rename** name forever.
        // `RoutineItemModifiedPayload` carries no `group_id`, so the SCHEMAFULL
        // backfill has to be `group_id ?? ''`; the create supplies the real one.
        let query_str = format!(
            "UPSERT type::record('routine_items', $item_id) SET {}",
            sets.join(", ")
        );

        let mut q = db.query(query_str.as_str()).bind(("item_id", item_id));
        if let Some(n) = name {
            q = q.bind(("name", n));
        }
        if let Some(d) = duration {
            q = q.bind(("duration", d));
        }
        if let Some(o) = order {
            q = q.bind(("order_num", o));
        }
        q.await?;

        Ok(())
    }

    async fn on_item_removed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let item_id = event.payload["item_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();

        // UPSERT so a removal that outruns its create isn't silently dropped.
        db.query(
            "UPSERT type::record('routine_items', $item_id) SET
                removed = true,
                group_id = group_id ?? '',
                name = name ?? '',
                estimated_duration_min = estimated_duration_min ?? 0,
                order_num = order_num ?? 0",
        )
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

        // `completion_key` is `{item_id}-{date}-done` — deterministic, and
        // therefore identical on every device. Two devices ticking the same item
        // on the same day used to collide on CREATE, and the local path is
        // fail-fast, so the second one surfaced a raw DB error to the user.
        // UPSERT makes the tick converge instead.
        db.query(
            "UPSERT type::record('routine_completions', $completion_id) SET
                item_id = $item_id,
                group_id = $group_id,
                date = $date,
                completed_at = type::datetime($completed_at),
                skipped = false,
                reason = NONE",
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

        // Same deterministic-key collision as `on_item_completed`.
        db.query(
            "UPSERT type::record('routine_completions', $completion_id) SET
                item_id = $item_id,
                group_id = $group_id,
                date = $date,
                completed_at = type::datetime($ts),
                skipped = true,
                reason = $reason",
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

    /// Helper: append + apply one routines event.
    async fn emit(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        event_type: &str,
        aggregate_id: &str,
        payload: serde_json::Value,
    ) {
        let e = store
            .append(NewEvent {
                id: None,
                event_type: event_type.into(),
                aggregate_id: aggregate_id.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload,
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();
    }

    async fn routines_fixture() -> (Database, SurrealEventStore, ProjectionRunner) {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    /// A mutation that arrives **before** the create it depends on must still
    /// land, and must survive the create arriving afterwards.
    ///
    /// This is an absence test for a silent no-op: routines was the projection
    /// family nobody converted to UPSERT-materialize. Device A adds an item and
    /// renames it; on device B the create is skipped by `apply_events_resilient`
    /// or simply hasn't been pulled, the rename's bare `UPDATE` matches nothing,
    /// and when the create finally lands B shows the **pre-rename** name
    /// forever, because nothing ever retries a no-op'd mutation.
    #[tokio::test]
    async fn item_rename_before_its_create_survives_the_create() {
        let (db, store, runner) = routines_fixture().await;

        // The rename lands first — the create was skipped or is still in flight.
        emit(
            &store,
            &runner,
            "routine_item_modified",
            "i1",
            serde_json::json!({ "item_id": "i1", "changes": { "name": "Stretch Deeper" } }),
        )
        .await;

        let mut resp = db
            .query("SELECT name FROM type::record('routine_items', 'i1')")
            .await
            .unwrap();
        let name: Option<String> = resp.take("name").unwrap();
        assert_eq!(
            name.as_deref(),
            Some("Stretch Deeper"),
            "rename was dropped instead of materializing a row"
        );

        // The create catches up. It legitimately owns `name`, so the ordering
        // loss is real — but the row exists and converges, rather than the
        // rename being lost with no trace.
        emit(
            &store,
            &runner,
            "routine_item_added",
            "i1",
            serde_json::json!({
                "group_id": "morning", "name": "Stretch",
                "estimated_duration_min": 5, "order": 0
            }),
        )
        .await;

        let mut resp = db
            .query("SELECT group_id, removed FROM type::record('routine_items', 'i1')")
            .await
            .unwrap();
        let group_id: Option<String> = resp.take("group_id").unwrap();
        let removed: Option<bool> = resp.take("removed").unwrap();
        assert_eq!(group_id.as_deref(), Some("morning"));
        assert_eq!(removed, Some(false));
    }

    /// A removal that outruns its create must stick. Previously the `UPDATE`
    /// matched nothing, the create then materialized the item, and it stayed
    /// visible on that device permanently.
    #[tokio::test]
    async fn item_removal_before_its_create_is_not_resurrected() {
        let (db, store, runner) = routines_fixture().await;

        emit(
            &store,
            &runner,
            "routine_item_removed",
            "i1",
            serde_json::json!({ "item_id": "i1" }),
        )
        .await;
        emit(
            &store,
            &runner,
            "routine_item_added",
            "i1",
            serde_json::json!({
                "group_id": "morning", "name": "Stretch",
                "estimated_duration_min": 5, "order": 0
            }),
        )
        .await;

        let mut resp = db
            .query("SELECT removed FROM type::record('routine_items', 'i1')")
            .await
            .unwrap();
        let removed: Option<bool> = resp.take("removed").unwrap();
        assert_eq!(
            removed,
            Some(true),
            "a create replayed after a removal un-deleted the item"
        );
    }

    /// `completion_key` is `{item_id}-{date}-{done|skip}` — deterministic, so
    /// two devices ticking the same item on the same day produce the *same*
    /// record id. Under `CREATE` the second one collided, and the local
    /// completion path is fail-fast, so the user saw a raw DB error.
    #[tokio::test]
    async fn same_day_completion_from_two_devices_does_not_collide() {
        let (db, store, runner) = routines_fixture().await;

        for _ in 0..2 {
            emit(
                &store,
                &runner,
                "routine_item_completed",
                "i1",
                serde_json::json!({
                    "item_id": "i1", "group_id": "morning", "date": "2026-08-26"
                }),
            )
            .await;
        }

        let mut resp = db
            .query("SELECT count() AS n FROM routine_completions GROUP ALL")
            .await
            .unwrap();
        let n: Option<i64> = resp.take("n").unwrap();
        assert_eq!(n, Some(1), "duplicate completion rows");
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

    #[tokio::test]
    async fn group_reordered_dedupes_duplicate_group_ids_last_wins() {
        // Defense against future callers / sync-merge strategies emitting a
        // duplicate group_id in one orderings list. Last entry wins.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RoutinesProjection)]);
        runner.init_all().await.unwrap();

        let g = store
            .append(NewEvent {
                id: None,
                event_type: "routine_group_created".into(),
                aggregate_id: "g1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "name": "g1", "frequency": "daily", "order": 5
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[g]).await.unwrap();

        let e = store
            .append(NewEvent {
                id: None,
                event_type: "routine_group_reordered".into(),
                aggregate_id: "reorder".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "orderings": [
                        { "group_id": "g1", "order": 1 },
                        { "group_id": "g1", "order": 7 }
                    ]
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();

        let mut resp = db
            .query("SELECT order_num FROM type::record('routine_groups', 'g1')")
            .await
            .unwrap();
        let order: Option<i64> = resp.take("order_num").unwrap();
        assert_eq!(order, Some(7), "last-wins on duplicate group_id");
    }

    #[tokio::test]
    async fn item_modified_combines_all_three_fields_in_one_update() {
        // Regression for the on_item_modified pattern: all fields applied
        // atomically via one statement (a previous version issued 3 separate
        // queries, leaving a window where partial-failure could land 1 of 3).
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
                    "group_id": "g1", "name": "Squat", "estimated_duration_min": 5, "order": 0
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
                    "changes": { "name": "Squat 5x5", "estimated_duration_min": 12, "order": 3 }
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT name, estimated_duration_min, order_num FROM type::record('routine_items', 'i1')")
            .await
            .unwrap();
        let name: Option<String> = resp.take("name").unwrap();
        let dur: Option<i64> = resp.take("estimated_duration_min").unwrap();
        let order: Option<i64> = resp.take("order_num").unwrap();
        assert_eq!(name.as_deref(), Some("Squat 5x5"));
        assert_eq!(dur, Some(12));
        assert_eq!(order, Some(3));
    }

    #[tokio::test]
    async fn item_modified_with_no_recognized_changes_is_a_noop() {
        // The handler must not crash or write garbage when `changes` has no
        // recognized keys (defensive against payload schema drift).
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
                    "group_id": "g1", "name": "Original", "estimated_duration_min": 5, "order": 0
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
                    "changes": { "unknown_field": "ignored" }
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT name FROM type::record('routine_items', 'i1')")
            .await
            .unwrap();
        let name: Option<String> = resp.take("name").unwrap();
        assert_eq!(name.as_deref(), Some("Original"));
    }
}
