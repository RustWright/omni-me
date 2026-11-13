//! Run records: what each run of a job did, kept on the host that ran it.
//!
//! A server-local table, not events: a poll every half hour per account would put
//! thousands of rows a month into a log that is permanent and syncs to every device.
//! Rows older than [`RETENTION_DAYS`] are pruned as new ones are written.

use super::spend::Spend;
use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use surrealdb::types::SurrealValue;

use crate::db::Database;

/// Long enough for the critical-alert budget to be measured over weeks.
pub const RETENTION_DAYS: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunOutcome {
    Success,
    Failure {
        error: String,
    },
    /// The job cannot continue until the user acts: a code, a reconnect.
    NeedsUser {
        reason: String,
    },
}

impl RunOutcome {
    fn kind(&self) -> &'static str {
        match self {
            RunOutcome::Success => "success",
            RunOutcome::Failure { .. } => "failure",
            RunOutcome::NeedsUser { .. } => "needs_user",
        }
    }

    fn detail(&self) -> Option<&str> {
        match self {
            RunOutcome::Success => None,
            RunOutcome::Failure { error } => Some(error),
            RunOutcome::NeedsUser { reason } => Some(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub job: String,
    /// What the run was for within the job, such as one mail account.
    pub subject: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub outcome: RunOutcome,
    pub counts: BTreeMap<String, u64>,
    pub spend: Spend,
}

impl RunRecord {
    pub fn new(
        job: &str,
        subject: &str,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
        outcome: RunOutcome,
    ) -> Self {
        Self {
            id: ulid::Ulid::new().to_string(),
            job: job.to_string(),
            subject: subject.to_string(),
            started_at,
            ended_at,
            outcome,
            counts: BTreeMap::new(),
            spend: Spend::default(),
        }
    }
}

/// Told about every recorded run, after it is stored. The alert engine is one.
#[async_trait]
pub trait RunObserver: Send + Sync {
    async fn observe(&self, run: &RunRecord);
}

#[async_trait]
pub trait RunStore: Send + Sync {
    async fn record(&self, run: &RunRecord) -> Result<(), String>;
    /// Newest first. `job` of `None` lists every job.
    async fn recent(&self, job: Option<&str>, limit: usize) -> Result<Vec<RunRecord>, String>;
    /// Every run of `job` that ended at or after `since`, newest first.
    async fn since(&self, job: &str, since: DateTime<Utc>) -> Result<Vec<RunRecord>, String>;
    /// The newest successful run of `job` for `subject`.
    async fn last_success(&self, job: &str, subject: &str) -> Result<Option<RunRecord>, String>;
}

/// Times are stored as fixed-width RFC 3339 strings, so ordering by them is
/// chronological and nothing has to convert a SurrealDB datetime back.
fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[derive(Debug, SurrealValue)]
struct RunRow {
    run_id: String,
    job: String,
    subject: String,
    started_at: String,
    ended_at: String,
    outcome: String,
    detail: Option<String>,
    counts: String,
    spend: String,
}

impl RunRow {
    fn into_record(self) -> Result<RunRecord, String> {
        let time = |s: &str| {
            DateTime::parse_from_rfc3339(s)
                .map(|t| t.with_timezone(&Utc))
                .map_err(|e| format!("run {}: bad time {s}: {e}", self.run_id))
        };
        let detail = self.detail.clone().unwrap_or_default();
        let outcome = match self.outcome.as_str() {
            "success" => RunOutcome::Success,
            "needs_user" => RunOutcome::NeedsUser { reason: detail },
            _ => RunOutcome::Failure { error: detail },
        };
        Ok(RunRecord {
            started_at: time(&self.started_at)?,
            ended_at: time(&self.ended_at)?,
            counts: serde_json::from_str(&self.counts).map_err(|e| e.to_string())?,
            spend: serde_json::from_str(&self.spend).map_err(|e| e.to_string())?,
            id: self.run_id,
            job: self.job,
            subject: self.subject,
            outcome,
        })
    }
}

const COLUMNS: &str = "run_id, job, subject, started_at, ended_at, outcome, detail, counts, spend";

pub struct SurrealRunStore {
    db: Database,
}

impl SurrealRunStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Idempotent. Call once at startup.
    pub async fn init_schema(&self) -> Result<(), String> {
        self.db
            .query(
                "DEFINE TABLE IF NOT EXISTS job_runs SCHEMAFULL;
                 DEFINE FIELD IF NOT EXISTS run_id ON job_runs TYPE string;
                 DEFINE FIELD IF NOT EXISTS job ON job_runs TYPE string;
                 DEFINE FIELD IF NOT EXISTS subject ON job_runs TYPE string;
                 DEFINE FIELD IF NOT EXISTS started_at ON job_runs TYPE string;
                 DEFINE FIELD IF NOT EXISTS ended_at ON job_runs TYPE string;
                 DEFINE FIELD IF NOT EXISTS outcome ON job_runs TYPE string;
                 DEFINE FIELD IF NOT EXISTS detail ON job_runs TYPE option<string>;
                 DEFINE FIELD IF NOT EXISTS counts ON job_runs TYPE string;
                 DEFINE FIELD IF NOT EXISTS spend ON job_runs TYPE string;
                 DEFINE INDEX IF NOT EXISTS job_runs_job_end ON job_runs FIELDS job, ended_at;
                 DEFINE INDEX IF NOT EXISTS job_runs_end ON job_runs FIELDS ended_at;",
            )
            .await
            .map_err(|e| format!("init job_runs: {e}"))?
            .check()
            .map_err(|e| format!("init job_runs: {e}"))?;
        Ok(())
    }

    async fn select(
        &self,
        sql: String,
        binds: Vec<(&'static str, String)>,
    ) -> Result<Vec<RunRecord>, String> {
        let mut query = self.db.query(sql);
        for (k, v) in binds {
            query = query.bind((k, v));
        }
        let mut resp = query.await.map_err(|e| format!("read job_runs: {e}"))?;
        let rows: Vec<RunRow> = resp.take(0).map_err(|e| format!("decode job_runs: {e}"))?;
        rows.into_iter().map(RunRow::into_record).collect()
    }
}

#[async_trait]
impl RunStore for SurrealRunStore {
    async fn record(&self, run: &RunRecord) -> Result<(), String> {
        let cutoff = stamp(Utc::now() - chrono::Duration::days(RETENTION_DAYS));
        self.db
            .query(
                "CREATE type::record('job_runs', $run_id) CONTENT {
                    run_id: $run_id, job: $job, subject: $subject,
                    started_at: $started_at, ended_at: $ended_at,
                    outcome: $outcome, detail: $detail, counts: $counts, spend: $spend
                 };
                 DELETE job_runs WHERE ended_at < $cutoff;",
            )
            .bind(("run_id", run.id.clone()))
            .bind(("job", run.job.clone()))
            .bind(("subject", run.subject.clone()))
            .bind(("started_at", stamp(run.started_at)))
            .bind(("ended_at", stamp(run.ended_at)))
            .bind(("outcome", run.outcome.kind().to_string()))
            .bind(("detail", run.outcome.detail().map(str::to_string)))
            .bind((
                "counts",
                serde_json::to_string(&run.counts).map_err(|e| e.to_string())?,
            ))
            .bind((
                "spend",
                serde_json::to_string(&run.spend).map_err(|e| e.to_string())?,
            ))
            .bind(("cutoff", cutoff))
            .await
            .map_err(|e| format!("write job_runs: {e}"))?
            .check()
            .map_err(|e| format!("write job_runs: {e}"))?;
        Ok(())
    }

    async fn recent(&self, job: Option<&str>, limit: usize) -> Result<Vec<RunRecord>, String> {
        let limit = limit.clamp(1, 500);
        match job {
            Some(job) => {
                self.select(
                    format!("SELECT {COLUMNS} FROM job_runs WHERE job = $job ORDER BY ended_at DESC LIMIT {limit}"),
                    vec![("job", job.to_string())],
                )
                .await
            }
            None => {
                self.select(
                    format!("SELECT {COLUMNS} FROM job_runs ORDER BY ended_at DESC LIMIT {limit}"),
                    vec![],
                )
                .await
            }
        }
    }

    async fn since(&self, job: &str, since: DateTime<Utc>) -> Result<Vec<RunRecord>, String> {
        self.select(
            format!("SELECT {COLUMNS} FROM job_runs WHERE job = $job AND ended_at >= $since ORDER BY ended_at DESC"),
            vec![("job", job.to_string()), ("since", stamp(since))],
        )
        .await
    }

    async fn last_success(&self, job: &str, subject: &str) -> Result<Option<RunRecord>, String> {
        let runs = self
            .select(
                format!(
                    "SELECT {COLUMNS} FROM job_runs WHERE job = $job AND subject = $subject \
                     AND outcome = 'success' ORDER BY ended_at DESC LIMIT 1"
                ),
                vec![("job", job.to_string()), ("subject", subject.to_string())],
            )
            .await?;
        Ok(runs.into_iter().next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> SurrealRunStore {
        let s = SurrealRunStore::new(crate::db::test_db().await);
        s.init_schema().await.unwrap();
        s
    }

    fn run(job: &str, subject: &str, ago_min: i64, outcome: RunOutcome) -> RunRecord {
        let end = Utc::now() - chrono::Duration::minutes(ago_min);
        let mut r = RunRecord::new(
            job,
            subject,
            end - chrono::Duration::seconds(5),
            end,
            outcome,
        );
        r.counts.insert("fetched".into(), 3);
        r.spend.calls = 2;
        r.spend.cost_micros = 40;
        r
    }

    #[tokio::test]
    async fn a_recorded_run_reads_back_whole() {
        let s = store().await;
        let r = run(
            "mail_import",
            "gmail",
            1,
            RunOutcome::NeedsUser {
                reason: "app password".into(),
            },
        );
        s.record(&r).await.unwrap();
        let back = s.recent(Some("mail_import"), 10).await.unwrap();
        assert_eq!(back.len(), 1);
        let got = &back[0];
        assert_eq!(got.outcome, r.outcome);
        assert_eq!(got.counts, r.counts);
        assert_eq!(got.spend, r.spend);
        assert_eq!(
            got.started_at.timestamp_millis(),
            r.started_at.timestamp_millis()
        );
    }

    #[tokio::test]
    async fn recent_is_newest_first_and_filters_by_job() {
        let s = store().await;
        s.record(&run("mail_import", "a", 30, RunOutcome::Success))
            .await
            .unwrap();
        s.record(&run("mail_import", "a", 1, RunOutcome::Success))
            .await
            .unwrap();
        s.record(&run("bank_import", "b", 2, RunOutcome::Success))
            .await
            .unwrap();
        let mail = s.recent(Some("mail_import"), 10).await.unwrap();
        assert_eq!(mail.len(), 2);
        assert!(mail[0].ended_at > mail[1].ended_at);
        assert_eq!(s.recent(None, 10).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn last_success_skips_failures_and_other_subjects() {
        let s = store().await;
        let ok = run("mail_import", "a", 60, RunOutcome::Success);
        s.record(&ok).await.unwrap();
        s.record(&run(
            "mail_import",
            "a",
            5,
            RunOutcome::Failure {
                error: "dns".into(),
            },
        ))
        .await
        .unwrap();
        s.record(&run("mail_import", "b", 1, RunOutcome::Success))
            .await
            .unwrap();
        let got = s.last_success("mail_import", "a").await.unwrap().unwrap();
        assert_eq!(got.id, ok.id);
    }

    #[tokio::test]
    async fn writing_a_run_prunes_rows_past_retention() {
        let s = store().await;
        s.record(&run(
            "mail_import",
            "a",
            (RETENTION_DAYS + 1) * 24 * 60,
            RunOutcome::Success,
        ))
        .await
        .unwrap();
        s.record(&run("mail_import", "a", 1, RunOutcome::Success))
            .await
            .unwrap();
        assert_eq!(s.recent(None, 10).await.unwrap().len(), 1);
    }
}
