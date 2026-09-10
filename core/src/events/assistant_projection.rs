//! SurrealDB projection over the assistant's conversations.
//!
//! Two tables. `assistant_threads` carries what a thread list needs — title,
//! timestamps, counts — and `assistant_messages` carries the turns. One table
//! with an embedded message array would have been fewer rows, and would have made
//! the thread list drag every answer body through to render a preview.
//!
//! ⚠️ **`assistant_messages` is deliberately absent from `assistant::catalog`.**
//! Registering it would make past conversations retrievable, which sounds like
//! recall and is actually the assistant citing its own earlier guesses as
//! evidence about the user's life. Derived belief with provenance is Phase E's
//! job and is a decision to take deliberately; `the_catalog_does_not_expose_conversations`
//! is what stops it happening as a side effect of this table existing.

use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::{AssistantAnswerGivenPayload, AssistantQuestionAskedPayload};

pub struct AssistantProjection;

impl AssistantProjection {
    pub const NAME: &'static str = "assistant";
}

#[async_trait]
impl Projection for AssistantProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS assistant_threads SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS thread_id ON assistant_threads TYPE string;
             DEFINE FIELD IF NOT EXISTS title ON assistant_threads TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS created_at ON assistant_threads TYPE datetime;
             DEFINE FIELD IF NOT EXISTS last_message_at ON assistant_threads TYPE datetime;
             DEFINE FIELD IF NOT EXISTS message_count ON assistant_threads TYPE int;

             DEFINE TABLE IF NOT EXISTS assistant_messages SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS message_id ON assistant_messages TYPE string;
             DEFINE FIELD IF NOT EXISTS thread_id ON assistant_messages TYPE string;
             DEFINE FIELD IF NOT EXISTS role ON assistant_messages TYPE string;
             DEFINE FIELD IF NOT EXISTS text ON assistant_messages TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS created_at ON assistant_messages TYPE datetime;
             DEFINE FIELD IF NOT EXISTS device_id ON assistant_messages TYPE string;
             DEFINE FIELD IF NOT EXISTS in_reply_to ON assistant_messages TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS stopped ON assistant_messages TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS detail ON assistant_messages TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS model ON assistant_messages TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS usage ON assistant_messages TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS elapsed_ms ON assistant_messages TYPE option<int>;
             DEFINE FIELD IF NOT EXISTS verbs ON assistant_messages TYPE option<array<string>>;
             -- ⚠️ A SCHEMAFULL table validates objects **inside** an array key by
             -- key, so every field of a `RecordRef` is declared here. FLEXIBLE
             -- would be the other route and is deliberately not taken: the shape
             -- is our own closed struct, so declaring it means a field added to
             -- `RecordRef` without a line here fails a test instead of being
             -- dropped on the floor.
             --
             -- Getting this wrong is silent twice over, which is why the whole
             -- row is spelled out: `FLEXIBLE` written after a `.*` companion is
             -- accepted and then stored as `TYPE any` with the FLEXIBLE dropped,
             -- and `Surreal::query` resolves to `Ok` for the write that then
             -- fails — so the answer row never appears and its question stays
             -- pending forever, re-answered and re-paid for on every sweep.
             DEFINE FIELD IF NOT EXISTS records_read ON assistant_messages TYPE option<array>;
             DEFINE FIELD IF NOT EXISTS records_read.* ON assistant_messages TYPE object;
             DEFINE FIELD IF NOT EXISTS records_read.*.kind ON assistant_messages TYPE string;
             DEFINE FIELD IF NOT EXISTS records_read.*.id ON assistant_messages TYPE string;
             DEFINE FIELD IF NOT EXISTS records_read.*.title ON assistant_messages
                 TYPE option<string>;
             DEFINE INDEX IF NOT EXISTS assistant_messages_thread
                 ON assistant_messages FIELDS thread_id;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query("DELETE FROM assistant_messages; DELETE FROM assistant_threads")
            .await?
            .check()?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "assistant_question_asked" => self.on_question(event, db).await,
            "assistant_answer_given" => self.on_answer(event, db).await,
            _ => Ok(()),
        }
    }
}

impl AssistantProjection {
    async fn on_question(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // Skip, never error — the `ConfigProjection::on_set` reasoning. A message
        // this build cannot read was written by a different one, and failing the
        // batch would take the user's unrelated edits down with it.
        let parsed: AssistantQuestionAskedPayload =
            match serde_json::from_value(event.payload.clone()) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        event_id = %event.id,
                        error = %e,
                        "skipping an assistant question this build cannot read"
                    );
                    return Ok(());
                }
            };

        self.upsert_message(
            db,
            &parsed.message_id,
            &parsed.thread_id,
            "user",
            Some(&parsed.text),
            event,
            None,
        )
        .await?;

        self.touch_thread(db, &parsed.thread_id, parsed.title.as_deref(), event)
            .await
    }

    async fn on_answer(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let parsed: AssistantAnswerGivenPayload =
            match serde_json::from_value(event.payload.clone()) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        event_id = %event.id,
                        error = %e,
                        "skipping an assistant answer this build cannot read"
                    );
                    return Ok(());
                }
            };

        self.upsert_message(
            db,
            &parsed.message_id,
            &parsed.thread_id,
            "assistant",
            parsed.text.as_deref(),
            event,
            Some(&parsed),
        )
        .await?;

        // An answer never sets a title: only the thread's first question does.
        self.touch_thread(db, &parsed.thread_id, None, event).await
    }

    /// Write one message row, keyed by `message_id`.
    ///
    /// `UPSERT` on the message id is what makes replay idempotent — `rebuild()`
    /// and `catch_up()` both re-apply events that already landed, and a plain
    /// `CREATE` would fail the second time.
    #[allow(clippy::too_many_arguments)]
    async fn upsert_message(
        &self,
        db: &Database,
        message_id: &str,
        thread_id: &str,
        role: &str,
        text: Option<&str>,
        event: &Event,
        answer: Option<&AssistantAnswerGivenPayload>,
    ) -> Result<(), EventError> {
        let usage = match answer {
            Some(a) => Some(serde_json::to_value(&a.usage).map_err(|e| {
                EventError::Validation(format!("could not serialize answer usage: {e}"))
            })?),
            None => None,
        };
        let records_read = match answer {
            Some(a) => Some(serde_json::to_value(&a.records_read).map_err(|e| {
                EventError::Validation(format!("could not serialize records_read: {e}"))
            })?),
            None => None,
        };

        db.query(
            "UPSERT type::record('assistant_messages', $id) SET
                message_id = $id,
                thread_id = $thread_id,
                role = $role,
                text = $text,
                created_at = type::datetime($ts),
                device_id = $device_id,
                in_reply_to = $in_reply_to,
                stopped = $stopped,
                detail = $detail,
                model = $model,
                usage = $usage,
                elapsed_ms = $elapsed_ms,
                verbs = $verbs,
                records_read = $records_read",
        )
        .bind(("id", message_id.to_string()))
        .bind(("thread_id", thread_id.to_string()))
        .bind(("role", role.to_string()))
        .bind(("text", text.map(str::to_string)))
        .bind(("ts", event.timestamp.to_rfc3339()))
        .bind(("device_id", event.device_id.clone()))
        .bind(("in_reply_to", answer.map(|a| a.in_reply_to.clone())))
        .bind(("stopped", answer.map(|a| a.stopped.clone())))
        .bind(("detail", answer.and_then(|a| a.detail.clone())))
        .bind(("model", answer.and_then(|a| a.model.clone())))
        .bind(("usage", usage))
        .bind(("elapsed_ms", answer.map(|a| a.elapsed_ms as i64)))
        .bind(("verbs", answer.map(|a| a.verbs.clone())))
        .bind(("records_read", records_read))
        .await?
        // ⚠️ `.check()` is load-bearing, not belt-and-braces. `Surreal::query`
        // resolves to `Ok` for a statement that *failed* — the error rides in the
        // `Response` until something calls `.take()` or `.check()`. Without this
        // the projection reports success, writes nothing, and the missing answer
        // row leaves its question pending forever, so the agent re-answers it on
        // every sweep and pays each time. That is exactly what happened.
        .check()?;

        Ok(())
    }

    /// Create the thread row if it is new, and move its clock forward.
    ///
    /// `message_count` is recounted from `assistant_messages` rather than
    /// incremented. Incrementing is correct exactly once per event and this
    /// projection re-applies events routinely — a counter would drift upward on
    /// every rebuild, and drift silently, because nothing else reads a number
    /// that only ever looks plausible.
    ///
    /// `last_message_at` moves forward only, so a late-arriving older message
    /// cannot drag a thread back down a list ordered by it. `created_at` moves
    /// backward only, for the mirror-image reason: sync delivers a thread's
    /// messages in arrival order, which is not authoring order, so the first
    /// message to *land* is not necessarily the first one asked.
    /// ⚠️ The comparisons happen **in Rust**, not in the query. `ConfigProjection`
    /// resolves its own last-write-wins the same way, and the reason to copy it
    /// rather than reach for a conditional `SET` is that this file's queries run
    /// against SurrealDB's embedded engine with no schema check to catch a clause
    /// that parses and means something else. Every construct used here is one the
    /// rest of the codebase already proves.
    async fn touch_thread(
        &self,
        db: &Database,
        thread_id: &str,
        title: Option<&str>,
        event: &Event,
    ) -> Result<(), EventError> {
        // Counting the rows rather than asking the database to aggregate: a
        // thread is a handful of turns, and the cheap aggregate would be the one
        // unverified construct in the file.
        let mut counted = db
            .query("SELECT message_id FROM assistant_messages WHERE thread_id = $thread_id")
            .bind(("thread_id", thread_id.to_string()))
            .await?
            .check()?;
        let ids: Vec<String> = counted.take("message_id").unwrap_or_default();
        let count = ids.len() as i64;

        let mut existing = db
            .query(
                "SELECT <string> created_at AS created_at,
                        <string> last_message_at AS last_message_at,
                        title
                 FROM type::record('assistant_threads', $thread_id) LIMIT 1",
            )
            .bind(("thread_id", thread_id.to_string()))
            .await?
            .check()?;
        let created_at: Option<String> = existing.take("created_at").unwrap_or(None);
        let last_message_at: Option<String> = existing.take("last_message_at").unwrap_or(None);
        let stored_title: Option<String> = existing.take("title").unwrap_or(None);

        let parse = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s).map(|t| t.with_timezone(&chrono::Utc))
        };

        // `created_at` moves backward only and `last_message_at` forward only.
        // Sync delivers a thread's messages in arrival order, which is not
        // authoring order, so the first message to *land* is not necessarily the
        // first one asked — without these two clamps the same conversation would
        // sort differently on two devices.
        let created = match created_at.as_deref().and_then(|s| parse(s).ok()) {
            Some(stored) if stored < event.timestamp => stored,
            _ => event.timestamp,
        };
        let last = match last_message_at.as_deref().and_then(|s| parse(s).ok()) {
            Some(stored) if stored > event.timestamp => stored,
            _ => event.timestamp,
        };
        // A title is set once, by the thread's first question. Keeping the stored
        // one when the incoming event carries none is what stops a follow-up
        // blanking the title it arrives ahead of.
        let title = title.map(str::to_string).or(stored_title);

        db.query(
            "UPSERT type::record('assistant_threads', $thread_id) SET
                thread_id = $thread_id,
                title = $title,
                created_at = type::datetime($created),
                last_message_at = type::datetime($last),
                message_count = $count",
        )
        .bind(("thread_id", thread_id.to_string()))
        .bind(("title", title))
        .bind(("created", created.to_rfc3339()))
        .bind(("last", last.to_rfc3339()))
        .bind(("count", count))
        .await?
        .check()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use crate::events::types::{
        AnswerStop, AnswerUsage, AssistantAnswerGivenPayload, AssistantQuestionAskedPayload,
    };
    use chrono::{Duration, Utc};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn question_at(
        ts: chrono::DateTime<Utc>,
        thread: &str,
        message: &str,
        text: &str,
        title: Option<&str>,
    ) -> Event {
        let payload = AssistantQuestionAskedPayload {
            thread_id: thread.into(),
            message_id: message.into(),
            text: text.into(),
            title: title.map(str::to_string),
        };
        let new = NewEvent::assistant_question_asked("phone", &payload).unwrap();
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

    fn answer_at(
        ts: chrono::DateTime<Utc>,
        thread: &str,
        message: &str,
        in_reply_to: &str,
        text: Option<&str>,
        stopped: AnswerStop,
    ) -> Event {
        let payload = AssistantAnswerGivenPayload {
            thread_id: thread.into(),
            message_id: message.into(),
            in_reply_to: in_reply_to.into(),
            text: text.map(str::to_string),
            stopped: stopped.to_string(),
            detail: None,
            model: Some("test-model".into()),
            usage: AnswerUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                reasoning_tokens: 3,
            },
            elapsed_ms: 1234,
            verbs: vec!["search".into(), "read".into()],
            records_read: vec![],
        };
        let new = NewEvent::assistant_answer_given("agent", &payload).unwrap();
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

    async fn thread_count(db: &Database, thread: &str) -> i64 {
        let mut resp = db
            .query("SELECT message_count FROM type::record('assistant_threads', $t)")
            .bind(("t", thread.to_string()))
            .await
            .unwrap();
        let counts: Vec<i64> = resp.take("message_count").unwrap_or_default();
        counts.first().copied().unwrap_or(-1)
    }

    #[tokio::test]
    async fn a_question_and_its_answer_land_as_two_messages_on_one_thread() {
        let db = test_db().await;
        AssistantProjection.init_schema(&db).await.unwrap();

        let now = Utc::now();
        let q = question_at(
            now,
            "t1",
            "m1",
            "what did I write about rent?",
            Some("rent"),
        );
        let a = answer_at(
            now + Duration::seconds(9),
            "t1",
            "m2",
            "m1",
            Some("you noted on 12 Aug that…"),
            AnswerStop::Answered,
        );
        AssistantProjection.apply(&q, &db).await.unwrap();
        AssistantProjection.apply(&a, &db).await.unwrap();

        assert_eq!(thread_count(&db, "t1").await, 2);

        let mut resp = db
            // ⚠️ `created_at` is selected as well as ordered on: SurrealDB v3
            // rejects `ORDER BY` on a field the projection does not return
            // ("Missing order idiom … in statement selection").
            .query(
                "SELECT role, created_at FROM assistant_messages
                 WHERE thread_id = 't1' ORDER BY created_at",
            )
            .await
            .unwrap();
        let roles: Vec<String> = resp.take("role").unwrap_or_default();
        assert_eq!(roles, vec!["user".to_string(), "assistant".to_string()]);
    }

    /// Replay must not change the answer — `rebuild()` and `catch_up()` both
    /// re-apply events that already landed. This is the test a counter-based
    /// `message_count` fails.
    #[tokio::test]
    async fn applying_the_same_events_twice_is_idempotent() {
        let db = test_db().await;
        AssistantProjection.init_schema(&db).await.unwrap();

        let now = Utc::now();
        let q = question_at(now, "t1", "m1", "hello?", Some("hello"));
        let a = answer_at(
            now + Duration::seconds(2),
            "t1",
            "m2",
            "m1",
            Some("hi"),
            AnswerStop::Answered,
        );
        for _ in 0..2 {
            AssistantProjection.apply(&q, &db).await.unwrap();
            AssistantProjection.apply(&a, &db).await.unwrap();
        }

        assert_eq!(
            thread_count(&db, "t1").await,
            2,
            "replay inflated the message count"
        );
    }

    /// Sync applies in arrival order, which is not authoring order. A thread's
    /// clocks must come out the same either way.
    #[tokio::test]
    async fn arrival_order_does_not_change_a_threads_clocks() {
        let now = Utc::now();
        let first = question_at(now, "t1", "m1", "first", Some("first"));
        let second = question_at(now + Duration::minutes(5), "t1", "m2", "second", None);

        let mut seen = Vec::new();
        for order in [[&first, &second], [&second, &first]] {
            let db = test_db().await;
            AssistantProjection.init_schema(&db).await.unwrap();
            for ev in order {
                AssistantProjection.apply(ev, &db).await.unwrap();
            }
            let mut resp = db
                .query(
                    "SELECT <string> created_at AS created_at, <string> last_message_at AS last,
                     title FROM type::record('assistant_threads', 't1')",
                )
                .await
                .unwrap();
            let created: Vec<String> = resp.take("created_at").unwrap_or_default();
            let last: Vec<String> = resp.take("last").unwrap_or_default();
            let title: Vec<Option<String>> = resp.take("title").unwrap_or_default();
            seen.push((created, last, title));
        }
        assert_eq!(seen[0], seen[1], "arrival order changed the thread row");
        assert_eq!(
            seen[0].2.first().cloned().flatten(),
            Some("first".to_string()),
            "the title did not survive a follow-up arriving first"
        );
    }

    /// A failed run is still a terminal event. If it did not materialize, the
    /// client could not tell it apart from a question still being worked on.
    #[tokio::test]
    async fn a_failed_answer_still_materializes() {
        let db = test_db().await;
        AssistantProjection.init_schema(&db).await.unwrap();

        let now = Utc::now();
        AssistantProjection
            .apply(&question_at(now, "t1", "m1", "q", Some("q")), &db)
            .await
            .unwrap();
        AssistantProjection
            .apply(
                &answer_at(
                    now + Duration::seconds(1),
                    "t1",
                    "m2",
                    "m1",
                    None,
                    AnswerStop::Failed,
                ),
                &db,
            )
            .await
            .unwrap();

        let mut resp = db
            .query("SELECT stopped, text FROM type::record('assistant_messages', 'm2')")
            .await
            .unwrap();
        let stopped: Vec<Option<String>> = resp.take("stopped").unwrap_or_default();
        let text: Vec<Option<String>> = resp.take("text").unwrap_or_default();
        assert_eq!(stopped.first().cloned().flatten(), Some("failed".into()));
        assert_eq!(text.first().cloned().flatten(), None);
    }

    /// A stop reason from a newer build must still close the question. Parsing
    /// it is optional; recording that the run ended is not.
    #[tokio::test]
    async fn an_unknown_stop_reason_still_closes_the_question() {
        let db = test_db().await;
        AssistantProjection.init_schema(&db).await.unwrap();

        let ev = Event {
            id: "e1".into(),
            event_type: "assistant_answer_given".into(),
            aggregate_id: "t1".into(),
            timestamp: Utc::now(),
            device_id: "agent".into(),
            payload: serde_json::json!({
                "thread_id": "t1",
                "message_id": "m2",
                "in_reply_to": "m1",
                "stopped": "quota_exhausted",
            }),
            received_at: None,
        };
        AssistantProjection.apply(&ev, &db).await.unwrap();

        let mut resp = db
            .query("SELECT stopped FROM type::record('assistant_messages', 'm2')")
            .await
            .unwrap();
        let stopped: Vec<Option<String>> = resp.take("stopped").unwrap_or_default();
        assert_eq!(
            stopped.first().cloned().flatten(),
            Some("quota_exhausted".into()),
            "a newer build's stop reason was dropped"
        );
        assert_eq!(AnswerStop::parse("quota_exhausted"), None);
    }

    /// ⚠️ An answer that cites a record must materialize like any other.
    ///
    /// The regression this pins is not obvious: `Surreal::query` returns `Ok`
    /// for a statement that *failed*, holding the error in the `Response` until
    /// something calls `.take()` or `.check()`. A projection that discards the
    /// response therefore reports success while writing nothing, and the row
    /// simply is not there — the agent then re-answers the question on its next
    /// sweep, forever, paying each time.
    #[tokio::test]
    async fn an_answer_citing_a_record_still_materializes() {
        let db = test_db().await;
        AssistantProjection.init_schema(&db).await.unwrap();

        let now = Utc::now();
        let payload = AssistantAnswerGivenPayload {
            thread_id: "t1".into(),
            message_id: "m2".into(),
            in_reply_to: "m1".into(),
            text: Some("you noted…".into()),
            stopped: AnswerStop::Answered.to_string(),
            detail: None,
            model: Some("test-model".into()),
            usage: AnswerUsage::default(),
            elapsed_ms: 10,
            verbs: vec!["search".into(), "read".into()],
            records_read: vec![
                crate::events::RecordRef {
                    kind: "note".into(),
                    id: "01JKNOTE".into(),
                    title: None,
                },
                crate::events::RecordRef {
                    kind: "journal".into(),
                    id: "2026-03-14".into(),
                    title: Some("14 March".into()),
                },
            ],
        };
        let ev = envelope_of(
            NewEvent::assistant_answer_given("agent", &payload).unwrap(),
            now,
        );
        AssistantProjection.apply(&ev, &db).await.unwrap();

        let mut resp = db
            .query("SELECT in_reply_to, records_read FROM type::record('assistant_messages', 'm2')")
            .await
            .unwrap();
        let in_reply_to: Vec<Option<String>> = resp.take("in_reply_to").unwrap_or_default();
        assert_eq!(
            in_reply_to.first().cloned().flatten(),
            Some("m1".into()),
            "the answer row is missing, so its question stays pending forever"
        );
        let cited: Vec<serde_json::Value> = resp.take("records_read").unwrap_or_default();
        assert_eq!(cited.len(), 1, "records_read did not round-trip: {cited:?}");
    }

    fn envelope_of(new: NewEvent, ts: chrono::DateTime<Utc>) -> Event {
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

    /// The payload must survive the real append/read path, not just an in-memory
    /// envelope — it round-trips through SurrealDB as a FLEXIBLE object.
    #[tokio::test]
    async fn the_payload_survives_the_event_store() {
        let db = test_db().await;
        AssistantProjection.init_schema(&db).await.unwrap();
        let store = SurrealEventStore::new(db.clone());

        let payload = AssistantQuestionAskedPayload {
            thread_id: "t1".into(),
            message_id: "m1".into(),
            text: "how many gym days in July?".into(),
            title: Some("gym July".into()),
        };
        let stored = store
            .append(NewEvent::assistant_question_asked("phone", &payload).unwrap())
            .await
            .unwrap();
        assert_eq!(stored.aggregate_id, "t1", "the aggregate is the thread");

        AssistantProjection.apply(&stored, &db).await.unwrap();
        let mut resp = db
            .query("SELECT text FROM type::record('assistant_messages', 'm1')")
            .await
            .unwrap();
        let text: Vec<Option<String>> = resp.take("text").unwrap_or_default();
        assert_eq!(
            text.first().cloned().flatten(),
            Some("how many gym days in July?".into())
        );
    }
}
