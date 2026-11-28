//! Scheduled prompts: the assistant asking on the user's behalf.
//!
//! Each is an ordinary `AssistantQuestionAsked`, not a subsystem, so initiative inherits every
//! property of the ask path (durable, synced, journal unwritable, approval-gated). Each prompt has
//! a name, carried on the question, so its due-check and housekeeping see only its own runs.
//! [`is_due`] is pure, taking both clocks; the agent owns only the timer.

use chrono::{DateTime, Datelike, Months, NaiveDate, Timelike, Utc, Weekday};
use chrono_tz::Tz;

use crate::config::{ConfigKey, ResolvedConfig};
use crate::db::Database;
use crate::events::{EventError, LEGACY_SCHEDULED_PROMPT};
use crate::jobs::Tier;

pub use super::chat_tidy::Housekeeping;

/// The daily check-in. Also the name every scheduled question from before names existed reads as.
pub const CHECK_IN: &str = LEGACY_SCHEDULED_PROMPT;

/// How long a check-in thread is kept: archive the previous one, delete after a
/// window. Two weeks to read it or act on it. Ruling 19 (RULINGS.md).
pub const KEEP_CHECK_INS_DAYS: i64 = 14;

/// The hour is a wall-clock hour from shared config, so it is clamped rather than trusted: a
/// check-in at the wrong time gets noticed, one that silently never runs does not.
pub const MIN_HOUR: i64 = 0;
pub const MAX_HOUR: i64 = 23;

/// How often a prompt runs. Each run belongs to a period, and a prompt is due once per period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    Daily,
    /// The week starts on this day.
    Weekly(Weekday),
    /// The month's period starts on this day, clamped to 1-28 so every month has it.
    Monthly(u32),
}

impl Cadence {
    /// The first day of the period `day` falls in.
    pub fn period_start(self, day: NaiveDate) -> NaiveDate {
        match self {
            Cadence::Daily => day,
            Cadence::Weekly(start) => {
                let back =
                    (day.weekday().num_days_from_monday() + 7 - start.num_days_from_monday()) % 7;
                day - chrono::Duration::days(i64::from(back))
            }
            Cadence::Monthly(d) => {
                let this = day
                    .with_day(d.clamp(1, 28))
                    .expect("days 1-28 exist in every month");
                if day >= this {
                    this
                } else {
                    this - Months::new(1)
                }
            }
        }
    }
}

/// One named prompt the agent raises on a schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledPrompt {
    /// Written on every question it raises and read back by the due-check. Never rename one: its
    /// history would stop counting and it would run again the same day.
    pub name: &'static str,
    /// The thread title of each question it raises.
    pub title: &'static str,
    pub text: String,
    pub enabled: bool,
    pub cadence: Cadence,
    pub hour: u32,
    /// The zone the hour and "today" are read in. The agent's own clock is UTC.
    pub timezone: Tz,
    /// How it reaches the user if they never open chat: ruling 51 (RULINGS.md).
    pub tier: Tier,
    /// Its threads are deleted this many days after they were raised.
    pub keep_days: i64,
}

/// Every scheduled prompt, read out of resolved config.
pub fn prompts(config: &ResolvedConfig) -> Vec<ScheduledPrompt> {
    vec![check_in(config)]
}

/// The user's zone. Validated on write; an unparseable value from an older build falls back to
/// UTC, the behaviour before the key existed.
pub fn user_timezone(config: &ResolvedConfig) -> Tz {
    config
        .text_of(ConfigKey::AssistantCheckInTimezone)
        .parse()
        .unwrap_or(Tz::UTC)
}

/// The check-in is Ambient: ruling 53 (RULINGS.md).
pub fn check_in(config: &ResolvedConfig) -> ScheduledPrompt {
    ScheduledPrompt {
        name: CHECK_IN,
        title: "Daily check-in",
        text: config.text_of(ConfigKey::AssistantCheckInPrompt),
        enabled: config.bool_of(ConfigKey::AssistantCheckIn),
        cadence: Cadence::Daily,
        hour: config
            .int_of(ConfigKey::AssistantCheckInHour)
            .clamp(MIN_HOUR, MAX_HOUR) as u32,
        timezone: user_timezone(config),
        tier: Tier::Ambient,
        keep_days: KEEP_CHECK_INS_DAYS,
    }
}

/// When `prompt` last raised a question, or `None` if it never has.
///
/// From the log, never a state file, which a restart, a fresh data dir or a second agent would
/// make wrong. The question itself is the record, and it syncs.
pub async fn last_run(db: &Database, prompt: &str) -> Result<Option<DateTime<Utc>>, EventError> {
    let mut resp = db
        .query(
            "SELECT <string> created_at AS created_at FROM assistant_messages
             WHERE prompt_name = $prompt ORDER BY created_at DESC LIMIT 1",
        )
        .bind(("prompt", prompt.to_string()))
        .await?
        .check()?;
    let rows: Vec<String> = resp.take("created_at").unwrap_or_default();
    Ok(rows
        .first()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc)))
}

/// [`is_due`] against the current clock, so the agent takes no `chrono` dependency.
pub fn is_due_now(prompt: &ScheduledPrompt, last_run: Option<DateTime<Utc>>) -> bool {
    is_due(prompt, last_run, Utc::now())
}

/// Whether `prompt` should run now.
///
/// Once per period against the last run, never a countdown: restarts must not repeat it, and an
/// agent down at the hour still runs it on return. Never before the hour on any day.
pub fn is_due(
    prompt: &ScheduledPrompt,
    last_run: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    if !prompt.enabled || prompt.text.trim().is_empty() {
        return false;
    }
    let now = now.with_timezone(&prompt.timezone);
    if now.hour() < prompt.hour {
        return false;
    }
    // Compared by local period, not elapsed time: an interval test drifts later each run, and a
    // UTC date puts an evening run on tomorrow.
    let today = now.date_naive();
    let current = prompt.cadence.period_start(today);
    match last_run {
        Some(last) => {
            let last = last.with_timezone(&prompt.timezone).date_naive();
            prompt.cadence.period_start(last) < current
        }
        // A first run waits for its own day: a weekly plan on a Wednesday, or a monthly review
        // on the 26th, would ask about a period mostly gone. Only later runs catch up.
        None => current == today,
    }
}

/// One scheduled thread, as the plan needs it.
#[derive(Debug, Clone)]
pub struct ScheduledThread {
    pub thread_id: String,
    pub created_at: DateTime<Utc>,
    pub archived: bool,
    pub deleted: bool,
    /// A suggestion in it is still undecided.
    pub has_pending: bool,
}

/// Archive every earlier thread still showing; delete those past `keep_days`.
/// A thread with an undecided suggestion is never deleted, so nothing the user
/// has not answered disappears.
pub fn plan_housekeeping(
    threads: &[ScheduledThread],
    current: &str,
    keep_days: i64,
    now: DateTime<Utc>,
) -> Housekeeping {
    let cutoff = now - chrono::Duration::days(keep_days);
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

/// [`plan_housekeeping`] against the current clock, as the events that carry it out.
pub fn housekeeping_events(
    threads: &[ScheduledThread],
    current: &str,
    keep_days: i64,
    device_id: &str,
) -> (Housekeeping, Vec<crate::events::NewEvent>) {
    let plan = plan_housekeeping(threads, current, keep_days, Utc::now());
    let events = super::chat_tidy::events(&plan, device_id);
    (plan, events)
}

/// Every thread `prompt` raised, with its state, for [`plan_housekeeping`].
pub async fn threads(db: &Database, prompt: &str) -> Result<Vec<ScheduledThread>, EventError> {
    let mut resp = db
        .query(
            "SELECT thread_id, <string> created_at AS created_at FROM assistant_messages
             WHERE prompt_name = $prompt;
             SELECT thread_id, archived, deleted FROM assistant_thread_state;
             SELECT VALUE thread_id FROM assistant_proposals
             WHERE decision IS NONE AND thread_id != NONE;",
        )
        .bind(("prompt", prompt.to_string()))
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
            Some(ScheduledThread {
                thread_id,
                created_at,
                archived,
                deleted,
                has_pending,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigMap, ConfigValue};

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn prompt(enabled: bool, hour: u32) -> ScheduledPrompt {
        ScheduledPrompt {
            name: CHECK_IN,
            title: "Daily check-in",
            text: "Review what is due.".to_string(),
            enabled,
            cadence: Cadence::Daily,
            hour,
            timezone: Tz::UTC,
            tier: Tier::Ambient,
            keep_days: KEEP_CHECK_INS_DAYS,
        }
    }

    #[test]
    fn the_check_in_is_off_by_default() {
        let config = ResolvedConfig::new(ConfigMap::new(), ConfigMap::new());
        let p = check_in(&config);
        assert!(
            !p.enabled,
            "initiative must be opt-in: a default-on check-in would start \
             proposing on an install that never asked for one"
        );
        assert!(
            !p.text.trim().is_empty(),
            "but a prompt is ready when it is"
        );
        assert_eq!(p.hour, 7);
        assert_eq!(p.tier, Tier::Ambient);
    }

    #[test]
    fn prompt_names_are_unique() {
        let config = ResolvedConfig::new(ConfigMap::new(), ConfigMap::new());
        let all = prompts(&config);
        let names: std::collections::BTreeSet<_> = all.iter().map(|p| p.name).collect();
        assert_eq!(
            names.len(),
            all.len(),
            "two prompts would share one history"
        );
    }

    #[test]
    fn a_disabled_prompt_is_never_due() {
        assert!(!is_due(&prompt(false, 7), None, at("2026-09-11T09:00:00Z")));
    }

    /// Asking a model nothing costs a request and returns something arbitrary,
    /// which then looks like the assistant volunteering.
    #[test]
    fn an_empty_prompt_is_treated_as_off() {
        let mut p = prompt(true, 7);
        p.text = "   ".into();
        assert!(!is_due(&p, None, at("2026-09-11T09:00:00Z")));
    }

    #[test]
    fn it_waits_until_the_hour_then_runs_once() {
        let p = prompt(true, 7);
        assert!(!is_due(&p, None, at("2026-09-11T06:59:00Z")), "too early");
        assert!(is_due(&p, None, at("2026-09-11T07:00:00Z")), "on the hour");

        let ran = at("2026-09-11T07:00:00Z");
        assert!(
            !is_due(&p, Some(ran), at("2026-09-11T23:00:00Z")),
            "already ran today"
        );
    }

    /// The agent is restarted casually (a deploy, an OOM, a config change), and
    /// each restart re-reads state.
    #[test]
    fn restarting_the_same_day_does_not_run_again() {
        let p = prompt(true, 7);
        let ran = at("2026-09-11T07:00:00Z");
        for hour in ["08", "12", "18", "23"] {
            let now = at(&format!("2026-09-11T{hour}:30:00Z"));
            assert!(!is_due(&p, Some(ran), now), "re-ran at {hour}");
        }
    }

    #[test]
    fn the_next_day_is_due_again() {
        let p = prompt(true, 7);
        let ran = at("2026-09-11T07:00:00Z");
        assert!(is_due(&p, Some(ran), at("2026-09-12T07:00:00Z")));
    }

    /// A silently missed run is indistinguishable from the feature being broken.
    #[test]
    fn a_missed_hour_still_runs_later_the_same_day() {
        let p = prompt(true, 7);
        let ran = at("2026-09-10T07:00:00Z");
        assert!(is_due(&p, Some(ran), at("2026-09-11T22:00:00Z")));
    }

    /// Under a 24h-interval test this case is not due, and the run drifts an
    /// hour later every day until it leaves the user's waking hours.
    #[test]
    fn the_schedule_does_not_drift_when_a_run_is_late() {
        let p = prompt(true, 7);
        let ran_late = at("2026-09-11T22:00:00Z");
        assert!(
            is_due(&p, Some(ran_late), at("2026-09-12T07:00:00Z")),
            "a late run must not push the next one to 22:00"
        );
    }

    /// Where an ordinal-only comparison breaks: day 1 of one year matches day 1
    /// of the next.
    #[test]
    fn the_same_ordinal_in_a_different_year_is_due() {
        let p = prompt(true, 0);
        let ran = at("2025-01-01T09:00:00Z");
        assert!(is_due(&p, Some(ran), at("2026-01-01T09:00:00Z")));
    }

    /// Hour 6 once meant 02:00 in Toronto, because it was read off the agent's UTC clock.
    #[test]
    fn the_hour_and_the_day_are_the_users_not_the_agents() {
        let mut p = prompt(true, 6);
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
    fn a_weekly_period_starts_on_its_day() {
        let monday = Cadence::Weekly(Weekday::Mon);
        // 2026-11-16 is a Monday.
        assert_eq!(monday.period_start(day("2026-11-16")), day("2026-11-16"));
        assert_eq!(monday.period_start(day("2026-11-22")), day("2026-11-16"));
        assert_eq!(monday.period_start(day("2026-11-23")), day("2026-11-23"));
        let sunday = Cadence::Weekly(Weekday::Sun);
        assert_eq!(sunday.period_start(day("2026-11-21")), day("2026-11-15"));
        assert_eq!(sunday.period_start(day("2026-11-22")), day("2026-11-22"));
    }

    #[test]
    fn a_monthly_period_crosses_the_year_and_clamps_its_day() {
        let first = Cadence::Monthly(1);
        assert_eq!(first.period_start(day("2026-12-31")), day("2026-12-01"));
        assert_eq!(first.period_start(day("2027-01-01")), day("2027-01-01"));
        let fifteenth = Cadence::Monthly(15);
        assert_eq!(fifteenth.period_start(day("2027-01-10")), day("2026-12-15"));
        let clamped = Cadence::Monthly(31);
        assert_eq!(clamped.period_start(day("2027-02-28")), day("2027-02-28"));
    }

    /// A weekly prompt missed on its day still runs later that week, once, and
    /// never before the hour.
    #[test]
    fn a_weekly_prompt_runs_once_a_week_and_catches_up() {
        let mut p = prompt(true, 7);
        p.cadence = Cadence::Weekly(Weekday::Mon);
        let ran = at("2026-11-09T07:00:00Z");
        assert!(
            !is_due(&p, Some(ran), at("2026-11-15T23:00:00Z")),
            "same week"
        );
        assert!(
            !is_due(&p, Some(ran), at("2026-11-17T06:00:00Z")),
            "before the hour"
        );
        assert!(
            is_due(&p, Some(ran), at("2026-11-17T07:00:00Z")),
            "missed Monday"
        );
        let caught_up = at("2026-11-17T07:00:00Z");
        assert!(!is_due(&p, Some(caught_up), at("2026-11-22T12:00:00Z")));
        assert!(is_due(&p, Some(caught_up), at("2026-11-23T07:00:00Z")));
    }

    /// Never run, a weekly or monthly prompt waits for its own day rather than firing mid-period.
    #[test]
    fn a_first_run_waits_for_its_day() {
        let mut p = prompt(true, 7);
        p.cadence = Cadence::Weekly(Weekday::Sun);
        // 2026-11-26 is a Thursday, 2026-11-29 a Sunday.
        assert!(!is_due(&p, None, at("2026-11-26T09:00:00Z")));
        assert!(is_due(&p, None, at("2026-11-29T09:00:00Z")));
        p.cadence = Cadence::Monthly(1);
        assert!(!is_due(&p, None, at("2026-11-26T09:00:00Z")));
        assert!(is_due(&p, None, at("2026-12-01T09:00:00Z")));
        p.cadence = Cadence::Daily;
        assert!(
            is_due(&p, None, at("2026-11-26T09:00:00Z")),
            "a daily one is due any day"
        );
    }

    /// A clock that ran ahead must not make every later tick due.
    #[test]
    fn a_last_run_in_the_future_is_not_a_reason_to_run() {
        let p = prompt(true, 7);
        assert!(!is_due(
            &p,
            Some(at("2026-11-25T07:00:00Z")),
            at("2026-11-21T09:00:00Z")
        ));
    }

    #[test]
    fn the_zone_is_read_from_config() {
        let mut global = ConfigMap::new();
        global.insert(
            ConfigKey::AssistantCheckInTimezone,
            ConfigValue::Text("America/Toronto".into()),
        );
        let config = ResolvedConfig::new(global, ConfigMap::new());
        assert_eq!(check_in(&config).timezone, chrono_tz::America::Toronto);
        assert!(
            ConfigKey::AssistantCheckInTimezone
                .validate(&ConfigValue::Text("Mars/Base".into()))
                .is_err()
        );
    }

    #[test]
    fn an_out_of_range_hour_is_clamped_rather_than_disabling_it() {
        let mut global = ConfigMap::new();
        global.insert(ConfigKey::AssistantCheckInHour, ConfigValue::Int(99));
        let config = ResolvedConfig::new(global, ConfigMap::new());
        assert_eq!(check_in(&config).hour, 23);

        let mut global = ConfigMap::new();
        global.insert(ConfigKey::AssistantCheckInHour, ConfigValue::Int(-4));
        let config = ResolvedConfig::new(global, ConfigMap::new());
        assert_eq!(check_in(&config).hour, 0);
    }

    fn thread(id: &str, days_ago: i64, archived: bool, has_pending: bool) -> ScheduledThread {
        ScheduledThread {
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
        let plan = plan_housekeeping(
            &threads,
            "today",
            KEEP_CHECK_INS_DAYS,
            at("2026-10-20T10:00:00Z"),
        );
        assert_eq!(plan.archive, vec!["yesterday".to_string()]);
        assert_eq!(plan.delete, vec!["old".to_string()]);
    }

    /// The collision this module exists to prevent: a second prompt's question
    /// must neither count as the check-in's run nor be housekept by it.
    #[tokio::test]
    async fn each_prompt_reads_only_its_own_runs_and_threads() {
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
        // "legacy" is a scheduled question from before names: it reads as the check-in.
        for (thread, scheduled, name) in [
            ("legacy", true, None),
            ("c2", true, Some(CHECK_IN)),
            ("morning", true, Some("morning_plan")),
            ("user", false, None),
        ] {
            let p = AssistantQuestionAskedPayload {
                thread_id: thread.into(),
                message_id: format!("m-{thread}"),
                text: "Review what is due.".into(),
                title: None,
                scheduled,
                prompt_name: name.map(str::to_string),
            };
            apply(NewEvent::assistant_question_asked("agent", &p).unwrap()).await;
        }
        let archived = AssistantThreadArchivedPayload {
            thread_id: "legacy".into(),
            archived: true,
        };
        apply(NewEvent {
            id: None,
            event_type: EventType::AssistantThreadArchived.to_string(),
            aggregate_id: "legacy".into(),
            timestamp: Utc::now(),
            device_id: "phone".into(),
            payload: serde_json::to_value(&archived).unwrap(),
        })
        .await;

        let mut check_ins = threads(&db, CHECK_IN).await.unwrap();
        check_ins.sort_by(|a, b| a.thread_id.cmp(&b.thread_id));
        let ids: Vec<&str> = check_ins.iter().map(|t| t.thread_id.as_str()).collect();
        assert_eq!(ids, ["c2", "legacy"], "a typed question is never housekept");
        assert!(!check_ins[0].archived && check_ins[1].archived);

        let morning = threads(&db, "morning_plan").await.unwrap();
        assert_eq!(morning.len(), 1);
        assert_eq!(morning[0].thread_id, "morning");

        assert!(last_run(&db, CHECK_IN).await.unwrap().is_some());
        assert!(last_run(&db, "morning_plan").await.unwrap().is_some());
        assert!(last_run(&db, "evening").await.unwrap().is_none());
    }
}
