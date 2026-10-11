//! Keeping the chat list short: idle chats are archived, long-archived ones
//! deleted. His ruling, 2026-10-11; both windows are settings. A delete is an
//! event like any other, so the conversation stays in the log.

use chrono::{DateTime, Utc};

use crate::config::{ConfigKey, ResolvedConfig};
use crate::db::Database;
use crate::events::{
    AssistantThreadArchivedPayload, AssistantThreadDeletedPayload, EventError, EventType, NewEvent,
};

/// Threads to archive and to delete.
#[derive(Debug, Default, PartialEq)]
pub struct Housekeeping {
    pub archive: Vec<String>,
    pub delete: Vec<String>,
}

impl Housekeeping {
    pub fn is_empty(&self) -> bool {
        self.archive.is_empty() && self.delete.is_empty()
    }
}

/// One chat, as the plan needs it.
#[derive(Debug, Clone)]
pub struct ChatThread {
    pub thread_id: String,
    pub last_message_at: DateTime<Utc>,
    pub archived: bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub deleted: bool,
    /// A suggestion in it is still undecided.
    pub has_pending: bool,
}

/// The two windows, in days, read from config.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatRetention {
    pub archive_after_days: i64,
    pub delete_after_days: i64,
}

impl ChatRetention {
    pub fn from_config(config: &ResolvedConfig) -> Self {
        Self {
            archive_after_days: config.int_of(ConfigKey::AssistantChatArchiveDays),
            delete_after_days: config.int_of(ConfigKey::AssistantChatDeleteDays),
        }
    }
}

/// Archive chats idle past the first window; delete archived ones past the
/// second. A chat with an undecided suggestion is never deleted, and one written
/// in since it was archived is not deleted until that message is old too.
pub fn plan(threads: &[ChatThread], rules: ChatRetention, now: DateTime<Utc>) -> Housekeeping {
    let idle_cutoff = now - chrono::Duration::days(rules.archive_after_days);
    let delete_cutoff = now - chrono::Duration::days(rules.delete_after_days);
    let mut out = Housekeeping::default();
    for t in threads.iter().filter(|t| !t.deleted) {
        if t.archived {
            let since = t.archived_at.unwrap_or(t.last_message_at);
            if since < delete_cutoff && t.last_message_at < delete_cutoff && !t.has_pending {
                out.delete.push(t.thread_id.clone());
            }
        } else if t.last_message_at < idle_cutoff {
            out.archive.push(t.thread_id.clone());
        }
    }
    out
}

/// The events that carry out a plan, signed by `device_id`.
pub fn events(plan: &Housekeeping, device_id: &str) -> Vec<NewEvent> {
    let event = |event_type: EventType, id: &str, payload: serde_json::Value| NewEvent {
        id: None,
        event_type: event_type.to_string(),
        aggregate_id: id.to_string(),
        timestamp: Utc::now(),
        device_id: device_id.to_string(),
        payload,
    };
    let mut out = Vec::new();
    for id in &plan.archive {
        let p = AssistantThreadArchivedPayload {
            thread_id: id.clone(),
            archived: true,
        };
        if let Ok(v) = serde_json::to_value(&p) {
            out.push(event(EventType::AssistantThreadArchived, id, v));
        }
    }
    for id in &plan.delete {
        let p = AssistantThreadDeletedPayload {
            thread_id: id.clone(),
        };
        if let Ok(v) = serde_json::to_value(&p) {
            out.push(event(EventType::AssistantThreadDeleted, id, v));
        }
    }
    out
}

/// [`plan`] against the current clock, as events. Here rather than in the
/// agent, which takes no `chrono` dependency.
pub fn tidy_now(
    threads: &[ChatThread],
    config: &ResolvedConfig,
    device_id: &str,
) -> (Housekeeping, Vec<NewEvent>) {
    let plan = plan(threads, ChatRetention::from_config(config), Utc::now());
    let events = events(&plan, device_id);
    (plan, events)
}

/// Every chat with its state, for [`plan`].
pub async fn chat_threads(db: &Database) -> Result<Vec<ChatThread>, EventError> {
    let mut resp = db
        .query(
            "SELECT thread_id, <string> last_message_at AS last_message_at FROM assistant_threads;
             SELECT thread_id, archived, deleted FROM assistant_thread_state;
             SELECT thread_id, <string> archived_at AS archived_at FROM assistant_thread_state
             WHERE archived_at != NONE;
             SELECT VALUE thread_id FROM assistant_proposals
             WHERE decision IS NONE AND thread_id != NONE;",
        )
        .await?
        .check()?;
    let ids: Vec<String> = resp.take((0, "thread_id")).unwrap_or_default();
    let last: Vec<String> = resp.take((0, "last_message_at")).unwrap_or_default();
    let state_ids: Vec<String> = resp.take((1, "thread_id")).unwrap_or_default();
    let archived: Vec<Option<bool>> = resp.take((1, "archived")).unwrap_or_default();
    let deleted: Vec<Option<bool>> = resp.take((1, "deleted")).unwrap_or_default();
    let stamped_ids: Vec<String> = resp.take((2, "thread_id")).unwrap_or_default();
    let stamps: Vec<String> = resp.take((2, "archived_at")).unwrap_or_default();
    let pending: Vec<String> = resp.take(3).unwrap_or_default();

    let parse = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&Utc))
    };
    let stamped: std::collections::HashMap<String, DateTime<Utc>> = stamped_ids
        .into_iter()
        .zip(stamps)
        .filter_map(|(id, at)| Some((id, parse(&at)?)))
        .collect();
    let state: std::collections::HashMap<String, (bool, bool, Option<DateTime<Utc>>)> = state_ids
        .into_iter()
        .zip(archived.into_iter().zip(deleted))
        .map(|(id, (a, d))| {
            let at = stamped.get(&id).copied();
            (id, (a.unwrap_or(false), d.unwrap_or(false), at))
        })
        .collect();
    Ok(ids
        .into_iter()
        .zip(last)
        .filter_map(|(thread_id, at)| {
            let last_message_at = parse(&at)?;
            let (archived, deleted, archived_at) =
                state.get(&thread_id).copied().unwrap_or_default();
            let has_pending = pending.contains(&thread_id);
            Some(ChatThread {
                thread_id,
                last_message_at,
                archived,
                archived_at,
                deleted,
                has_pending,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    const NOW: &str = "2026-10-20T10:00:00Z";
    const RULES: ChatRetention = ChatRetention {
        archive_after_days: 30,
        delete_after_days: 90,
    };

    fn chat(id: &str, idle_days: i64, archived_days: Option<i64>, pending: bool) -> ChatThread {
        let ago = |d| at(NOW) - chrono::Duration::days(d);
        ChatThread {
            thread_id: id.into(),
            last_message_at: ago(idle_days),
            archived: archived_days.is_some(),
            archived_at: archived_days.map(ago),
            deleted: false,
            has_pending: pending,
        }
    }

    #[test]
    fn idle_chats_are_archived_and_long_archived_ones_deleted() {
        let threads = vec![
            chat("fresh", 3, None, false),
            chat("idle", 31, None, false),
            chat("idle-undecided", 31, None, true),
            chat("archived-recently", 200, Some(10), false),
            chat("archived-long-ago", 200, Some(91), false),
            chat("archived-long-ago-undecided", 200, Some(91), true),
        ];
        let plan = plan(&threads, RULES, at(NOW));
        assert_eq!(plan.archive, ["idle", "idle-undecided"]);
        assert_eq!(plan.delete, ["archived-long-ago"]);
    }

    #[test]
    fn a_chat_written_in_after_archiving_is_not_deleted() {
        let threads = vec![chat("revived", 5, Some(120), false)];
        assert!(plan(&threads, RULES, at(NOW)).is_empty());
    }

    #[test]
    fn deleted_chats_are_left_alone() {
        let mut gone = chat("gone", 400, Some(400), false);
        gone.deleted = true;
        assert!(plan(&[gone], RULES, at(NOW)).is_empty());
    }

    #[test]
    fn the_windows_come_from_config() {
        let config = ResolvedConfig::default();
        assert_eq!(
            ChatRetention::from_config(&config),
            ChatRetention {
                archive_after_days: 30,
                delete_after_days: 90
            }
        );
    }

    #[tokio::test]
    async fn chat_threads_reads_each_chat_with_its_state() {
        use crate::events::{
            AssistantProjection, AssistantQuestionAskedPayload, Event, Projection,
        };
        let db = crate::db::test_db().await;
        AssistantProjection.init_schema(&db).await.unwrap();
        let apply = |new: NewEvent| {
            let db = db.clone();
            async move {
                let ev = Event {
                    id: ulid::Ulid::new().to_string(),
                    event_type: new.event_type,
                    aggregate_id: new.aggregate_id,
                    timestamp: Utc::now(),
                    device_id: new.device_id,
                    payload: new.payload,
                    received_at: None,
                };
                AssistantProjection.apply(&ev, &db).await.unwrap();
            }
        };
        for thread in ["a", "b"] {
            let p = AssistantQuestionAskedPayload {
                thread_id: thread.into(),
                message_id: format!("m-{thread}"),
                text: "What did I buy?".into(),
                title: None,
                scheduled: false,
            };
            apply(NewEvent::assistant_question_asked("phone", &p).unwrap()).await;
        }
        let plan = Housekeeping {
            archive: vec!["a".into()],
            delete: vec![],
        };
        for e in events(&plan, "agent") {
            apply(e).await;
        }

        let mut threads = chat_threads(&db).await.unwrap();
        threads.sort_by(|x, y| x.thread_id.cmp(&y.thread_id));
        assert_eq!(threads.len(), 2);
        assert!(threads[0].archived && threads[0].archived_at.is_some());
        assert!(!threads[1].archived && threads[1].archived_at.is_none());
    }
}
