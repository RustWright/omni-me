//! The job status view: every job on the contract with its recent runs.
//!
//! `GET /jobs` lists the registry with each job's latest run per subject and its
//! last 24 hours. `GET /jobs/{name}/runs` is one job's run history, newest first.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use omni_me_core::jobs::runs::{RunOutcome, RunRecord};
use omni_me_core::jobs::{self, JobSpec, PENDING, PendingJob};

use crate::AppState;

pub fn jobs_routes() -> Router<AppState> {
    Router::new()
        .route("/jobs", get(list_handler))
        .route("/jobs/{name}/runs", get(runs_handler))
}

#[derive(Serialize)]
struct JobsView {
    jobs: Vec<JobView>,
    pending: &'static [PendingJob],
}

#[derive(Serialize)]
struct JobView {
    #[serde(flatten)]
    spec: JobSpec,
    health: &'static str,
    /// The newest run of each subject seen in the recent history.
    latest: Vec<RunRecord>,
    runs_24h: usize,
    failures_24h: usize,
    cost_micros_24h: u64,
}

/// The worst state across subjects' latest runs.
fn health(latest: &[RunRecord]) -> &'static str {
    if latest.is_empty() {
        "unknown"
    } else if latest
        .iter()
        .any(|r| matches!(r.outcome, RunOutcome::NeedsUser { .. }))
    {
        "needs_user"
    } else if latest
        .iter()
        .any(|r| matches!(r.outcome, RunOutcome::Failure { .. }))
    {
        "degraded"
    } else {
        "healthy"
    }
}

async fn list_handler(
    State(state): State<AppState>,
) -> Result<Json<JobsView>, (StatusCode, String)> {
    let store = state.auto_import_registry.run_store();
    let mut views = Vec::new();
    for spec in jobs::REGISTRY {
        let (recent, day) = match &store {
            Some(s) => {
                let err = |e: String| (StatusCode::INTERNAL_SERVER_ERROR, e);
                let recent = s.recent(Some(spec.name), 200).await.map_err(err)?;
                let day = s
                    .since(spec.name, Utc::now() - Duration::hours(24))
                    .await
                    .map_err(err)?;
                (recent, day)
            }
            None => (Vec::new(), Vec::new()),
        };
        let mut by_subject: BTreeMap<String, RunRecord> = BTreeMap::new();
        for run in recent {
            by_subject.entry(run.subject.clone()).or_insert(run);
        }
        let latest: Vec<RunRecord> = by_subject.into_values().collect();
        views.push(JobView {
            spec: *spec,
            health: health(&latest),
            runs_24h: day.len(),
            failures_24h: day
                .iter()
                .filter(|r| r.outcome != RunOutcome::Success)
                .count(),
            cost_micros_24h: day.iter().map(|r| r.spend.cost_micros).sum(),
            latest,
        });
    }
    Ok(Json(JobsView {
        jobs: views,
        pending: PENDING,
    }))
}

#[derive(Deserialize)]
struct RunsQuery {
    limit: Option<usize>,
}

async fn runs_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<RunsQuery>,
) -> Result<Json<Vec<RunRecord>>, (StatusCode, String)> {
    if jobs::spec(&name).is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("no job named {name} on the contract"),
        ));
    }
    let Some(store) = state.auto_import_registry.run_store() else {
        return Ok(Json(Vec::new()));
    };
    store
        .recent(Some(&name), q.limit.unwrap_or(20))
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(subject: &str, outcome: RunOutcome) -> RunRecord {
        RunRecord::new(jobs::MAIL_IMPORT, subject, Utc::now(), Utc::now(), outcome)
    }

    #[test]
    fn health_is_the_worst_subject() {
        assert_eq!(health(&[]), "unknown");
        let ok = run("a", RunOutcome::Success);
        let down = run(
            "b",
            RunOutcome::Failure {
                error: "dns".into(),
            },
        );
        let code = run(
            "c",
            RunOutcome::NeedsUser {
                reason: "otp".into(),
            },
        );
        assert_eq!(health(std::slice::from_ref(&ok)), "healthy");
        assert_eq!(health(&[ok.clone(), down.clone()]), "degraded");
        assert_eq!(health(&[ok, down, code]), "needs_user");
    }
}
