//! The daily review summary: one silent notification of what waits on review.
//!
//! Counts only, built by code, at `notify.summary_hour` in the user's zone, and
//! skipped when nothing has arrived since the last one sent (`project.md` §
//! Notifications). Unmatched transactions are counted apart from the review queue.

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::runs::{RunOutcome, RunRecord, RunStore};
use crate::assistant::inbox;
use crate::config::{ConfigKey, Feature, ResolvedConfig};
use crate::db::Database;
use crate::notify::{Notification, Notifier, Priority};

pub const REVIEW_SUMMARY: &str = "review_summary";

/// How often the schedule is checked, so a changed hour applies the same day.
pub const TICK: std::time::Duration = std::time::Duration::from_secs(300);

/// The retention scan's own ceiling on a device, reused so both count alike.
const PURGE_SCAN_LIMIT: u32 = 500;

/// Everything waiting, by id, so "new since the last summary" is a set difference
/// rather than a count that a cleared item and a new one could cancel out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub batches: BTreeSet<String>,
    pub suggestions: BTreeSet<String>,
    pub unmatched: BTreeSet<String>,
    pub purge: BTreeSet<String>,
    /// Open alerts of a kind set to Review, by alert id, each with its line.
    #[serde(default)]
    pub alerts: BTreeMap<String, String>,
    /// The listed alerts a tap should open Finances for (large charges).
    #[serde(default)]
    pub finance_alerts: BTreeSet<String>,
    /// Set when the last 7 days sent more critical alerts than the budget.
    #[serde(default)]
    pub over_budget: Option<String>,
}

impl Waiting {
    /// Only what a rendered screen reviews: a disabled feature contributes nothing,
    /// as with the nav badges (`approvals::summary`).
    pub async fn gather(
        db: &Database,
        config: &ResolvedConfig,
        now: DateTime<Utc>,
    ) -> Result<Self, String> {
        let mut w = Waiting::default();
        let err = |e: &dyn std::fmt::Display| e.to_string();
        if config.enabled(Feature::Llm) {
            for p in inbox::pending(db).await.map_err(|e| err(&e))? {
                w.suggestions.insert(p.proposal_id);
            }
        }
        for &feature in inbox::DOMAIN_REVIEWED {
            if config.enabled(feature) {
                for p in inbox::pending_for_feature(db, feature)
                    .await
                    .map_err(|e| err(&e))?
                {
                    w.suggestions.insert(p.proposal_id);
                }
            }
        }
        if config.enabled(Feature::Finances) {
            w.batches = ids(
                db,
                // The row, not `batch_id`: a re-pull rewrites `batch_id` on a pending row.
                "SELECT VALUE record::id(id) FROM pending_auto_import_batches WHERE status = 'pending'",
            )
            .await?;
            w.unmatched = ids(
                db,
                "SELECT VALUE record::id(id) FROM transactions WHERE removed = false \
                 AND superseded_by IS NONE AND array::any(postings, |$p| $p.account = 'Unmatched')",
            )
            .await?;
        }
        if config.enabled(Feature::Documents) {
            let due = crate::retention::candidates(db, now, PURGE_SCAN_LIMIT)
                .await
                .map_err(|e| err(&e))?;
            w.purge = due.into_iter().map(|c| c.document_id).collect();
        }
        for a in super::alerts::open_review_alerts(db).await? {
            if a.tab() == "finances" {
                w.finance_alerts.insert(a.alert_id.clone());
            }
            w.alerts.insert(a.alert_id.clone(), a.line());
        }
        w.over_budget = super::alerts::over_budget(db, now).await?;
        Ok(w)
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
            && self.suggestions.is_empty()
            && self.unmatched.is_empty()
            && self.purge.is_empty()
            && self.alerts.is_empty()
            && self.over_budget.is_none()
    }

    /// Whether anything is here that the last summary did not list. A changed
    /// over-budget line is new: it carries this week's count.
    pub fn has_new_since(&self, last: &Waiting) -> bool {
        !self.batches.is_subset(&last.batches)
            || !self.suggestions.is_subset(&last.suggestions)
            || !self.unmatched.is_subset(&last.unmatched)
            || !self.purge.is_subset(&last.purge)
            || self.alerts.keys().any(|id| !last.alerts.contains_key(id))
            || (self.over_budget.is_some() && self.over_budget != last.over_budget)
    }

    pub fn counts(&self) -> BTreeMap<String, u64> {
        [
            ("batches", self.batches.len()),
            ("suggestions", self.suggestions.len()),
            ("unmatched", self.unmatched.len()),
            ("purge", self.purge.len()),
            ("alerts", self.alerts.len()),
            ("over_budget", usize::from(self.over_budget.is_some())),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v as u64))
        .collect()
    }

    /// The tab a tap opens: the one with the most waiting, Finances on a tie, and
    /// Settings when only source alerts or the budget line are listed.
    pub fn tab(&self) -> &'static str {
        let finances = self.batches.len() + self.unmatched.len() + self.finance_alerts.len();
        let candidates = [
            (finances, "finances"),
            (self.suggestions.len(), "assistant"),
            (self.purge.len(), "archive"),
        ];
        candidates
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .fold(None, |best: Option<(usize, &str)>, c| match best {
                Some(b) if b.0 >= c.0 => Some(b),
                _ => Some(c),
            })
            .map_or("settings", |(_, tab)| tab)
    }

    /// One line per screen, naming only what is non-zero.
    pub fn message(&self) -> String {
        let mut lines = Vec::new();
        let mut finances = Vec::new();
        if !self.batches.is_empty() {
            finances.push(format!("{} to review", self.batches.len()));
        }
        if !self.unmatched.is_empty() {
            finances.push(format!("{} unmatched", self.unmatched.len()));
        }
        if !finances.is_empty() {
            lines.push(format!("Finances: {}", finances.join(", ")));
        }
        if !self.suggestions.is_empty() {
            let n = self.suggestions.len();
            lines.push(format!("Suggestions: {n} waiting"));
        }
        if !self.purge.is_empty() {
            lines.push(format!("Archive: {} due for purge", self.purge.len()));
        }
        lines.extend(self.alerts.values().cloned());
        lines.extend(self.over_budget.clone());
        lines.join("\n")
    }
}

async fn ids(db: &Database, sql: &str) -> Result<BTreeSet<String>, String> {
    let mut resp = db.query(sql).await.map_err(|e| e.to_string())?;
    let rows: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().collect())
}

/// The day the summary is due and whether `now` has reached its hour.
pub fn due(now: DateTime<Utc>, tz: Tz, hour: u32) -> Option<NaiveDate> {
    let local = now.with_timezone(&tz);
    let at = local
        .date_naive()
        .and_time(NaiveTime::from_hms_opt(hour, 0, 0)?);
    let at = tz.from_local_datetime(&at).earliest()?;
    (local >= at).then(|| local.date_naive())
}

pub struct SummaryJob {
    db: Database,
    runs: Arc<dyn RunStore>,
    notifier: Arc<dyn Notifier>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    last_day: Option<NaiveDate>,
    last_sent: Waiting,
}

impl SummaryJob {
    pub fn new(db: Database, runs: Arc<dyn RunStore>, notifier: Arc<dyn Notifier>) -> Self {
        Self { db, runs, notifier }
    }

    pub async fn init_schema(&self) -> Result<(), String> {
        self.db
            .query(
                "DEFINE TABLE IF NOT EXISTS job_summary_state SCHEMAFULL;
                 DEFINE FIELD IF NOT EXISTS state ON job_summary_state TYPE string;",
            )
            .await
            .map_err(|e| format!("init job_summary_state: {e}"))?
            .check()
            .map_err(|e| format!("init job_summary_state: {e}"))?;
        // The summary reads the alerts table, and may start before the engine.
        super::alerts::init_schema(&self.db).await
    }

    async fn load(&self) -> State {
        let read = async {
            let mut resp = self
                .db
                .query("SELECT VALUE state FROM type::record('job_summary_state', 'last')")
                .await
                .map_err(|e| e.to_string())?;
            let rows: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
            Ok::<_, String>(rows.into_iter().next())
        };
        match read.await {
            Ok(Some(s)) => serde_json::from_str(&s).unwrap_or_default(),
            Ok(None) => State::default(),
            Err(e) => {
                tracing::warn!(error = %e, "could not read the summary state; treating as first run");
                State::default()
            }
        }
    }

    async fn save(&self, state: &State) -> Result<(), String> {
        let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
        self.db
            .query("UPSERT type::record('job_summary_state', 'last') CONTENT { state: $state }")
            .bind(("state", json))
            .await
            .map_err(|e| format!("save summary state: {e}"))?
            .check()
            .map_err(|e| format!("save summary state: {e}"))?;
        Ok(())
    }

    /// Run if today's summary is due and has not run. A failed run leaves the day
    /// open, so the next tick tries again.
    pub async fn tick(&self, now: DateTime<Utc>) {
        let config = match crate::events::load_persisted(&self.db).await {
            Ok(global) => ResolvedConfig::new(global, Default::default()),
            Err(e) => {
                tracing::warn!(error = %e, "summary: could not read config");
                return;
            }
        };
        let tz: Tz = config
            .text_of(ConfigKey::AssistantCheckInTimezone)
            .parse()
            .unwrap_or(chrono_tz::UTC);
        let hour = config.int_of(ConfigKey::NotifySummaryHour).clamp(0, 23) as u32;
        let Some(day) = due(now, tz, hour) else {
            return;
        };
        let mut state = self.load().await;
        if state.last_day == Some(day) {
            return;
        }

        let started = Utc::now();
        let mut sent = 0;
        let gathered = Waiting::gather(&self.db, &config, now).await;
        // This run's own counts: `state.last_sent` still holds yesterday's after a failure.
        let mut counts = gathered.as_ref().map(Waiting::counts).unwrap_or_default();
        let outcome = match gathered {
            Err(e) => RunOutcome::Failure { error: e },
            Ok(waiting) if waiting.is_empty() || !waiting.has_new_since(&state.last_sent) => {
                state.last_day = Some(day);
                state.last_sent = waiting;
                RunOutcome::Success
            }
            Ok(waiting) => {
                let n = Notification {
                    title: "Waiting on review".into(),
                    message: waiting.message(),
                    priority: Priority::Low,
                    tags: Vec::new(),
                    click: Some(crate::notify::app_link(waiting.tab())),
                };
                match self.notifier.send(&n).await {
                    Ok(()) => {
                        sent = 1;
                        let listed: Vec<String> = waiting.alerts.keys().cloned().collect();
                        if let Err(e) =
                            super::alerts::close_listed_one_shots(&self.db, listed).await
                        {
                            tracing::warn!(error = %e, "summary sent, but its one-shot alerts stay open");
                        }
                        state.last_day = Some(day);
                        state.last_sent = waiting;
                        RunOutcome::Success
                    }
                    Err(e) => RunOutcome::Failure { error: e },
                }
            }
        };
        let mut run = RunRecord::new(
            REVIEW_SUMMARY,
            "daily",
            started,
            Utc::now(),
            outcome.clone(),
        );
        counts.insert("sent".into(), sent);
        run.counts = counts;
        if outcome == RunOutcome::Success
            && let Err(e) = self.save(&state).await
        {
            tracing::warn!(error = %e, "summary ran but its state was not saved; it may repeat");
        }
        if let Err(e) = self.runs.record(&run).await {
            tracing::warn!(error = %e, "could not record the summary run");
        }
    }

    pub fn spawn(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(TICK);
            loop {
                tick.tick().await;
                self.tick(Utc::now()).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::runs::SurrealRunStore;
    use std::sync::Mutex;

    #[derive(Default)]
    /// The second field turns ntfy off: every send fails.
    struct Phone(Mutex<Vec<Notification>>, std::sync::atomic::AtomicBool);

    #[async_trait::async_trait]
    impl Notifier for Phone {
        async fn send(&self, n: &Notification) -> Result<(), String> {
            if self.1.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("ntfy unreachable".into());
            }
            self.0.lock().unwrap().push(n.clone());
            Ok(())
        }
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn a_cleared_item_and_a_new_one_still_count_as_new() {
        let last = Waiting {
            batches: set(&["a", "b"]),
            ..Default::default()
        };
        let now = Waiting {
            batches: set(&["b", "c"]),
            ..Default::default()
        };
        assert!(now.has_new_since(&last), "same count, one new id");
        let fewer = Waiting {
            batches: set(&["b"]),
            ..Default::default()
        };
        assert!(!fewer.has_new_since(&last), "only cleared, nothing new");
    }

    #[test]
    fn unmatched_is_counted_apart_from_review() {
        let w = Waiting {
            batches: set(&["a", "b", "c"]),
            unmatched: set(&["t1"]),
            purge: set(&["d1", "d2"]),
            ..Default::default()
        };
        assert_eq!(
            w.message(),
            "Finances: 3 to review, 1 unmatched\nArchive: 2 due for purge"
        );
    }

    #[test]
    fn a_tap_opens_the_tab_with_the_most_waiting() {
        let w = Waiting {
            batches: set(&["a"]),
            unmatched: set(&["t1"]),
            suggestions: set(&["p1", "p2", "p3"]),
            ..Default::default()
        };
        assert_eq!(w.tab(), "assistant");
        let tie = Waiting {
            batches: set(&["a"]),
            purge: set(&["d"]),
            ..Default::default()
        };
        assert_eq!(tie.tab(), "finances", "Finances wins a tie");
        assert_eq!(Waiting::default().tab(), "settings");
        let charge = Waiting {
            alerts: [("a1".to_string(), "Large charge".to_string())].into(),
            finance_alerts: set(&["a1"]),
            ..Default::default()
        };
        assert_eq!(
            charge.tab(),
            "finances",
            "a listed large charge is a finances item"
        );
        let source = Waiting {
            alerts: [("a2".to_string(), "Mail import needs you".to_string())].into(),
            ..Default::default()
        };
        assert_eq!(source.tab(), "settings");
    }

    #[test]
    fn a_state_saved_before_finance_alerts_still_decodes() {
        let old = r#"{"batches":["b1"],"suggestions":[],"unmatched":[],"purge":[],"alerts":{}}"#;
        let w: Waiting = serde_json::from_str(old).unwrap();
        assert_eq!(w.batches, set(&["b1"]));
        assert!(w.finance_alerts.is_empty());
    }

    #[test]
    fn due_once_the_hour_has_passed_in_the_users_zone() {
        let tz: Tz = "America/Chicago".parse().unwrap();
        // 18:30 CST on 11-14 is not yet 19:00; 19:05 CST is.
        assert_eq!(due(utc("2026-11-15T00:30:00Z"), tz, 19), None);
        assert_eq!(
            due(utc("2026-11-15T01:05:00Z"), tz, 19),
            Some(NaiveDate::from_ymd_opt(2026, 11, 14).unwrap())
        );
    }

    /// The config says the summary is due at hour 0, so any `now` is past it.
    async fn job() -> (SummaryJob, Database, Arc<Phone>, Arc<SurrealRunStore>) {
        use crate::config::ConfigValue;
        use crate::events::{
            AssistantProjection, BudgetProjection, ConfigProjection, EventStore, NewEvent,
            ProjectionRunner, SurrealEventStore,
        };
        let db = crate::db::test_db().await;
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![
                Box::new(ConfigProjection),
                Box::new(AssistantProjection),
                Box::new(BudgetProjection),
            ],
        );
        runner.init_all().await.unwrap();
        let mut events = Vec::new();
        for (key, v) in [
            (ConfigKey::NotifySummaryHour, ConfigValue::Int(0)),
            (ConfigKey::FeatureLlm, ConfigValue::Bool(false)),
            (ConfigKey::FeatureDocuments, ConfigValue::Bool(false)),
        ] {
            let set = NewEvent::config_set("d1", key, Some(v)).unwrap();
            events.push(
                SurrealEventStore::new(db.clone())
                    .append(set)
                    .await
                    .unwrap(),
            );
        }
        runner.apply_events(&events).await.unwrap();
        // Schemaless here: the summary reads only the record id and `status`.
        db.query("DEFINE TABLE IF NOT EXISTS pending_auto_import_batches SCHEMALESS")
            .await
            .unwrap();
        let runs = Arc::new(SurrealRunStore::new(db.clone()));
        runs.init_schema().await.unwrap();
        let phone = Arc::new(Phone::default());
        let job = SummaryJob::new(db.clone(), runs.clone(), phone.clone());
        job.init_schema().await.unwrap();
        (job, db, phone, runs)
    }

    async fn add_batch(db: &Database, id: &str) {
        db.query(
            "CREATE type::record('pending_auto_import_batches', $id) SET batch_id = $id, \
             status = 'pending'",
        )
        .bind(("id", id.to_string()))
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn sends_once_a_day_and_skips_when_nothing_is_new() {
        let (job, db, phone, runs) = job().await;
        job.tick(Utc::now()).await;
        assert!(
            phone.0.lock().unwrap().is_empty(),
            "nothing waiting, nothing sent"
        );

        add_batch(&db, "b1").await;
        job.tick(Utc::now()).await;
        assert!(phone.0.lock().unwrap().is_empty(), "today already ran");

        // A new day: the state's day is rolled back to yesterday by hand.
        let yesterday = Utc::now().date_naive().pred_opt().unwrap();
        job.save(&State {
            last_day: Some(yesterday),
            last_sent: Waiting::default(),
        })
        .await
        .unwrap();
        job.tick(Utc::now()).await;
        let sent = phone.0.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].message, "Finances: 1 to review");
        assert_eq!(sent[0].priority, Priority::Low);

        // The tick that found today already done records nothing.
        let recorded = runs.recent(Some(REVIEW_SUMMARY), 10).await.unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].counts["sent"], 1);
    }

    #[tokio::test]
    async fn a_failed_send_records_its_own_counts() {
        let (job, db, phone, runs) = job().await;
        phone.1.store(true, std::sync::atomic::Ordering::SeqCst);
        add_batch(&db, "b1").await;
        add_batch(&db, "b2").await;
        job.tick(Utc::now()).await;
        let recorded = runs.recent(Some(REVIEW_SUMMARY), 1).await.unwrap();
        assert!(matches!(recorded[0].outcome, RunOutcome::Failure { .. }));
        assert_eq!(
            recorded[0].counts["batches"], 2,
            "not the last sent summary's"
        );
        assert_eq!(recorded[0].counts["sent"], 0);
    }

    #[tokio::test]
    async fn a_re_pulled_batch_is_not_new() {
        let (job, db, phone, _) = job().await;
        add_batch(&db, "b1").await;
        let yesterday = Utc::now().date_naive().pred_opt().unwrap();
        let config = ResolvedConfig::new(
            crate::events::load_persisted(&db).await.unwrap(),
            Default::default(),
        );
        let waiting = Waiting::gather(&db, &config, Utc::now()).await.unwrap();
        // A re-pull of the same file: the projection rewrites `batch_id` in place.
        db.query("UPDATE type::record('pending_auto_import_batches', 'b1') SET batch_id = 'b2'")
            .await
            .unwrap();
        job.save(&State {
            last_day: Some(yesterday),
            last_sent: waiting,
        })
        .await
        .unwrap();
        job.tick(Utc::now()).await;
        assert!(phone.0.lock().unwrap().is_empty(), "nothing arrived since");
    }
}
