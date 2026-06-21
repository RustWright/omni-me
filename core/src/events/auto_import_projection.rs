//! SurrealDB projection over auto-import batch events (Phase 3.10.4).
//!
//! One read-optimized table: `pending_auto_import_batches`. The UI lists
//! `status = "pending"` rows; `committed` and `dismissed` are kept for audit
//! / debugging.
//!
//! Idempotency: record id is derived from `format!("{source}-{dedup_key}")`
//! so duplicate `Proposed` events (e.g., the IMAP poller re-fetches the same
//! message UID) UPSERT into the same row instead of producing a second
//! pending entry. This is what makes the "user dismissed a batch — don't
//! re-propose on the next IMAP tick" behavior work for free: the second
//! Proposed event's UPSERT runs, but the existing row has status="dismissed",
//! which we preserve (UPSERT does NOT downgrade `dismissed` back to
//! `pending` — see `on_proposed` below).
//!
//! `Committed` and `Dismissed` events update by `batch_id` field (not record
//! id, since the record id is source+dedup_key). An index on `batch_id`
//! keeps that lookup fast.

use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};

pub struct AutoImportProjection;

#[async_trait]
impl Projection for AutoImportProjection {
    fn name(&self) -> &str {
        "auto_import"
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS pending_auto_import_batches SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS batch_id ON pending_auto_import_batches TYPE string;
             DEFINE FIELD IF NOT EXISTS source ON pending_auto_import_batches TYPE string;
             DEFINE FIELD IF NOT EXISTS dedup_key ON pending_auto_import_batches TYPE string;
             DEFINE FIELD IF NOT EXISTS fetched_at ON pending_auto_import_batches TYPE string;
             DEFINE FIELD IF NOT EXISTS draft_postings ON pending_auto_import_batches TYPE array;
             DEFINE FIELD IF NOT EXISTS draft_postings.* ON pending_auto_import_batches TYPE object FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS source_metadata ON pending_auto_import_batches TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS status ON pending_auto_import_batches TYPE string;
             DEFINE FIELD IF NOT EXISTS resolved_at ON pending_auto_import_batches TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS resolve_reason ON pending_auto_import_batches TYPE option<string>;
             DEFINE INDEX IF NOT EXISTS pending_auto_import_batch_id_idx ON pending_auto_import_batches FIELDS batch_id;",
        )
        .await?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query("DELETE FROM pending_auto_import_batches;").await?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "auto_import_batch_proposed" => self.on_proposed(event, db).await,
            "auto_import_batch_committed" => self.on_resolved(event, db, "committed").await,
            "auto_import_batch_dismissed" => self.on_resolved(event, db, "dismissed").await,
            _ => Ok(()),
        }
    }
}

impl AutoImportProjection {
    async fn on_proposed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let batch_id = event.payload["batch_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let source = event.payload["source"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let dedup_key = event.payload["dedup_key"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let fetched_at = event.payload["fetched_at"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let draft_postings = event.payload["draft_postings"].clone();
        let source_metadata = event.payload.get("source_metadata").cloned();

        // Record id is derived from (source, dedup_key) so duplicate
        // proposals collapse to the same row. We do NOT clobber a resolved
        // row (status = committed | dismissed) — if the row exists and is
        // already resolved, the Proposed re-emit is a no-op (the dismissed
        // batch stays dismissed; the committed batch stays committed).
        let record_id = format!("{source}-{dedup_key}");

        let mut existing = db
            .query(
                "SELECT status FROM type::record('pending_auto_import_batches', $rid) LIMIT 1",
            )
            .bind(("rid", record_id.clone()))
            .await?;
        let existing_status: Option<String> = existing.take("status").unwrap_or(None);
        if let Some(s) = existing_status
            && (s == "committed" || s == "dismissed")
        {
            // Resolved batch — Proposed re-emit is a no-op. Dedup at work.
            return Ok(());
        }

        db.query(
            "UPSERT type::record('pending_auto_import_batches', $rid) CONTENT {
                batch_id: $batch_id,
                source: $source,
                dedup_key: $dedup_key,
                fetched_at: $fetched_at,
                draft_postings: $draft_postings,
                source_metadata: $source_metadata,
                status: 'pending',
                resolved_at: NONE,
                resolve_reason: NONE
            }",
        )
        .bind(("rid", record_id))
        .bind(("batch_id", batch_id))
        .bind(("source", source))
        .bind(("dedup_key", dedup_key))
        .bind(("fetched_at", fetched_at))
        .bind(("draft_postings", draft_postings))
        .bind(("source_metadata", source_metadata))
        .await?;
        Ok(())
    }

    async fn on_resolved(
        &self,
        event: &Event,
        db: &Database,
        new_status: &str,
    ) -> Result<(), EventError> {
        let batch_id = event.payload["batch_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let resolved_at = event.timestamp.to_rfc3339();
        let resolve_reason = event
            .payload
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Look up by batch_id field (record id is source+dedup_key; the
        // resolved event only carries batch_id, hence the index).
        db.query(
            "UPDATE pending_auto_import_batches SET
                status = $status,
                resolved_at = $resolved_at,
                resolve_reason = $resolve_reason
             WHERE batch_id = $batch_id",
        )
        .bind(("batch_id", batch_id))
        .bind(("status", new_status.to_string()))
        .bind(("resolved_at", resolved_at))
        .bind(("resolve_reason", resolve_reason))
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::projection::ProjectionRunner;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use chrono::Utc;

    async fn test_db_and_runner() -> (Database, SurrealEventStore, ProjectionRunner) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let store = SurrealEventStore::new(db.clone());
        let runner =
            ProjectionRunner::new(db.clone(), vec![Box::new(AutoImportProjection)]);
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    fn proposed_payload(batch_id: &str, source: &str, dedup_key: &str) -> serde_json::Value {
        serde_json::json!({
            "batch_id": batch_id,
            "source": source,
            "dedup_key": dedup_key,
            "fetched_at": Utc::now().to_rfc3339(),
            "draft_postings": [{
                "external_id": "ext-1",
                "date": "2026-04-15",
                "description": "Test txn",
                "postings": []
            }],
        })
    }

    async fn list_pending(db: &Database) -> Vec<(String, String)> {
        let mut resp = db
            .query("SELECT batch_id, status FROM pending_auto_import_batches ORDER BY batch_id")
            .await
            .unwrap();
        let batch_ids: Vec<String> = resp.take("batch_id").unwrap();
        let statuses: Vec<String> = resp.take("status").unwrap();
        batch_ids.into_iter().zip(statuses).collect()
    }

    #[tokio::test]
    async fn proposed_event_appears_as_pending_row() {
        let (db, store, runner) = test_db_and_runner().await;
        let event = store
            .append(NewEvent {
                id: Some("B1".into()),
                event_type: "auto_import_batch_proposed".into(),
                aggregate_id: "B1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: proposed_payload("B1", "meridian-aed", "meridian-aed-uid-42"),
            })
            .await
            .unwrap();
        runner.apply_events(&[event]).await.unwrap();

        let rows = list_pending(&db).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "B1");
        assert_eq!(rows[0].1, "pending");
    }

    #[tokio::test]
    async fn same_dedup_key_proposed_twice_yields_single_row() {
        // meridian-aed re-fetches the same email UID → two Proposed events with
        // the same source+dedup_key but different batch_ids. The UPSERT
        // collapses them into one pending row.
        let (db, store, runner) = test_db_and_runner().await;
        for batch_id in ["B1", "B2"] {
            let event = store
                .append(NewEvent {
                    id: Some(batch_id.into()),
                    event_type: "auto_import_batch_proposed".into(),
                    aggregate_id: batch_id.into(),
                    timestamp: Utc::now(),
                    device_id: "d1".into(),
                    payload: proposed_payload(batch_id, "meridian-aed", "meridian-aed-uid-42"),
                })
                .await
                .unwrap();
            runner.apply_events(&[event]).await.unwrap();
        }

        let rows = list_pending(&db).await;
        assert_eq!(rows.len(), 1, "duplicate dedup_key should collapse");
    }

    #[tokio::test]
    async fn committed_event_flips_status() {
        let (db, store, runner) = test_db_and_runner().await;
        let proposed = store
            .append(NewEvent {
                id: Some("B1".into()),
                event_type: "auto_import_batch_proposed".into(),
                aggregate_id: "B1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: proposed_payload("B1", "wise", "wise-12345"),
            })
            .await
            .unwrap();
        runner.apply_events(&[proposed]).await.unwrap();

        let committed = store
            .append(NewEvent {
                id: None,
                event_type: "auto_import_batch_committed".into(),
                aggregate_id: "B1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "batch_id": "B1",
                    "accepted_indices": [0],
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[committed]).await.unwrap();

        let rows = list_pending(&db).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "committed");
    }

    #[tokio::test]
    async fn dismissed_event_flips_status_and_blocks_reproposal() {
        let (db, store, runner) = test_db_and_runner().await;
        // 1) Proposed
        let proposed = store
            .append(NewEvent {
                id: Some("B1".into()),
                event_type: "auto_import_batch_proposed".into(),
                aggregate_id: "B1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: proposed_payload("B1", "meridian-aed", "meridian-aed-uid-99"),
            })
            .await
            .unwrap();
        runner.apply_events(&[proposed]).await.unwrap();

        // 2) Dismissed
        let dismissed = store
            .append(NewEvent {
                id: None,
                event_type: "auto_import_batch_dismissed".into(),
                aggregate_id: "B1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "batch_id": "B1",
                    "reason": "hallucinated rows",
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[dismissed]).await.unwrap();

        // 3) Re-fetch same UID → another Proposed with same dedup_key.
        //    Must NOT downgrade the dismissed row back to pending.
        let reproposed = store
            .append(NewEvent {
                id: Some("B2".into()),
                event_type: "auto_import_batch_proposed".into(),
                aggregate_id: "B2".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: proposed_payload("B2", "meridian-aed", "meridian-aed-uid-99"),
            })
            .await
            .unwrap();
        runner.apply_events(&[reproposed]).await.unwrap();

        let rows = list_pending(&db).await;
        assert_eq!(rows.len(), 1, "still one row");
        assert_eq!(rows[0].1, "dismissed", "stays dismissed across re-propose");
    }

    // --- queries integration (Phase 3.10.5) ---

    #[tokio::test]
    async fn queries_list_pending_returns_only_pending_rows() {
        use crate::db::queries::{count_pending_batches, get_pending_batch_by_id, list_pending_batches};

        let (db, store, runner) = test_db_and_runner().await;

        // Pending row.
        let p1 = store
            .append(NewEvent {
                id: Some("P1".into()),
                event_type: "auto_import_batch_proposed".into(),
                aggregate_id: "P1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: proposed_payload("P1", "wise", "wise-001"),
            })
            .await
            .unwrap();
        runner.apply_events(&[p1]).await.unwrap();

        // Pending → committed; should drop out of list_pending_batches.
        let p2 = store
            .append(NewEvent {
                id: Some("P2".into()),
                event_type: "auto_import_batch_proposed".into(),
                aggregate_id: "P2".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: proposed_payload("P2", "meridian-aed", "meridian-aed-uid-200"),
            })
            .await
            .unwrap();
        runner.apply_events(&[p2]).await.unwrap();
        let c2 = store
            .append(NewEvent {
                id: None,
                event_type: "auto_import_batch_committed".into(),
                aggregate_id: "P2".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({"batch_id": "P2", "accepted_indices": [0]}),
            })
            .await
            .unwrap();
        runner.apply_events(&[c2]).await.unwrap();

        let pending = list_pending_batches(&db).await.unwrap();
        assert_eq!(pending.len(), 1, "only P1 is pending");
        assert_eq!(pending[0].batch_id, "P1");
        assert_eq!(pending[0].source, "wise");
        assert_eq!(pending[0].status, "pending");

        let count = count_pending_batches(&db).await.unwrap();
        assert_eq!(count, 1);

        // get_pending_batch_by_id resolves both pending and resolved rows
        // (commit_batch/dismiss_batch use it to fetch the draft_postings even
        // for already-resolved rows, so it must return them too — the
        // status-gate is in the command, not the query).
        let fetched = get_pending_batch_by_id(&db, "P2").await.unwrap();
        assert!(fetched.is_some(), "committed row still queryable by batch_id");
        assert_eq!(fetched.unwrap().status, "committed");
    }
}
