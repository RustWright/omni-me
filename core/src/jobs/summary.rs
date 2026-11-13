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
                "SELECT VALUE batch_id FROM pending_auto_import_batches WHERE status = 'pending'",
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
        Ok(w)
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
            && self.suggestions.is_empty()
            && self.unmatched.is_empty()
            && self.purge.is_empty()
    }

    /// Whether anything is here that the last summary did not list.
    pub fn has_new_since(&self, last: &Waiting) -> bool {
        !self.batches.is_subset(&last.batches)
            || !self.suggestions.is_subset(&last.suggestions)
            || !self.unmatched.is_subset(&last.unmatched)
            || !self.purge.is_subset(&last.purge)
    }

    pub fn counts(&self) -> BTreeMap<String, u64> {
        [
            ("batches", self.batches.len()),
            ("suggestions", self.suggestions.len()),
            ("unmatched", self.unmatched.len()),
            ("purge", self.purge.len()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v as u64))
        .collect()
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
        Ok(())
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
        let outcome = match Waiting::gather(&self.db, &config, now).await {
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
                };
                match self.notifier.send(&n).await {
                    Ok(()) => {
                        sent = 1;
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
        run.counts = state.last_sent.counts();
        run.counts.insert("sent".into(), sent);
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
    struct Phone(Mutex<Vec<Notification>>);

    #[async_trait::async_trait]
    impl Notifier for Phone {
        async fn send(&self, n: &Notification) -> Result<(), String> {
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
        // Schemaless here: the summary reads only `batch_id` and `status`.
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
}
