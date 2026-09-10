//! Reading conversations back out of the projection.
//!
//! Two questions, both asked by the agent's responder loop: *what is waiting for
//! an answer*, and *what was already said in this thread*. Both live here rather
//! than in the agent binary because both are queries against the read model, and
//! because the answering loop is much easier to test when finding work is
//! separable from doing it.

use chrono::{DateTime, Utc};

use crate::db::Database;
use crate::events::{AssistantQuestionAskedPayload, EventError};

/// How many recent user messages a sweep considers.
///
/// A bound rather than a time window, so the cost of a sweep does not grow with
/// the history and does not depend on a clock. If a question has this many newer
/// messages stacked behind it, the conversation has moved on without it — and the
/// client stops waiting on its own timeout regardless.
const SWEEP_WINDOW: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// One prior turn, as it goes back to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub role: Role,
    pub text: String,
}

/// A question with no answer behind it, and what was said before it.
#[derive(Debug, Clone)]
pub struct PendingQuestion {
    pub question: AssistantQuestionAskedPayload,
    pub asked_at: DateTime<Utc>,
    /// Earlier turns in this thread, oldest first, excluding this question.
    ///
    /// Carried here rather than fetched later because the sweep has already read
    /// the thread to find out the question was unanswered — a second read would
    /// be the same rows, one race window later.
    pub history: Vec<Turn>,
}

impl PendingQuestion {
    /// How long this question has been waiting, against the caller's clock.
    ///
    /// Lives here so the agent does not need `chrono` to compare an event
    /// timestamp against a policy expressed in [`std::time::Duration`]. A
    /// question whose timestamp is in the future — a device with a skewed clock —
    /// reports zero rather than saturating, which keeps it answerable instead of
    /// making it look impossibly old.
    pub fn age(&self) -> std::time::Duration {
        self.age_at(Utc::now())
    }

    /// [`PendingQuestion::age`] against a given clock, so a horizon can be tested
    /// without waiting for one.
    pub fn age_at(&self, now: DateTime<Utc>) -> std::time::Duration {
        (now - self.asked_at)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO)
    }
}

/// One materialized message row.
struct MessageRow {
    message_id: String,
    thread_id: String,
    role: String,
    text: Option<String>,
    created_at: DateTime<Utc>,
    in_reply_to: Option<String>,
}

/// Every message on one thread, oldest first.
async fn thread_messages(db: &Database, thread_id: &str) -> Result<Vec<MessageRow>, EventError> {
    // ⚠️ `created_at` is selected as well as ordered on. SurrealDB v3 rejects
    // `ORDER BY` over a field the projection does not return, and the error names
    // a "missing order idiom", which does not read as the cause.
    let mut resp = db
        .query(
            "SELECT message_id, thread_id, role, text, <string> created_at AS created_at,
                    in_reply_to
             FROM assistant_messages WHERE thread_id = $thread_id ORDER BY created_at",
        )
        .bind(("thread_id", thread_id.to_string()))
        .await?
        .check()?;

    let ids: Vec<String> = resp.take("message_id").unwrap_or_default();
    let threads: Vec<String> = resp.take("thread_id").unwrap_or_default();
    let roles: Vec<String> = resp.take("role").unwrap_or_default();
    let texts: Vec<Option<String>> = resp.take("text").unwrap_or_default();
    let created: Vec<String> = resp.take("created_at").unwrap_or_default();
    let replies: Vec<Option<String>> = resp.take("in_reply_to").unwrap_or_default();

    let mut rows = Vec::with_capacity(ids.len());
    for i in 0..ids.len() {
        let Some(ts) = created
            .get(i)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        else {
            // Loud, and skipped rather than fatal: an unreadable timestamp on one
            // message must not stop the thread it sits in from being answered.
            tracing::warn!(message_id = ?ids.get(i), "assistant message has no readable timestamp");
            continue;
        };
        rows.push(MessageRow {
            message_id: ids[i].clone(),
            thread_id: threads.get(i).cloned().unwrap_or_default(),
            role: roles.get(i).cloned().unwrap_or_default(),
            text: texts.get(i).cloned().flatten(),
            created_at: ts.with_timezone(&Utc),
            in_reply_to: replies.get(i).cloned().flatten(),
        });
    }
    Ok(rows)
}

/// Questions still waiting on an answer, oldest first.
///
/// ⚠️ **Any answer closes a question, including a failed one.** Treating a
/// failure as still-pending would make the loop retry it on the next sweep, which
/// against a broken endpoint is a spend loop with no ceiling. A person who wants
/// another attempt asks again, and that is a new question with its own event.
pub async fn pending_questions(db: &Database) -> Result<Vec<PendingQuestion>, EventError> {
    let mut resp = db
        .query(
            "SELECT thread_id, <string> created_at AS created_at
             FROM assistant_messages WHERE role = 'user'
             ORDER BY created_at DESC LIMIT $limit",
        )
        .bind(("limit", SWEEP_WINDOW as i64))
        .await?
        .check()?;
    let threads: Vec<String> = resp.take("thread_id").unwrap_or_default();

    // Distinct, order-insensitive: the per-thread read below is what actually
    // decides, and re-reading a thread once per message in it would be the
    // sweep's whole cost.
    let mut seen = std::collections::BTreeSet::new();
    let mut pending = Vec::new();
    for thread_id in threads {
        if !seen.insert(thread_id.clone()) {
            continue;
        }
        let rows = thread_messages(db, &thread_id).await?;
        let answered: std::collections::BTreeSet<&str> = rows
            .iter()
            .filter(|r| r.role == "assistant")
            .filter_map(|r| r.in_reply_to.as_deref())
            .collect();

        for (i, row) in rows.iter().enumerate() {
            if row.role != "user" || answered.contains(row.message_id.as_str()) {
                continue;
            }
            pending.push(PendingQuestion {
                question: AssistantQuestionAskedPayload {
                    thread_id: row.thread_id.clone(),
                    message_id: row.message_id.clone(),
                    text: row.text.clone().unwrap_or_default(),
                    // Not read back: the payload is rebuilt only far enough to
                    // author the answer, and a title is a property of the thread
                    // row, which already has it.
                    title: None,
                },
                asked_at: row.created_at,
                history: turns(&rows[..i]),
            });
        }
    }

    // Oldest first: a backlog is worked in the order it was asked, so a question
    // cannot be starved by newer ones arriving while the model is busy.
    pending.sort_by_key(|p| p.asked_at);
    Ok(pending)
}

/// Rows to model turns, dropping anything with nothing to say.
///
/// A failed or budget-exhausted answer has no `text`, and putting it in the
/// history as an empty assistant turn would tell the model it had already replied
/// to something it never saw.
fn turns(rows: &[MessageRow]) -> Vec<Turn> {
    rows.iter()
        .filter_map(|r| {
            let text = r.text.as_deref().filter(|t| !t.trim().is_empty())?;
            let role = match r.role.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => return None,
            };
            Some(Turn {
                role,
                text: text.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{
        AnswerStop, AnswerUsage, AssistantAnswerGivenPayload, AssistantProjection, Event, NewEvent,
        Projection,
    };
    use chrono::Duration;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        AssistantProjection.init_schema(&db).await.unwrap();
        db
    }

    fn envelope(new: NewEvent, ts: DateTime<Utc>) -> Event {
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

    async fn ask(db: &Database, ts: DateTime<Utc>, thread: &str, message: &str, text: &str) {
        let payload = AssistantQuestionAskedPayload {
            thread_id: thread.into(),
            message_id: message.into(),
            text: text.into(),
            title: Some(text.into()),
        };
        let ev = envelope(
            NewEvent::assistant_question_asked("phone", &payload).unwrap(),
            ts,
        );
        AssistantProjection.apply(&ev, db).await.unwrap();
    }

    async fn reply(
        db: &Database,
        ts: DateTime<Utc>,
        thread: &str,
        message: &str,
        in_reply_to: &str,
        text: Option<&str>,
        stopped: AnswerStop,
    ) {
        let payload = AssistantAnswerGivenPayload {
            thread_id: thread.into(),
            message_id: message.into(),
            in_reply_to: in_reply_to.into(),
            text: text.map(str::to_string),
            stopped: stopped.to_string(),
            detail: None,
            model: Some("test-model".into()),
            usage: AnswerUsage::default(),
            elapsed_ms: 0,
            verbs: vec![],
            records_read: vec![],
        };
        let ev = envelope(
            NewEvent::assistant_answer_given("agent", &payload).unwrap(),
            ts,
        );
        AssistantProjection.apply(&ev, db).await.unwrap();
    }

    #[tokio::test]
    async fn an_unanswered_question_is_pending_and_an_answered_one_is_not() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "t1", "m1", "answered one").await;
        reply(
            &db,
            now + Duration::seconds(5),
            "t1",
            "m2",
            "m1",
            Some("here you go"),
            AnswerStop::Answered,
        )
        .await;
        ask(&db, now + Duration::seconds(10), "t1", "m3", "waiting one").await;

        let pending = pending_questions(&db).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].question.message_id, "m3");
        assert_eq!(pending[0].question.thread_id, "t1");
    }

    /// A failed attempt closes the question. Otherwise every sweep retries it,
    /// and against a broken endpoint that is an unbounded spend loop.
    #[tokio::test]
    async fn a_failed_answer_closes_the_question_rather_than_leaving_it_pending() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "t1", "m1", "q").await;
        reply(
            &db,
            now + Duration::seconds(1),
            "t1",
            "m2",
            "m1",
            None,
            AnswerStop::Failed,
        )
        .await;

        assert!(pending_questions(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_follow_up_carries_the_earlier_turns_oldest_first() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "t1", "m1", "what did I write about rent?").await;
        reply(
            &db,
            now + Duration::seconds(5),
            "t1",
            "m2",
            "m1",
            Some("you noted on 12 Aug that…"),
            AnswerStop::Answered,
        )
        .await;
        ask(
            &db,
            now + Duration::seconds(20),
            "t1",
            "m3",
            "what about last month?",
        )
        .await;

        let pending = pending_questions(&db).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].history,
            vec![
                Turn {
                    role: Role::User,
                    text: "what did I write about rent?".into()
                },
                Turn {
                    role: Role::Assistant,
                    text: "you noted on 12 Aug that…".into()
                },
            ]
        );
    }

    /// An answer with no text must not enter the history as an empty assistant
    /// turn — that tells the model it already replied to something it never saw.
    #[tokio::test]
    async fn a_textless_answer_does_not_enter_the_history() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "t1", "m1", "first").await;
        reply(
            &db,
            now + Duration::seconds(1),
            "t1",
            "m2",
            "m1",
            None,
            AnswerStop::TurnBudget,
        )
        .await;
        ask(&db, now + Duration::seconds(2), "t1", "m3", "second").await;

        let pending = pending_questions(&db).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].history,
            vec![Turn {
                role: Role::User,
                text: "first".into()
            }],
            "a textless answer leaked into the history"
        );
    }

    /// Threads are independent: one thread's history must never reach another's.
    #[tokio::test]
    async fn history_does_not_leak_between_threads() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "t1", "m1", "thread one").await;
        reply(
            &db,
            now + Duration::seconds(1),
            "t1",
            "m2",
            "m1",
            Some("reply one"),
            AnswerStop::Answered,
        )
        .await;
        ask(&db, now + Duration::seconds(2), "t2", "m3", "thread two").await;

        let pending = pending_questions(&db).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].question.thread_id, "t2");
        assert!(
            pending[0].history.is_empty(),
            "another thread's turns reached this one: {:?}",
            pending[0].history
        );
    }

    /// A backlog is worked oldest first, so a question cannot be starved by
    /// newer ones arriving while the model is busy.
    #[tokio::test]
    async fn a_backlog_comes_back_oldest_first() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now + Duration::seconds(30), "t3", "m3", "third").await;
        ask(&db, now, "t1", "m1", "first").await;
        ask(&db, now + Duration::seconds(15), "t2", "m2", "second").await;

        let pending = pending_questions(&db).await.unwrap();
        let ids: Vec<&str> = pending
            .iter()
            .map(|p| p.question.message_id.as_str())
            .collect();
        assert_eq!(ids, vec!["m1", "m2", "m3"]);
    }

    /// The horizon reads off this, so a question's age must be measured against
    /// when it was *asked*, not when it was pulled.
    #[tokio::test]
    async fn age_is_measured_from_when_the_question_was_asked() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now - Duration::minutes(90), "t1", "m1", "old one").await;

        let pending = pending_questions(&db).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].age_at(now).as_secs() / 60, 90);
    }

    /// A device with a skewed clock can date a question in the future. That must
    /// read as new, not as impossibly old — a saturating subtraction would push
    /// it past any horizon and it would never be answered.
    #[tokio::test]
    async fn a_question_dated_in_the_future_is_not_treated_as_stale() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now + Duration::minutes(10), "t1", "m1", "from ahead").await;

        let pending = pending_questions(&db).await.unwrap();
        assert_eq!(pending[0].age_at(now), std::time::Duration::ZERO);
    }

    #[tokio::test]
    async fn a_database_with_no_conversations_yields_nothing() {
        let db = test_db().await;
        assert!(pending_questions(&db).await.unwrap().is_empty());
    }
}
