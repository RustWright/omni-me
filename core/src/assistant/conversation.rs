//! Reading conversations back out of the projection.
//!
//! Two questions, both asked by the agent's responder loop: *what is waiting for
//! an answer*, and *what was already said in this thread*. Both live here rather
//! than in the agent binary because both are queries against the read model, and
//! because the answering loop is much easier to test when finding work is
//! separable from doing it.

use chrono::{DateTime, Utc};

use serde::Serialize;
use surrealdb::types::{SurrealValue, Value as DbValue};

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
                    // Neither is read back: the payload is rebuilt only far enough
                    // to author the answer. A title is a property of the thread
                    // row, which already has it, and `scheduled` is a fact about
                    // how the *question* was raised that only its own event and
                    // the message row carry — the answer does not restate it.
                    title: None,
                    scheduled: false,
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

/// A thread as the thread list renders it.
///
/// The list reads this table rather than aggregating `assistant_messages`,
/// which is the whole reason the projection keeps two tables: opening the
/// Assistant tab must not drag every message body it has ever stored across
/// the IPC boundary to show a dozen titles.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct ThreadSummary {
    pub thread_id: String,
    /// Set from the thread's first question only, so it can be `None` on a
    /// thread whose opening question has not synced yet.
    pub title: Option<String>,
    pub created_at: String,
    pub last_message_at: String,
    pub message_count: i64,
}

/// One message, with everything the client needs to render it.
///
/// `usage` and `records_read` stay [`DbValue`] rather than typed structs: they
/// are stored shapes owned by the event payload, and re-declaring them here
/// would be a second definition to keep in step with the first. The client
/// reads them as JSON either way.
///
/// ⚠️ **`records_read` carries no title** — see
/// [`crate::assistant::answer::records_read`] for why, and resolve the display
/// name against the record tables at render time.
#[derive(Debug, Clone, Serialize, SurrealValue)]
pub struct ConversationMessage {
    pub message_id: String,
    pub thread_id: String,
    /// `user` or `assistant`.
    pub role: String,
    /// `None` on an answer that failed, ran out of turns, or was skipped as
    /// stale — [`stopped`](Self::stopped) says which.
    pub text: Option<String>,
    pub created_at: String,
    pub in_reply_to: Option<String>,
    pub stopped: Option<String>,
    /// The provider's failure sentence, when there was one.
    pub detail: Option<String>,
    pub model: Option<String>,
    pub elapsed_ms: Option<i64>,
    pub verbs: Option<Vec<String>>,
    pub records_read: Option<DbValue>,
    pub usage: Option<DbValue>,
    /// True when the agent raised this question on a schedule. `None` on answers
    /// and on every question authored before the field existed.
    pub scheduled: Option<bool>,
}

/// One thread, plus whether it is still waiting on an answer.
///
/// **The pending flag is computed here, not in the client.** "Unanswered" is the
/// same judgement the responder's sweep makes, and two implementations of it
/// would disagree the first time a `stopped` variant is added — the client would
/// spin on a question the agent considers closed.
#[derive(Debug, Clone, Serialize)]
pub struct ThreadView {
    pub messages: Vec<ConversationMessage>,
    /// The `message_id` of the question still waiting, if any.
    pub pending: Option<String>,
    /// When that question was asked, so the UI can decide when to say the
    /// assistant may be offline without needing a second clock source.
    pub pending_since: Option<String>,
}

/// Every thread, most recently active first.
pub async fn list_threads(db: &Database) -> Result<Vec<ThreadSummary>, EventError> {
    // ⚠️ `last_message_at` is selected as well as ordered on — SurrealDB v3
    // rejects `ORDER BY` over a field the projection does not return, and names
    // it a "missing order idiom", which does not read as the cause. Ordering on
    // the `<string>` alias is chronological because SurrealDB renders datetimes
    // in a fixed RFC 3339 form, so lexical order matches.
    let mut resp = db
        .query(
            "SELECT thread_id, title, message_count,
                    <string> created_at AS created_at,
                    <string> last_message_at AS last_message_at
             FROM assistant_threads ORDER BY last_message_at DESC",
        )
        .await?
        .check()?;
    Ok(resp.take(0)?)
}

/// One thread's messages, oldest first, with its pending question resolved.
pub async fn read_thread(db: &Database, thread_id: &str) -> Result<ThreadView, EventError> {
    let mut resp = db
        .query(
            "SELECT message_id, thread_id, role, text, in_reply_to, stopped, detail,
                    model, elapsed_ms, verbs, records_read, usage, scheduled,
                    <string> created_at AS created_at
             FROM assistant_messages WHERE thread_id = $thread_id ORDER BY created_at",
        )
        .bind(("thread_id", thread_id.to_string()))
        .await?
        .check()?;
    let messages: Vec<ConversationMessage> = resp.take(0)?;

    // Any answer closes a question, including a failed one — the same rule the
    // sweep applies (see `pending_questions`). Only the newest unanswered
    // question is reported: an older one the agent has already passed over is
    // not something the client should keep a spinner on.
    let answered: std::collections::BTreeSet<&str> = messages
        .iter()
        .filter(|m| m.role == "assistant")
        .filter_map(|m| m.in_reply_to.as_deref())
        .collect();
    let pending = messages
        .iter()
        .rev()
        .find(|m| m.role == "user" && !answered.contains(m.message_id.as_str()));

    Ok(ThreadView {
        pending: pending.map(|m| m.message_id.clone()),
        pending_since: pending.map(|m| m.created_at.clone()),
        messages,
    })
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
        Projection, RecordRef,
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
            scheduled: false,
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

    /// Like [`reply`], but carrying citations — the shape the SCHEMAFULL table
    /// rejected until `records_read`'s subfields were declared.
    async fn reply_citing(
        db: &Database,
        ts: DateTime<Utc>,
        thread: &str,
        message: &str,
        in_reply_to: &str,
        text: Option<&str>,
        records_read: Vec<RecordRef>,
    ) {
        let payload = AssistantAnswerGivenPayload {
            thread_id: thread.into(),
            message_id: message.into(),
            in_reply_to: in_reply_to.into(),
            text: text.map(str::to_string),
            stopped: AnswerStop::Answered.to_string(),
            detail: None,
            model: Some("test-model".into()),
            usage: AnswerUsage::default(),
            elapsed_ms: 0,
            verbs: vec!["read".into()],
            records_read,
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

    #[tokio::test]
    async fn threads_list_most_recently_active_first() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "old", "m1", "asked first").await;
        ask(&db, now + Duration::seconds(60), "new", "m2", "asked later").await;
        // The older thread then gets a reply, which should lift it back to the top.
        reply(
            &db,
            now + Duration::seconds(120),
            "old",
            "m3",
            "m1",
            Some("late answer"),
            AnswerStop::Answered,
        )
        .await;

        let threads = list_threads(&db).await.unwrap();
        assert_eq!(
            threads
                .iter()
                .map(|t| t.thread_id.as_str())
                .collect::<Vec<_>>(),
            ["old", "new"],
            "ordering is by last activity, not by when the thread was started"
        );
        let old = &threads[0];
        assert_eq!(old.message_count, 2);
        assert_eq!(old.title.as_deref(), Some("asked first"));
    }

    #[tokio::test]
    async fn a_thread_read_reports_the_question_still_waiting() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "t1", "m1", "first").await;
        reply(
            &db,
            now + Duration::seconds(5),
            "t1",
            "m2",
            "m1",
            Some("answered"),
            AnswerStop::Answered,
        )
        .await;
        ask(&db, now + Duration::seconds(10), "t1", "m3", "second").await;

        let view = read_thread(&db, "t1").await.unwrap();
        assert_eq!(
            view.messages
                .iter()
                .map(|m| m.message_id.as_str())
                .collect::<Vec<_>>(),
            ["m1", "m2", "m3"],
            "oldest first, so the client can render it as a transcript"
        );
        assert_eq!(view.pending.as_deref(), Some("m3"));
        assert!(view.pending_since.is_some());
    }

    /// The same rule the sweep applies: any answer closes a question. If these
    /// two ever disagree the client spins forever on a question the agent is
    /// finished with.
    #[tokio::test]
    async fn a_failed_answer_leaves_nothing_pending_for_the_client_either() {
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

        let view = read_thread(&db, "t1").await.unwrap();
        assert_eq!(view.pending, None);
        assert_eq!(view.messages[1].stopped.as_deref(), Some("failed"));
        assert_eq!(
            view.messages[1].text, None,
            "a failed answer renders no prose"
        );
    }

    /// ⚠️ The whole round trip for `scheduled`: event → projection → read model.
    /// Every hop is somewhere the field can be silently dropped, and if it is,
    /// the client's "asked on your behalf" marker simply never appears — a
    /// failure that looks like nothing at all.
    #[tokio::test]
    async fn a_scheduled_question_is_marked_all_the_way_to_the_read_model() {
        let db = test_db().await;
        let now = Utc::now();

        let payload = AssistantQuestionAskedPayload {
            thread_id: "t1".into(),
            message_id: "m1".into(),
            text: "Review what is due.".into(),
            title: Some("Daily check-in".into()),
            scheduled: true,
        };
        let ev = envelope(
            NewEvent::assistant_question_asked("agent", &payload).unwrap(),
            now,
        );
        AssistantProjection.apply(&ev, &db).await.unwrap();

        let view = read_thread(&db, "t1").await.unwrap();
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].scheduled, Some(true));

        // And the scheduler reads its own last run from exactly this row.
        let last = crate::assistant::check_in::last_check_in(&db)
            .await
            .unwrap();
        assert!(
            last.is_some(),
            "the check-in must be findable as its own record"
        );
    }

    /// The mirror: an ordinary question must not look scheduled, or every answer
    /// would carry a marker saying the user did not ask for it.
    #[tokio::test]
    async fn a_user_asked_question_is_not_marked_scheduled() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "t1", "m1", "what did I write about rent?").await;

        let view = read_thread(&db, "t1").await.unwrap();
        assert_ne!(view.messages[0].scheduled, Some(true));
        assert!(
            crate::assistant::check_in::last_check_in(&db)
                .await
                .unwrap()
                .is_none(),
            "a user's question must never count as a check-in, or the schedule \
             would skip a day every time they asked something"
        );
    }

    #[tokio::test]
    async fn an_answers_citations_survive_the_read() {
        let db = test_db().await;
        let now = Utc::now();
        ask(&db, now, "t1", "m1", "what did I write about rent?").await;
        reply_citing(
            &db,
            now + Duration::seconds(3),
            "t1",
            "m2",
            "m1",
            Some("you wrote a note"),
            vec![RecordRef {
                kind: "note".into(),
                id: "note-1".into(),
                title: None,
            }],
        )
        .await;

        let view = read_thread(&db, "t1").await.unwrap();
        let cited = view.messages[1]
            .records_read
            .clone()
            .expect("the citation should survive the round trip");
        let json = cited.into_json_value();
        assert_eq!(json[0]["kind"], "note");
        assert_eq!(json[0]["id"], "note-1");
    }

    #[tokio::test]
    async fn reading_a_thread_that_does_not_exist_is_empty_rather_than_an_error() {
        let db = test_db().await;
        let view = read_thread(&db, "nope").await.unwrap();
        assert!(view.messages.is_empty());
        assert_eq!(view.pending, None);
    }
}
