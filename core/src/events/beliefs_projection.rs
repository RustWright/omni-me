//! SurrealDB projection over what the assistant has concluded about the user.
//!
//! One table, `beliefs`. A row is a statement plus the evidence behind it and a
//! confidence the model was made to choose from three words rather than invent a
//! number.
//!
//! ## Why this is its own projection, not part of `assistant`
//!
//! `AssistantProjection` materializes the *conversation* — what was asked, what
//! was answered, what was proposed. That is a record of the assistant's activity.
//! Beliefs are a record of **the user**, which the assistant happens to have
//! derived, and the difference is not filing: the conversation tables are
//! deliberately absent from `assistant::catalog` so the assistant cannot mine its
//! own past guesses as evidence, while `beliefs` **is** catalogued and is meant
//! to be read back. One projection holding both would make that boundary a matter
//! of remembering which table is which.
//!
//! ## Retired, never deleted
//!
//! Superseding sets `superseded_at` and leaves everything else. "What did it used
//! to think, and why did that stop being true" has to stay a query — a belief
//! that vanishes takes its own audit trail with it, and the audit trail is the
//! entire mitigation for a system that accumulates opinions about a person.

use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::{BeliefRecordedPayload, BeliefSupersededPayload};

pub struct BeliefsProjection;

impl BeliefsProjection {
    pub const NAME: &'static str = "beliefs";
}

#[async_trait]
impl Projection for BeliefsProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS beliefs SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS belief_id ON beliefs TYPE string;
             -- ⚠️ Every column the record event writes is `option<>` for the
             -- reason `assistant_proposals` documents at length: the pull filter
             -- runs on the author's clock, so a supersession can be folded before
             -- the belief it retires. Required columns would fail that inbound
             -- event, and a supersession that cannot land leaves a retired belief
             -- showing as live forever.
             DEFINE FIELD IF NOT EXISTS statement ON beliefs TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS confidence ON beliefs TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS review_after ON beliefs TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS recorded_at ON beliefs TYPE option<datetime>;
             DEFINE FIELD IF NOT EXISTS device_id ON beliefs TYPE option<string>;
             -- A SCHEMAFULL table validates objects inside an array key by key,
             -- so every `RecordRef` field is spelled out. Same closed-struct
             -- argument as `assistant_messages.records_read`, and the same trap
             -- if FLEXIBLE is reached for instead.
             DEFINE FIELD IF NOT EXISTS evidence ON beliefs TYPE option<array>;
             DEFINE FIELD IF NOT EXISTS evidence.* ON beliefs TYPE object;
             DEFINE FIELD IF NOT EXISTS evidence.*.kind ON beliefs TYPE string;
             DEFINE FIELD IF NOT EXISTS evidence.*.id ON beliefs TYPE string;
             DEFINE FIELD IF NOT EXISTS evidence.*.title ON beliefs TYPE option<string>;
             -- NONE while the belief still holds. Set, it is retired — whatever
             -- else this build can or cannot read about the supersession.
             DEFINE FIELD IF NOT EXISTS superseded_at ON beliefs TYPE option<datetime>;
             DEFINE FIELD IF NOT EXISTS superseded_reason ON beliefs TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS superseded_by ON beliefs TYPE option<string>;
             DEFINE INDEX IF NOT EXISTS beliefs_superseded ON beliefs FIELDS superseded_at;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query("DELETE FROM beliefs").await?.check()?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "belief_recorded" => self.on_recorded(event, db).await,
            "belief_superseded" => self.on_superseded(event, db).await,
            _ => Ok(()),
        }
    }
}

impl BeliefsProjection {
    async fn on_recorded(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // Skip, never error — `ConfigProjection::on_set`'s rule. A belief this
        // build cannot read was written by a different one, and failing the batch
        // would take the user's unrelated edits down with it.
        let parsed: BeliefRecordedPayload = match serde_json::from_value(event.payload.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    error = %e,
                    "skipping a belief this build cannot read"
                );
                return Ok(());
            }
        };

        let evidence = serde_json::to_value(&parsed.evidence)
            .map_err(|e| EventError::Validation(format!("could not serialize evidence: {e}")))?;

        // Writes only its own columns, so a supersession that landed first is not
        // undone by the belief arriving after it.
        db.query(
            "UPSERT type::record('beliefs', $id) SET
                belief_id = $id,
                statement = $statement,
                confidence = $confidence,
                review_after = $review_after,
                recorded_at = type::datetime($ts),
                device_id = $device_id,
                evidence = $evidence",
        )
        .bind(("id", parsed.belief_id.clone()))
        .bind(("statement", parsed.statement.clone()))
        .bind(("confidence", parsed.confidence.clone()))
        .bind(("review_after", parsed.review_after.clone()))
        .bind(("ts", event.timestamp.to_rfc3339()))
        .bind(("device_id", event.device_id.clone()))
        .bind(("evidence", evidence))
        .await?
        .check()?;

        Ok(())
    }

    async fn on_superseded(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let parsed: BeliefSupersededPayload = match serde_json::from_value(event.payload.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    error = %e,
                    "skipping a belief supersession this build cannot read"
                );
                return Ok(());
            }
        };

        db.query(
            "UPSERT type::record('beliefs', $id) SET
                belief_id = $id,
                superseded_at = type::datetime($ts),
                superseded_reason = $reason,
                superseded_by = $by",
        )
        .bind(("id", parsed.belief_id.clone()))
        .bind(("ts", event.timestamp.to_rfc3339()))
        .bind(("reason", parsed.reason.clone()))
        .bind(("by", parsed.superseded_by.clone()))
        .await?
        .check()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::store::NewEvent;
    use crate::events::types::{BeliefRecordedPayload, BeliefSupersededPayload, RecordRef};
    use chrono::{Duration, Utc};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("beliefs.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        BeliefsProjection.init_schema(&db).await.unwrap();
        db
    }

    fn recorded_at(ts: chrono::DateTime<Utc>, id: &str) -> Event {
        let payload = BeliefRecordedPayload {
            belief_id: id.into(),
            statement: "You consistently underestimate how long admin tasks take.".into(),
            confidence: "medium".into(),
            review_after: Some("2026-12-01".into()),
            evidence: vec![RecordRef {
                kind: "journal".into(),
                id: "2026-08-14".into(),
                title: None,
            }],
        };
        let new = NewEvent::belief_recorded("agent", &payload).unwrap();
        Event {
            id: ulid::Ulid::new().to_string(),
            event_type: new.event_type,
            aggregate_id: new.aggregate_id,
            timestamp: ts,
            device_id: new.device_id,
            payload: new.payload,
            received_at: None,
        }
    }

    fn superseded_at(ts: chrono::DateTime<Utc>, id: &str) -> Event {
        let payload = BeliefSupersededPayload {
            belief_id: id.into(),
            reason: Some("Their last four estimates were accurate.".into()),
            superseded_by: None,
        };
        let new = NewEvent::belief_superseded("phone", &payload).unwrap();
        Event {
            id: ulid::Ulid::new().to_string(),
            event_type: new.event_type,
            aggregate_id: new.aggregate_id,
            timestamp: ts,
            device_id: new.device_id,
            payload: new.payload,
            received_at: None,
        }
    }

    /// ⚠️ Asked as a `WHERE`, not by reading the column back.
    ///
    /// Both obvious readings are wrong in opposite directions, and neither fails
    /// loudly: taking a `datetime` as `Option<String>` yields `None` for a real
    /// timestamp (so nothing looks retired), while `<string> superseded_at` on an
    /// unset column yields a non-empty string (so everything does). The
    /// comparison the inbox already relies on is the one construct proven here.
    async fn is_retired(db: &Database, id: &str) -> bool {
        let mut resp = db
            .query(
                "SELECT belief_id FROM beliefs
                 WHERE belief_id = $id AND superseded_at IS NOT NONE",
            )
            .bind(("id", id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let got: Vec<String> = resp.take("belief_id").unwrap_or_default();
        !got.is_empty()
    }

    #[tokio::test]
    async fn a_belief_lands_with_its_evidence_and_can_be_retired() {
        let db = test_db().await;
        let t0 = Utc::now();

        BeliefsProjection
            .apply(&recorded_at(t0, "b1"), &db)
            .await
            .unwrap();

        let mut resp = db
            .query("SELECT statement, confidence, evidence FROM type::record('beliefs', 'b1')")
            .await
            .unwrap();
        let statements: Vec<Option<String>> = resp.take("statement").unwrap_or_default();
        let confidences: Vec<Option<String>> = resp.take("confidence").unwrap_or_default();
        assert!(
            statements
                .first()
                .cloned()
                .flatten()
                .unwrap_or_default()
                .contains("underestimate")
        );
        assert_eq!(
            confidences.first().cloned().flatten(),
            Some("medium".into())
        );
        assert!(!is_retired(&db, "b1").await);

        BeliefsProjection
            .apply(&superseded_at(t0 + Duration::seconds(5), "b1"), &db)
            .await
            .unwrap();
        assert!(is_retired(&db, "b1").await);
    }

    /// ⚠️ The arrival-order case. A supersession authored on a device whose clock
    /// trails the agent's can be folded first; if it cannot land, a retired
    /// belief keeps showing as live.
    #[tokio::test]
    async fn a_supersession_that_arrives_first_still_converges() {
        let db = test_db().await;
        let t0 = Utc::now();

        BeliefsProjection
            .apply(&superseded_at(t0, "b1"), &db)
            .await
            .unwrap();
        assert!(is_retired(&db, "b1").await, "the supersession must land");

        BeliefsProjection
            .apply(&recorded_at(t0 + Duration::seconds(5), "b1"), &db)
            .await
            .unwrap();

        assert!(
            is_retired(&db, "b1").await,
            "the late record must not resurrect a retired belief — a rebuild would \
             otherwise un-retire every belief the user has ever dismissed"
        );
        let mut resp = db
            .query("SELECT statement FROM type::record('beliefs', 'b1')")
            .await
            .unwrap();
        let statements: Vec<Option<String>> = resp.take("statement").unwrap_or_default();
        assert!(
            statements.first().cloned().flatten().is_some(),
            "and fill in"
        );
    }

    #[tokio::test]
    async fn replaying_is_idempotent() {
        let db = test_db().await;
        let t0 = Utc::now();
        let rec = recorded_at(t0, "b1");
        let sup = superseded_at(t0 + Duration::seconds(5), "b1");

        for event in [&rec, &sup, &rec, &sup] {
            BeliefsProjection.apply(event, &db).await.unwrap();
        }

        let mut resp = db.query("SELECT belief_id FROM beliefs").await.unwrap();
        let ids: Vec<String> = resp.take("belief_id").unwrap_or_default();
        assert_eq!(ids.len(), 1);
        assert!(is_retired(&db, "b1").await);
    }
}
