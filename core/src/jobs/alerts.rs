//! Critical alerts from run records: a job's failure policy, made into notifications.
//!
//! Each alert is one episode (a subject needing the user, or failing past its
//! window), opened once and closed by the next success, so a failing source alerts
//! once rather than on every retry. Episodes are stored, which keeps a restart from
//! re-sending one and gives slice 3 the record to measure the weekly budget from.

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use std::sync::Arc;
use surrealdb::types::SurrealValue;

use super::runs::{RunObserver, RunOutcome, RunRecord, RunStore};
use crate::config::{ConfigKey, ResolvedConfig};
use crate::db::Database;
use crate::notify::{Notification, Notifier, Priority};

/// How often held and failed sends are retried.
pub const FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    NeedsUser,
    Failing,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::NeedsUser => "needs_user",
            Kind::Failing => "failing",
        }
    }
}

/// When the quiet window containing `now` ends, or `None` when `now` is outside it.
/// `start == end` means no quiet hours. A window may wrap midnight (22 to 8).
pub fn quiet_until(now: DateTime<Utc>, tz: Tz, start: u32, end: u32) -> Option<DateTime<Utc>> {
    if start == end {
        return None;
    }
    let local = now.with_timezone(&tz);
    let hour = chrono::Timelike::hour(&local);
    let inside = if start < end {
        hour >= start && hour < end
    } else {
        hour >= start || hour < end
    };
    if !inside {
        return None;
    }
    let mut day = local.date_naive();
    if hour >= end {
        day = day.succ_opt()?;
    }
    let end_local = day.and_time(NaiveTime::from_hms_opt(end, 0, 0)?);
    // A DST gap at the end hour falls back to an hour later rather than never.
    tz.from_local_datetime(&end_local)
        .earliest()
        .or_else(|| {
            tz.from_local_datetime(&(end_local + Duration::hours(1)))
                .earliest()
        })
        .map(|t| t.with_timezone(&Utc))
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[derive(Debug, SurrealValue)]
struct Pending {
    alert_id: String,
    title: String,
    message: String,
}

pub struct AlertEngine {
    db: Database,
    runs: Arc<dyn RunStore>,
    notifier: Arc<dyn Notifier>,
}

impl AlertEngine {
    pub fn new(db: Database, runs: Arc<dyn RunStore>, notifier: Arc<dyn Notifier>) -> Self {
        Self { db, runs, notifier }
    }

    pub async fn init_schema(&self) -> Result<(), String> {
        self.db
            .query(
                "DEFINE TABLE IF NOT EXISTS job_alerts SCHEMAFULL;
                 DEFINE FIELD IF NOT EXISTS alert_id ON job_alerts TYPE string;
                 DEFINE FIELD IF NOT EXISTS job ON job_alerts TYPE string;
                 DEFINE FIELD IF NOT EXISTS subject ON job_alerts TYPE string;
                 DEFINE FIELD IF NOT EXISTS kind ON job_alerts TYPE string;
                 DEFINE FIELD IF NOT EXISTS title ON job_alerts TYPE string;
                 DEFINE FIELD IF NOT EXISTS message ON job_alerts TYPE string;
                 DEFINE FIELD IF NOT EXISTS opened_at ON job_alerts TYPE string;
                 DEFINE FIELD IF NOT EXISTS due_at ON job_alerts TYPE string;
                 DEFINE FIELD IF NOT EXISTS sent_at ON job_alerts TYPE option<string>;
                 DEFINE FIELD IF NOT EXISTS closed_at ON job_alerts TYPE option<string>;
                 DEFINE INDEX IF NOT EXISTS job_alerts_open ON job_alerts FIELDS job, subject, closed_at;",
            )
            .await
            .map_err(|e| format!("init job_alerts: {e}"))?
            .check()
            .map_err(|e| format!("init job_alerts: {e}"))?;
        Ok(())
    }

    async fn is_open(&self, run: &RunRecord, kind: Kind) -> Result<bool, String> {
        let mut resp = self
            .db
            .query(
                "SELECT alert_id FROM job_alerts WHERE job = $job AND subject = $subject \
                 AND kind = $kind AND closed_at IS NONE LIMIT 1",
            )
            .bind(("job", run.job.clone()))
            .bind(("subject", run.subject.clone()))
            .bind(("kind", kind.as_str()))
            .await
            .map_err(|e| format!("read job_alerts: {e}"))?;
        let ids: Vec<String> = resp.take("alert_id").map_err(|e| e.to_string())?;
        Ok(!ids.is_empty())
    }

    async fn open(
        &self,
        run: &RunRecord,
        kind: Kind,
        title: String,
        message: String,
    ) -> Result<(), String> {
        if self.is_open(run, kind).await? {
            return Ok(());
        }
        let now = Utc::now();
        let due = self.hold_until(now).await.unwrap_or(now);
        self.db
            .query(
                "CREATE type::record('job_alerts', $id) CONTENT {
                    alert_id: $id, job: $job, subject: $subject, kind: $kind,
                    title: $title, message: $message,
                    opened_at: $now, due_at: $due, sent_at: NONE, closed_at: NONE
                 }",
            )
            .bind(("id", ulid::Ulid::new().to_string()))
            .bind(("job", run.job.clone()))
            .bind(("subject", run.subject.clone()))
            .bind(("kind", kind.as_str()))
            .bind(("title", title))
            .bind(("message", message))
            .bind(("now", stamp(now)))
            .bind(("due", stamp(due)))
            .await
            .map_err(|e| format!("write job_alerts: {e}"))?
            .check()
            .map_err(|e| format!("write job_alerts: {e}"))?;
        self.flush().await;
        Ok(())
    }

    async fn close_all(&self, run: &RunRecord) -> Result<(), String> {
        self.db
            .query(
                "UPDATE job_alerts SET closed_at = $now WHERE job = $job AND subject = $subject \
                 AND closed_at IS NONE",
            )
            .bind(("now", stamp(Utc::now())))
            .bind(("job", run.job.clone()))
            .bind(("subject", run.subject.clone()))
            .await
            .map_err(|e| format!("close job_alerts: {e}"))?
            .check()
            .map_err(|e| format!("close job_alerts: {e}"))?;
        Ok(())
    }

    /// The end of the current quiet window, read from config each time.
    async fn hold_until(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let global = crate::events::load_persisted(&self.db).await.ok()?;
        let config = ResolvedConfig::new(global, Default::default());
        let tz: Tz = config
            .text_of(ConfigKey::AssistantCheckInTimezone)
            .parse()
            .ok()?;
        let start = config.int_of(ConfigKey::NotifyQuietStart).clamp(0, 23) as u32;
        let end = config.int_of(ConfigKey::NotifyQuietEnd).clamp(0, 23) as u32;
        quiet_until(now, tz, start, end)
    }

    /// Send every open, unsent alert that is due. A failed send stays unsent and
    /// is retried on the next flush.
    pub async fn flush(&self) {
        let now = stamp(Utc::now());
        let pending: Vec<Pending> = match self
            .db
            .query(
                "SELECT alert_id, title, message FROM job_alerts \
                 WHERE sent_at IS NONE AND closed_at IS NONE AND due_at <= $now",
            )
            .bind(("now", now))
            .await
            .and_then(|mut r| r.take(0))
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "could not read pending alerts");
                return;
            }
        };
        for p in pending {
            let n = Notification {
                title: p.title,
                message: p.message,
                priority: Priority::High,
                tags: vec!["warning".into()],
            };
            match self.notifier.send(&n).await {
                Ok(()) => {
                    let marked = self
                        .db
                        .query("UPDATE type::record('job_alerts', $id) SET sent_at = $now")
                        .bind(("id", p.alert_id.clone()))
                        .bind(("now", stamp(Utc::now())))
                        .await;
                    if let Err(e) = marked {
                        tracing::warn!(alert = p.alert_id, error = %e, "sent an alert but could not mark it");
                    }
                }
                Err(e) => {
                    tracing::warn!(alert = p.alert_id, error = %e, "alert send failed; retrying later")
                }
            }
        }
    }

    /// When `subject`'s current failing streak began: its last success, else its
    /// oldest recorded run.
    async fn failing_since(&self, run: &RunRecord) -> Result<DateTime<Utc>, String> {
        if let Some(ok) = self.runs.last_success(&run.job, &run.subject).await? {
            return Ok(ok.ended_at);
        }
        let history = self.runs.recent(Some(&run.job), 500).await?;
        Ok(history
            .iter()
            .filter(|r| r.subject == run.subject)
            .map(|r| r.started_at)
            .min()
            .unwrap_or(run.started_at))
    }

    async fn handle(&self, run: &RunRecord) -> Result<(), String> {
        let Some(spec) = super::spec(&run.job) else {
            return Ok(());
        };
        match &run.outcome {
            RunOutcome::Success => self.close_all(run).await,
            RunOutcome::NeedsUser { reason } if spec.failure.alert_needs_user => {
                let title = format!("{} needs you", spec.title);
                let message = format!(
                    "{}: {reason}. Reconnect it in Settings, Auto-Import.",
                    run.subject
                );
                self.open(run, Kind::NeedsUser, title, message).await
            }
            RunOutcome::Failure { error } => {
                let Some(hours) = spec.failure.alert_failing_hours else {
                    return Ok(());
                };
                let since = self.failing_since(run).await?;
                if run.ended_at - since < Duration::hours(hours as i64) {
                    return Ok(());
                }
                let title = format!("{} failing for {hours} h", spec.title);
                let message = format!(
                    "{} has not completed a run since {}. Last error: {error}",
                    run.subject,
                    since.format("%Y-%m-%d %H:%M UTC")
                );
                self.open(run, Kind::Failing, title, message).await
            }
            RunOutcome::NeedsUser { .. } => Ok(()),
        }
    }
}

#[async_trait]
impl RunObserver for AlertEngine {
    async fn observe(&self, run: &RunRecord) {
        if let Err(e) = self.handle(run).await {
            tracing::warn!(job = run.job, subject = run.subject, error = %e, "alert check failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::runs::SurrealRunStore;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Phone(Mutex<Vec<Notification>>);

    #[async_trait]
    impl Notifier for Phone {
        async fn send(&self, n: &Notification) -> Result<(), String> {
            self.0.lock().unwrap().push(n.clone());
            Ok(())
        }
    }

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn quiet_hours_wrap_midnight_in_the_users_zone() {
        let tz: Tz = "America/Winnipeg".parse().unwrap();
        // 23:30 local on 11-13 (CST, UTC-6) holds until 08:00 local on 11-14.
        let held = quiet_until(utc("2026-11-14T05:30:00Z"), tz, 22, 8).unwrap();
        assert_eq!(held, utc("2026-11-14T14:00:00Z"));
        // 03:00 local holds until 08:00 the same day.
        let held = quiet_until(utc("2026-11-14T09:00:00Z"), tz, 22, 8).unwrap();
        assert_eq!(held, utc("2026-11-14T14:00:00Z"));
        // Noon local is outside the window.
        assert_eq!(quiet_until(utc("2026-11-14T18:00:00Z"), tz, 22, 8), None);
    }

    #[test]
    fn a_window_that_does_not_wrap_and_an_empty_one() {
        let tz: Tz = "UTC".parse().unwrap();
        assert_eq!(
            quiet_until(utc("2026-11-14T13:00:00Z"), tz, 12, 14),
            Some(utc("2026-11-14T14:00:00Z"))
        );
        assert_eq!(quiet_until(utc("2026-11-14T15:00:00Z"), tz, 12, 14), None);
        assert_eq!(quiet_until(utc("2026-11-14T03:00:00Z"), tz, 8, 8), None);
    }

    /// Quiet hours set to an empty window, so every test sends at once whatever
    /// the clock says.
    async fn engine() -> (AlertEngine, Arc<SurrealRunStore>, Arc<Phone>) {
        use crate::config::ConfigValue;
        use crate::events::{
            ConfigProjection, EventStore, NewEvent, ProjectionRunner, SurrealEventStore,
        };
        let db = crate::db::test_db().await;
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(ConfigProjection)]);
        runner.init_all().await.unwrap();
        for (key, v) in [
            (ConfigKey::NotifyQuietStart, 0),
            (ConfigKey::NotifyQuietEnd, 0),
        ] {
            let set = NewEvent::config_set("d1", key, Some(ConfigValue::Int(v))).unwrap();
            let stored = SurrealEventStore::new(db.clone())
                .append(set)
                .await
                .unwrap();
            runner.apply_events(&[stored]).await.unwrap();
        }
        let runs = Arc::new(SurrealRunStore::new(db.clone()));
        runs.init_schema().await.unwrap();
        let phone = Arc::new(Phone::default());
        let engine = AlertEngine::new(db, runs.clone(), phone.clone());
        engine.init_schema().await.unwrap();
        (engine, runs, phone)
    }

    fn run(ago_h: i64, outcome: RunOutcome) -> RunRecord {
        let end = Utc::now() - Duration::hours(ago_h);
        RunRecord::new(crate::jobs::MAIL_IMPORT, "gmail", end, end, outcome)
    }

    async fn record(engine: &AlertEngine, runs: &SurrealRunStore, r: RunRecord) {
        runs.record(&r).await.unwrap();
        engine.observe(&r).await;
    }

    #[tokio::test]
    async fn needing_the_user_alerts_once_until_a_success() {
        let (engine, runs, phone) = engine().await;
        let code = || {
            run(
                0,
                RunOutcome::NeedsUser {
                    reason: "app password revoked".into(),
                },
            )
        };
        record(&engine, &runs, code()).await;
        record(&engine, &runs, code()).await;
        assert_eq!(phone.0.lock().unwrap().len(), 1, "one alert per episode");
        assert!(
            phone.0.lock().unwrap()[0]
                .message
                .contains("app password revoked")
        );

        record(&engine, &runs, run(0, RunOutcome::Success)).await;
        record(&engine, &runs, code()).await;
        assert_eq!(
            phone.0.lock().unwrap().len(),
            2,
            "a success ends the episode"
        );
    }

    #[tokio::test]
    async fn failing_alerts_only_past_the_window() {
        let (engine, runs, phone) = engine().await;
        let down = |h| {
            run(
                h,
                RunOutcome::Failure {
                    error: "connection reset".into(),
                },
            )
        };
        record(&engine, &runs, run(30, RunOutcome::Success)).await;
        record(&engine, &runs, down(29)).await;
        record(&engine, &runs, down(10)).await;
        assert!(
            phone.0.lock().unwrap().is_empty(),
            "under 24 h since the last success"
        );
        record(&engine, &runs, down(0)).await;
        record(&engine, &runs, down(0)).await;
        let sent = phone.0.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].title, "Mail import failing for 24 h");
    }

    #[tokio::test]
    async fn a_job_off_the_contract_never_alerts() {
        let (engine, _runs, phone) = engine().await;
        let mut r = run(
            0,
            RunOutcome::NeedsUser {
                reason: "otp".into(),
            },
        );
        r.job = crate::jobs::MAIL_BACKFILL.into();
        engine.observe(&r).await;
        assert!(phone.0.lock().unwrap().is_empty());
    }
}
