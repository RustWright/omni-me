//! The scheduled check-in: the assistant asking on the user's behalf.
//!
//! ## Why this is a question and not a new subsystem
//!
//! The tempting shape for "initiative" is a task runner with its own outputs, its
//! own storage and its own screen. This is deliberately not that. A scheduled run
//! authors an ordinary `AssistantQuestionAsked`, the ordinary answering loop
//! picks it up, and anything it wants to change goes through the ordinary
//! proposal gate into the ordinary inbox.
//!
//! That is not only less code. It means initiative **inherits every property the
//! ask path already has**: the answer is durable, it syncs to every device, the
//! journal stays unwritable, nothing is applied without approval, and the whole
//! exchange is readable in the same thread list as everything else. A parallel
//! path would have had to re-earn each of those, and would have been the obvious
//! place for one of them to quietly not hold.
//!
//! ## What decides whether it is due
//!
//! [`is_due`] is pure and takes both clocks it needs, so the policy can be tested
//! without waiting a day. The agent owns only the timer.

use chrono::{DateTime, Datelike, Timelike, Utc};
use chrono_tz::Tz;

use crate::config::{ConfigKey, ResolvedConfig};
use crate::db::Database;
use crate::events::EventError;

/// When the last scheduled check-in was raised, or `None` if there has never
/// been one.
///
/// ⚠️ **Derived from the log, not from a state file.** A file would have to be
/// written after each run and would then be wrong in three ordinary situations:
/// a restart between raising the question and writing the file, a fresh data
/// directory, and a second agent that never sees the first one's disk. The
/// question itself is the record that it happened, and it syncs.
pub async fn last_check_in(db: &Database) -> Result<Option<DateTime<Utc>>, EventError> {
    let mut resp = db
        .query(
            "SELECT <string> created_at AS created_at FROM assistant_messages
             WHERE scheduled = true ORDER BY created_at DESC LIMIT 1",
        )
        .await?
        .check()?;
    let rows: Vec<String> = resp.take("created_at").unwrap_or_default();
    Ok(rows
        .first()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc)))
}

/// How long a check-in thread is kept (his ruling, 2026-10-10: archive the
/// previous one, delete after a window). Two weeks to read it or act on it.
pub const KEEP_CHECK_INS_DAYS: i64 = 14;

/// What the agent tidies after raising a check-in.
#[derive(Debug, Default, PartialEq)]
pub struct Housekeeping {
    pub archive: Vec<String>,
    pub delete: Vec<String>,
}

/// One check-in thread, as the plan needs it.
#[derive(Debug, Clone)]
pub struct CheckInThread {
    pub thread_id: String,
    pub created_at: DateTime<Utc>,
    pub archived: bool,
    pub deleted: bool,
    /// A suggestion in it is still undecided.
    pub has_pending: bool,
}

/// Archive every earlier check-in still showing; delete those past the window.
/// A thread with an undecided suggestion is never deleted, so nothing he has
/// not answered disappears.
pub fn plan_housekeeping(
    threads: &[CheckInThread],
    current: &str,
    now: DateTime<Utc>,
) -> Housekeeping {
    let cutoff = now - chrono::Duration::days(KEEP_CHECK_INS_DAYS);
    let mut out = Housekeeping::default();
    for t in threads
        .iter()
        .filter(|t| t.thread_id != current && !t.deleted)
    {
        if t.created_at < cutoff && !t.has_pending {
            out.delete.push(t.thread_id.clone());
        } else if !t.archived {
            out.archive.push(t.thread_id.clone());
        }
    }
    out
}

/// Every check-in thread with its state, for [`plan_housekeeping`].
pub async fn check_in_threads(db: &Database) -> Result<Vec<CheckInThread>, EventError> {
    let mut resp = db
        .query(
            "SELECT thread_id, <string> created_at AS created_at FROM assistant_messages
             WHERE scheduled = true;
             SELECT thread_id, archived, deleted FROM assistant_thread_state;
             SELECT VALUE thread_id FROM assistant_proposals
             WHERE decision IS NONE AND thread_id != NONE;",
        )
        .await?
        .check()?;
    let ids: Vec<String> = resp.take((0, "thread_id")).unwrap_or_default();
    let created: Vec<String> = resp.take((0, "created_at")).unwrap_or_default();
    let state_ids: Vec<String> = resp.take((1, "thread_id")).unwrap_or_default();
    let archived: Vec<Option<bool>> = resp.take((1, "archived")).unwrap_or_default();
    let deleted: Vec<Option<bool>> = resp.take((1, "deleted")).unwrap_or_default();
    let pending: Vec<String> = resp.take(2).unwrap_or_default();

    let state: std::collections::HashMap<String, (bool, bool)> = state_ids
        .into_iter()
        .zip(archived.into_iter().zip(deleted))
        .map(|(id, (a, d))| (id, (a.unwrap_or(false), d.unwrap_or(false))))
        .collect();
    Ok(ids
        .into_iter()
        .zip(created)
        .filter_map(|(thread_id, at)| {
            let created_at = DateTime::parse_from_rfc3339(&at).ok()?.with_timezone(&Utc);
            let (archived, deleted) = state.get(&thread_id).copied().unwrap_or_default();
            let has_pending = pending.contains(&thread_id);
            Some(CheckInThread {
                thread_id,
                created_at,
                archived,
                deleted,
                has_pending,
            })
        })
        .collect())
}

/// The hour is a wall-clock hour, so it is clamped rather than trusted.
///
/// A value outside the range is a typo in shared config that arrived over sync,
/// and refusing to run at all would be a worse answer than running at a sensible
/// hour — the user notices a check-in at the wrong time, they do not notice one
/// that silently never happens.
pub const MIN_HOUR: i64 = 0;
pub const MAX_HOUR: i64 = 23;

/// What the agent needs to know to run check-ins.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckInPolicy {
    pub enabled: bool,
    pub prompt: String,
    pub hour: u32,
    /// The zone the hour and "today" are read in. The agent's own clock is UTC.
    pub timezone: Tz,
}

impl CheckInPolicy {
    /// Read the policy out of resolved config.
    pub fn from_config(config: &ResolvedConfig) -> Self {
        let hour = config
            .int_of(ConfigKey::AssistantCheckInHour)
            .clamp(MIN_HOUR, MAX_HOUR) as u32;
        CheckInPolicy {
            enabled: config.bool_of(ConfigKey::AssistantCheckIn),
            prompt: config.text_of(ConfigKey::AssistantCheckInPrompt),
            hour,
            // Validated on write; an unparseable value from an older build falls
            // back to UTC, the behaviour before the key existed.
            timezone: config
                .text_of(ConfigKey::AssistantCheckInTimezone)
                .parse()
                .unwrap_or(Tz::UTC),
        }
    }
}

/// [`is_due`] against the current clock.
///
/// Exists so the agent does not take a `chrono` dependency to make one call, and
/// so the testable half stays pure — every rule lives in [`is_due`], and this is
/// only the clock read.
pub fn is_due_now(policy: &CheckInPolicy, last_run: Option<DateTime<Utc>>) -> bool {
    is_due(policy, last_run, Utc::now())
}

/// Whether a check-in should run now.
///
/// ⚠️ **Once per calendar day, decided against the last run rather than against a
/// countdown.** An agent restarted four times in an afternoon must not check in
/// four times, and an agent that was down at the scheduled hour should still
/// check in when it comes back rather than skipping the day — a missed day is
/// indistinguishable to the user from the feature being broken. Both fall out of
/// comparing dates instead of measuring intervals.
pub fn is_due(policy: &CheckInPolicy, last_run: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    if !policy.enabled || policy.prompt.trim().is_empty() {
        return false;
    }
    let now = now.with_timezone(&policy.timezone);
    if now.hour() < policy.hour {
        return false;
    }
    match last_run {
        // ⚠️ Compared by ordinal date, not by `now - last < 24h`. An interval test
        // makes the check-in drift later every day, because each run's clock
        // becomes the next one's baseline. Both dates are local, or an evening
        // run lands on tomorrow's UTC date and suppresses tomorrow's.
        Some(last) => {
            let last = last.with_timezone(&policy.timezone);
            (last.year(), last.ordinal()) != (now.year(), now.ordinal())
        }
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigMap, ConfigValue};

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn policy(enabled: bool, hour: u32) -> CheckInPolicy {
        CheckInPolicy {
            enabled,
            prompt: "Review what is due.".to_string(),
            hour,
            timezone: Tz::UTC,
        }
    }

    #[test]
    fn it_is_off_by_default() {
        let config = ResolvedConfig::new(ConfigMap::new(), ConfigMap::new());
        let p = CheckInPolicy::from_config(&config);
        assert!(
            !p.enabled,
            "initiative must be opt-in: a default-on check-in would start \
             proposing on an install that never asked for one"
        );
        assert!(
            !p.prompt.trim().is_empty(),
            "but a prompt is ready when it is"
        );
        assert_eq!(p.hour, 7);
    }

    #[test]
    fn a_disabled_policy_is_never_due() {
        assert!(!is_due(&policy(false, 7), None, at("2026-09-11T09:00:00Z")));
    }

    /// An empty prompt is treated as off rather than sent to the model. Asking a
    /// model nothing costs a request and returns something arbitrary, which then
    /// looks like the assistant volunteering.
    #[test]
    fn an_empty_prompt_is_treated_as_off() {
        let mut p = policy(true, 7);
        p.prompt = "   ".into();
        assert!(!is_due(&p, None, at("2026-09-11T09:00:00Z")));
    }

    #[test]
    fn it_waits_until_the_hour_then_runs_once() {
        let p = policy(true, 7);
        assert!(!is_due(&p, None, at("2026-09-11T06:59:00Z")), "too early");
        assert!(is_due(&p, None, at("2026-09-11T07:00:00Z")), "on the hour");

        let ran = at("2026-09-11T07:00:00Z");
        assert!(
            !is_due(&p, Some(ran), at("2026-09-11T23:00:00Z")),
            "already ran today"
        );
    }

    /// ⚠️ A restart must not re-run it. The agent is restarted casually — a
    /// deploy, an OOM, a config change — and each restart re-reads state.
    #[test]
    fn restarting_the_same_day_does_not_check_in_again() {
        let p = policy(true, 7);
        let ran = at("2026-09-11T07:00:00Z");
        for hour in ["08", "12", "18", "23"] {
            let now = at(&format!("2026-09-11T{hour}:30:00Z"));
            assert!(!is_due(&p, Some(ran), now), "re-ran at {hour}");
        }
    }

    #[test]
    fn the_next_day_is_due_again() {
        let p = policy(true, 7);
        let ran = at("2026-09-11T07:00:00Z");
        assert!(is_due(&p, Some(ran), at("2026-09-12T07:00:00Z")));
    }

    /// ⚠️ An agent that was down at the scheduled hour checks in when it returns,
    /// rather than skipping the day. A silently missed check-in is
    /// indistinguishable from the feature being broken.
    #[test]
    fn a_missed_hour_still_runs_later_the_same_day() {
        let p = policy(true, 7);
        let ran = at("2026-09-10T07:00:00Z");
        assert!(is_due(&p, Some(ran), at("2026-09-11T22:00:00Z")));
    }

    /// ⚠️ Compared by date, not by elapsed time. Under a 24h-interval test this
    /// case is *not* due, and the check-in drifts an hour later every day until
    /// it wanders out of the user's waking hours.
    #[test]
    fn the_schedule_does_not_drift_when_a_run_is_late() {
        let p = policy(true, 7);
        let ran_late = at("2026-09-11T22:00:00Z");
        assert!(
            is_due(&p, Some(ran_late), at("2026-09-12T07:00:00Z")),
            "a late run must not push the next one to 22:00"
        );
    }

    /// A year boundary is where an ordinal-only comparison breaks: day 1 of one
    /// year matches day 1 of the next.
    #[test]
    fn the_same_ordinal_in_a_different_year_is_due() {
        let p = policy(true, 0);
        let ran = at("2025-01-01T09:00:00Z");
        assert!(is_due(&p, Some(ran), at("2026-01-01T09:00:00Z")));
    }

    /// 2026-10-08 on dev: hour 6 meant 02:00 in Toronto, because the hour was
    /// read off the agent's UTC clock.
    #[test]
    fn the_hour_and_the_day_are_the_users_not_the_agents() {
        let mut p = policy(true, 6);
        p.timezone = "America/Toronto".parse().unwrap();
        // 06:00 EDT is 10:00 UTC.
        assert!(!is_due(&p, None, at("2026-10-09T09:59:00Z")), "05:59 local");
        assert!(is_due(&p, None, at("2026-10-09T10:00:00Z")), "06:00 local");
        // Ran 21:00 local on the 8th, which is already the 9th in UTC.
        let ran = at("2026-10-09T01:00:00Z");
        assert!(
            is_due(&p, Some(ran), at("2026-10-09T10:00:00Z")),
            "yesterday evening locally must not count as today"
        );
    }

    #[test]
    fn the_zone_is_read_from_config() {
        let mut global = ConfigMap::new();
        global.insert(
            ConfigKey::AssistantCheckInTimezone,
            ConfigValue::Text("America/Toronto".into()),
        );
        let config = ResolvedConfig::new(global, ConfigMap::new());
        assert_eq!(
            CheckInPolicy::from_config(&config).timezone,
            chrono_tz::America::Toronto
        );
        assert!(
            ConfigKey::AssistantCheckInTimezone
                .validate(&ConfigValue::Text("Mars/Base".into()))
                .is_err()
        );
    }

    fn thread(id: &str, days_ago: i64, archived: bool, has_pending: bool) -> CheckInThread {
        CheckInThread {
            thread_id: id.into(),
            created_at: at("2026-10-20T10:00:00Z") - chrono::Duration::days(days_ago),
            archived,
            deleted: false,
            has_pending,
        }
    }

    #[test]
    fn housekeeping_archives_the_previous_and_deletes_past_the_window() {
        let threads = vec![
            thread("today", 0, false, false),
            thread("yesterday", 1, false, false),
            thread("last-week", 7, true, false),
            thread("old", 20, true, false),
            thread("old-but-undecided", 20, true, true),
        ];
        let plan = plan_housekeeping(&threads, "today", at("2026-10-20T10:00:00Z"));
        assert_eq!(plan.archive, vec!["yesterday".to_string()]);
        assert_eq!(plan.delete, vec!["old".to_string()]);
    }

    #[tokio::test]
    async fn check_in_threads_reads_every_scheduled_thread_with_its_state() {
        use crate::events::{
            AssistantProjection, AssistantQuestionAskedPayload, AssistantThreadArchivedPayload,
            Event, EventType, NewEvent, Projection,
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
        for (thread, scheduled) in [("c1", true), ("c2", true), ("user", false)] {
            let p = AssistantQuestionAskedPayload {
                thread_id: thread.into(),
                message_id: format!("m-{thread}"),
                text: "Review what is due.".into(),
                title: None,
                scheduled,
            };
            apply(NewEvent::assistant_question_asked("agent", &p).unwrap()).await;
        }
        let archived = AssistantThreadArchivedPayload {
            thread_id: "c1".into(),
            archived: true,
        };
        apply(NewEvent {
            id: None,
            event_type: EventType::AssistantThreadArchived.to_string(),
            aggregate_id: "c1".into(),
            timestamp: Utc::now(),
            device_id: "phone".into(),
            payload: serde_json::to_value(&archived).unwrap(),
        })
        .await;

        let mut threads = check_in_threads(&db).await.unwrap();
        threads.sort_by(|a, b| a.thread_id.cmp(&b.thread_id));
        let ids: Vec<&str> = threads.iter().map(|t| t.thread_id.as_str()).collect();
        assert_eq!(ids, ["c1", "c2"], "a typed question is never housekept");
        assert!(threads[0].archived && !threads[1].archived);
    }

    #[test]
    fn an_out_of_range_hour_is_clamped_rather_than_disabling_it() {
        let mut global = ConfigMap::new();
        global.insert(ConfigKey::AssistantCheckInHour, ConfigValue::Int(99));
        let config = ResolvedConfig::new(global, ConfigMap::new());
        assert_eq!(CheckInPolicy::from_config(&config).hour, 23);

        let mut global = ConfigMap::new();
        global.insert(ConfigKey::AssistantCheckInHour, ConfigValue::Int(-4));
        let config = ResolvedConfig::new(global, ConfigMap::new());
        assert_eq!(CheckInPolicy::from_config(&config).hour, 0);
    }
}
