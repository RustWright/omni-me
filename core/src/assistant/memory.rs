//! Reading back what the assistant believes.
//!
//! The list exists so the belief set can be **audited rather than trusted**,
//! which `docs/src/assistant.md` names as the whole mitigation for a system that
//! accumulates opinions about a person. That is why retired beliefs are readable
//! too: a memory you can only see the current state of cannot be audited, only
//! inspected.

use chrono::{NaiveDate, Utc};
use serde::Serialize;
use surrealdb::types::SurrealValue;

use crate::db::Database;
use crate::events::EventError;

/// One cited record, as it comes back off the projection.
///
/// Deliberately **not** [`crate::events::RecordRef`], which is the shape written
/// into the log. Same split as `AnswerUsage` against `llm::chat::Usage`: the
/// event type is a permanent record and must not acquire a database trait to
/// suit a query, and the query shape must be free to change without moving the
/// log underneath it. They are converted by serde at the edges, not aliased.
#[derive(Debug, Clone, PartialEq, Serialize, SurrealValue)]
pub struct EvidenceRef {
    pub kind: String,
    pub id: String,
    /// Always `None` off the wire — the event deliberately freezes no title, so a
    /// renamed record still cites correctly. Resolve it locally for display.
    pub title: Option<String>,
}

/// One belief, as a person reads it.
#[derive(Debug, Clone, Serialize)]
pub struct Belief {
    pub belief_id: String,
    pub statement: String,
    /// `low` | `medium` | `high`. A value this build does not know is passed
    /// through rather than normalized — the reader shows what was recorded.
    pub confidence: String,
    pub recorded_at: String,
    /// `YYYY-MM-DD`, when one was set.
    pub review_after: Option<String>,
    /// The records the run had actually opened. May be empty: an unsupported
    /// conclusion is shown as unsupported, not hidden.
    pub evidence: Vec<EvidenceRef>,
    /// `None` while it still holds.
    pub superseded_at: Option<String>,
    pub superseded_reason: Option<String>,
}

impl Belief {
    pub fn is_live(&self) -> bool {
        self.superseded_at.is_none()
    }

    /// Whether this is past its review date, against a given day.
    ///
    /// Takes the date rather than reading the clock so a caller can ask about any
    /// day and a test needs no time travel. A belief with no review date is never
    /// due — the absence means "no reason to expect this to change", not "never
    /// checked".
    pub fn is_due_for_review(&self, today: NaiveDate) -> bool {
        if !self.is_live() {
            return false;
        }
        self.review_after
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .is_some_and(|due| due <= today)
    }
}

/// Every belief that still holds, newest first.
pub async fn live(db: &Database) -> Result<Vec<Belief>, EventError> {
    query(db, "WHERE superseded_at IS NONE").await
}

/// Everything ever believed, including what has been retired.
pub async fn all(db: &Database) -> Result<Vec<Belief>, EventError> {
    query(db, "").await
}

/// Live beliefs whose review date has passed, oldest review date first.
///
/// The queue Phase F's scheduled review works from. Ordered by how overdue it is
/// rather than by when it was recorded, because the point is which one has been
/// unexamined longest.
pub async fn due_for_review(db: &Database, today: NaiveDate) -> Result<Vec<Belief>, EventError> {
    let mut due: Vec<Belief> = live(db)
        .await?
        .into_iter()
        .filter(|b| b.is_due_for_review(today))
        .collect();
    due.sort_by(|a, b| a.review_after.cmp(&b.review_after));
    Ok(due)
}

/// Today, as the review queue reckons it.
pub fn today() -> NaiveDate {
    Utc::now().date_naive()
}

async fn query(db: &Database, filter: &str) -> Result<Vec<Belief>, EventError> {
    // ⚠️ `recorded_at` is selected as well as ordered on — SurrealDB v3 rejects an
    // `ORDER BY` over a field the projection does not return.
    let sql = format!(
        "SELECT belief_id, statement, confidence, review_after, evidence,
                <string> recorded_at AS recorded_at,
                <string> superseded_at AS superseded_at,
                superseded_reason
         FROM beliefs {filter} ORDER BY recorded_at DESC"
    );
    let mut resp = db.query(sql).await?.check()?;

    let ids: Vec<String> = resp.take("belief_id").unwrap_or_default();
    let statements: Vec<Option<String>> = resp.take("statement").unwrap_or_default();
    let confidences: Vec<Option<String>> = resp.take("confidence").unwrap_or_default();
    let reviews: Vec<Option<String>> = resp.take("review_after").unwrap_or_default();
    let evidence: Vec<Option<Vec<EvidenceRef>>> = resp.take("evidence").unwrap_or_default();
    let recorded: Vec<Option<String>> = resp.take("recorded_at").unwrap_or_default();
    let superseded: Vec<Option<String>> = resp.take("superseded_at").unwrap_or_default();
    let reasons: Vec<Option<String>> = resp.take("superseded_reason").unwrap_or_default();

    let mut out = Vec::with_capacity(ids.len());
    for (i, belief_id) in ids.iter().enumerate() {
        // A row whose statement has not arrived is skipped rather than shown as a
        // blank belief. It exists only in the window where a supersession was
        // folded before the belief it retires — see the projection's schema note.
        let (Some(statement), Some(recorded_at)) = (
            statements.get(i).cloned().flatten(),
            recorded.get(i).cloned().flatten(),
        ) else {
            continue;
        };

        out.push(Belief {
            belief_id: belief_id.clone(),
            statement,
            confidence: confidences
                .get(i)
                .cloned()
                .flatten()
                .unwrap_or_else(|| "low".to_string()),
            recorded_at,
            review_after: reviews.get(i).cloned().flatten(),
            evidence: evidence.get(i).cloned().flatten().unwrap_or_default(),
            // ⚠️ `<string>` on an unset datetime yields a non-empty string rather
            // than NONE, so an uncast-vs-cast mix-up reports every belief as
            // retired. Normalized here, once, rather than at each caller.
            superseded_at: superseded
                .get(i)
                .cloned()
                .flatten()
                .filter(|s| !s.is_empty() && s != "NONE"),
            superseded_reason: reasons.get(i).cloned().flatten(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{BeliefRecordedPayload, BeliefSupersededPayload, BeliefsProjection};
    use crate::events::{Event, NewEvent, Projection, RecordRef};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        BeliefsProjection.init_schema(&db).await.unwrap();
        db
    }

    fn wrap(new: NewEvent) -> Event {
        Event {
            id: ulid::Ulid::new().to_string(),
            event_type: new.event_type,
            aggregate_id: new.aggregate_id,
            timestamp: Utc::now(),
            device_id: new.device_id,
            payload: new.payload,
            received_at: None,
        }
    }

    async fn record(db: &Database, id: &str, statement: &str, review_after: Option<&str>) {
        let payload = BeliefRecordedPayload {
            belief_id: id.into(),
            statement: statement.into(),
            confidence: "medium".into(),
            review_after: review_after.map(str::to_string),
            evidence: vec![RecordRef {
                kind: "journal".into(),
                id: "2026-08-14".into(),
                title: None,
            }],
        };
        let event = wrap(NewEvent::belief_recorded("agent", &payload).unwrap());
        BeliefsProjection.apply(&event, db).await.unwrap();
    }

    async fn retire(db: &Database, id: &str) {
        let payload = BeliefSupersededPayload {
            belief_id: id.into(),
            reason: Some("no longer true".into()),
            superseded_by: None,
        };
        let event = wrap(NewEvent::belief_superseded("phone", &payload).unwrap());
        BeliefsProjection.apply(&event, db).await.unwrap();
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[tokio::test]
    async fn live_excludes_retired_but_all_keeps_them() {
        let db = test_db().await;
        record(&db, "b1", "You underestimate admin tasks.", None).await;
        record(&db, "b2", "You sleep badly when travelling.", None).await;
        retire(&db, "b2").await;

        let live = live(&db).await.unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].belief_id, "b1");
        assert!(live[0].is_live());
        assert_eq!(live[0].evidence.len(), 1, "evidence survives the read");

        // ⚠️ The audit property: a retired belief is still readable, with why.
        let all = all(&db).await.unwrap();
        assert_eq!(all.len(), 2);
        let retired = all.iter().find(|b| b.belief_id == "b2").unwrap();
        assert!(!retired.is_live());
        assert_eq!(retired.superseded_reason.as_deref(), Some("no longer true"));
    }

    #[tokio::test]
    async fn a_review_date_in_the_past_is_due_and_one_in_the_future_is_not() {
        let db = test_db().await;
        record(&db, "b1", "Past due.", Some("2026-01-01")).await;
        record(&db, "b2", "Not yet.", Some("2027-01-01")).await;
        record(&db, "b3", "No date at all.", None).await;

        let due = due_for_review(&db, day("2026-09-10")).await.unwrap();
        let ids: Vec<&str> = due.iter().map(|b| b.belief_id.as_str()).collect();
        assert_eq!(ids, vec!["b1"], "only the overdue one");
    }

    /// A belief with no review date is not "never checked" — it is a conclusion
    /// nobody expects to change. Sweeping those into the queue would make the
    /// queue permanently full and therefore ignored.
    #[tokio::test]
    async fn a_belief_with_no_review_date_is_never_due() {
        let db = test_db().await;
        record(&db, "b1", "No date.", None).await;
        assert!(
            due_for_review(&db, day("2099-01-01"))
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// A retired belief must never come back through the review queue.
    #[tokio::test]
    async fn a_retired_belief_is_not_due_however_overdue_its_date() {
        let db = test_db().await;
        record(&db, "b1", "Retired and overdue.", Some("2020-01-01")).await;
        retire(&db, "b1").await;
        assert!(
            due_for_review(&db, day("2026-09-10"))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn the_review_queue_puts_the_longest_unexamined_first() {
        let db = test_db().await;
        record(&db, "recent", "Recently due.", Some("2026-09-01")).await;
        record(&db, "ancient", "Long overdue.", Some("2024-01-01")).await;

        let due = due_for_review(&db, day("2026-09-10")).await.unwrap();
        let ids: Vec<&str> = due.iter().map(|b| b.belief_id.as_str()).collect();
        assert_eq!(ids, vec!["ancient", "recent"]);
    }
}
