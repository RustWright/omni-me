//! The job contract: one declared shape for everything that runs on its own.
//!
//! Each job on the contract declares the same record ([`JobSpec`]) and leaves a run
//! record per run ([`runs`]). Why each field exists and the placement rule are in
//! `docs/src/agents.md` § The job contract.

pub mod alerts;
pub mod runs;
pub mod spend;
pub mod summary;

use serde::Serialize;

use crate::credentials::LlmRole;
use crate::events::{
    AssistantProjection, AutoImportProjection, BudgetProjection, ConfigProjection,
    DocumentsProjection, NotesProjection, RetentionProjection,
};

pub const MAIL_IMPORT: &str = "mail_import";
pub const MAIL_BACKFILL: &str = "mail_backfill";
pub const BANK_IMPORT: &str = "bank_import";

/// Who chooses the next step. `docs/src/agents.md` § Agent, workflow, or plain code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Agent,
    Workflow,
    PlainCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Host {
    Device,
    Server,
    AgentHost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum Trigger {
    /// Repeats on an interval; the detail names where the interval is set.
    Poll(&'static str),
    Calendar(&'static str),
    OnEvent(&'static str),
    OnRequest(&'static str),
}

/// What happens if he never sees the output. `project.md` § Notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Critical,
    Review,
    Ambient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Read,
    Propose,
    Autonomous,
}

/// How a failed run is handled, and when a failure becomes an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FailurePolicy {
    pub retry: &'static str,
    /// A run that needs the user (a code, a reconnect) alerts as critical at once.
    pub alert_needs_user: bool,
    /// Alert as critical once a subject has gone this long without a success.
    pub alert_failing_hours: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Output {
    pub lands_in: &'static str,
    pub tier: Tier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Seat {
    #[serde(serialize_with = "role_letter")]
    pub role: LlmRole,
    pub purpose: &'static str,
    /// Effort or thinking as the job sets it. "provider default" means it sets none.
    pub effort: &'static str,
}

fn role_letter<S: serde::Serializer>(role: &LlmRole, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(match role {
        LlmRole::Interactive => "A",
        LlmRole::Batch => "B",
        LlmRole::Triage => "C0",
        LlmRole::Extractor => "C1",
        LlmRole::Reader => "C2",
        LlmRole::Transcriber => "C3",
        LlmRole::Structurer => "D",
    })
}

/// Declared by every job that calls a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ModelUse {
    pub seats: &'static [Seat],
    /// The per-run budget as enforced today. Declared, so an unbounded one is visible.
    pub budget: &'static str,
    pub permission: Permission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct JobSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub shape: Shape,
    pub trigger: Trigger,
    pub host: Host,
    pub off_switch: &'static str,
    pub failure: FailurePolicy,
    pub output: Output,
    pub model: Option<ModelUse>,
    /// Projections whose tables the job reads. The host guard test holds each
    /// job to a host that folds all of them.
    pub reads: &'static [&'static str],
}

/// Every job on the contract. A job joins as it migrates, never in a mass move.
pub const REGISTRY: &[JobSpec] = &[
    JobSpec {
        name: MAIL_IMPORT,
        title: "Mail import",
        shape: Shape::Workflow,
        trigger: Trigger::Poll("per source, 30 min default (OMNI_AUTO_IMPORT_INTERVAL_SECS)"),
        host: Host::Server,
        off_switch: "enable_imap at deploy; Pause per account in Settings",
        failure: FailurePolicy {
            retry: "exponential backoff, 1 s doubling to 1 h; halts on a reconnect until Reconnect",
            alert_needs_user: true,
            alert_failing_hours: Some(24),
        },
        output: Output {
            lands_in: "Finances review, and the archive",
            tier: Tier::Review,
        },
        model: Some(ModelUse {
            seats: &[
                Seat {
                    role: LlmRole::Triage,
                    purpose: "could this email record money",
                    effort: "provider default, 8 output tokens",
                },
                Seat {
                    role: LlmRole::Extractor,
                    purpose: "the email to draft transactions",
                    effort: "provider default",
                },
            ],
            budget: "no cap per run; per email one triage call and at most 3 extraction attempts",
            permission: Permission::Propose,
        }),
        reads: &[
            ConfigProjection::NAME,
            AutoImportProjection::NAME,
            DocumentsProjection::NAME,
            NotesProjection::NAME,
            BudgetProjection::NAME,
        ],
    },
    JobSpec {
        name: BANK_IMPORT,
        title: "Bank import",
        shape: Shape::PlainCode,
        trigger: Trigger::Poll("per source: its schedule_secs, else the 30 min default"),
        host: Host::Server,
        off_switch: "enable_bank at deploy; Pause per source in Settings",
        failure: FailurePolicy {
            retry: "exponential backoff, 1 s doubling to 1 h; halts on a code request until Reconnect",
            alert_needs_user: true,
            alert_failing_hours: Some(24),
        },
        output: Output {
            lands_in: "Finances review",
            tier: Tier::Review,
        },
        model: None,
        // Dedup also reads the raw event log, which every host holds.
        reads: &[AutoImportProjection::NAME],
    },
    JobSpec {
        name: summary::REVIEW_SUMMARY,
        title: "Daily review summary",
        shape: Shape::PlainCode,
        trigger: Trigger::Calendar("daily at notify.summary_hour (19:00 default), user's zone"),
        host: Host::Server,
        off_switch: "OMNI_NTFY_URL/TOPIC unset; skipped on its own when nothing is new",
        failure: FailurePolicy {
            retry: "a failed run leaves the day open; retried every 5 min until it sends",
            alert_needs_user: false,
            alert_failing_hours: None,
        },
        output: Output {
            lands_in: "one silent ntfy notification of counts",
            tier: Tier::Review,
        },
        model: None,
        reads: &[
            ConfigProjection::NAME,
            AssistantProjection::NAME,
            AutoImportProjection::NAME,
            BudgetProjection::NAME,
            DocumentsProjection::NAME,
            RetentionProjection::NAME,
        ],
    },
];

/// Jobs not yet on the contract, so the registry still lists everything that runs.
/// Fields are from the verified table in `docs/src/agents.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PendingJob {
    pub title: &'static str,
    pub shape: Shape,
    pub host: Host,
}

pub const PENDING: &[PendingJob] = &[
    PendingJob {
        title: "Answer a question",
        shape: Shape::Agent,
        host: Host::AgentHost,
    },
    PendingJob {
        title: "Daily check-in",
        shape: Shape::Agent,
        host: Host::AgentHost,
    },
    PendingJob {
        title: "Chat tidying",
        shape: Shape::PlainCode,
        host: Host::AgentHost,
    },
    PendingJob {
        title: "Search index refresh",
        shape: Shape::PlainCode,
        host: Host::AgentHost,
    },
    PendingJob {
        title: "Old-mail backfill",
        shape: Shape::Workflow,
        host: Host::Server,
    },
    PendingJob {
        title: "Document enrichment",
        shape: Shape::Workflow,
        host: Host::Server,
    },
    PendingJob {
        title: "Note structuring",
        shape: Shape::Workflow,
        host: Host::Server,
    },
    PendingJob {
        title: "Recurring detection",
        shape: Shape::PlainCode,
        host: Host::Device,
    },
    PendingJob {
        title: "Journal auto-close",
        shape: Shape::PlainCode,
        host: Host::Device,
    },
    PendingJob {
        title: "Retention candidates",
        shape: Shape::PlainCode,
        host: Host::Device,
    },
];

pub fn spec(name: &str) -> Option<&'static JobSpec> {
    REGISTRY.iter().find(|j| j.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigMap, ResolvedConfig};
    use crate::events::registry;
    use std::collections::BTreeSet;

    /// With every feature on, so the guard checks placement rather than config.
    fn folded_on(host: Host) -> BTreeSet<String> {
        let all_on = ResolvedConfig::new(ConfigMap::new(), ConfigMap::new());
        let projections = match host {
            Host::Server => registry::build_projections_server(),
            Host::AgentHost => registry::build_projections_headless(&all_on),
            Host::Device => registry::build_projections(&all_on, std::path::PathBuf::new()),
        };
        projections.iter().map(|p| p.name().to_string()).collect()
    }

    /// The placement rule's guard: a table a host does not fold reads back empty,
    /// and nothing says so.
    #[test]
    fn every_job_sits_on_a_host_that_folds_what_it_reads() {
        for job in REGISTRY {
            let folded = folded_on(job.host);
            for table in job.reads {
                assert!(
                    folded.contains(*table),
                    "{} runs on {:?}, which does not fold {table}: {folded:?}",
                    job.name,
                    job.host
                );
            }
        }
    }

    #[test]
    fn job_names_are_unique_and_never_pending_too() {
        let names: BTreeSet<_> = REGISTRY.iter().map(|j| j.name).collect();
        assert_eq!(names.len(), REGISTRY.len());
        for job in REGISTRY {
            assert!(
                PENDING.iter().all(|p| p.title != job.title),
                "{} is listed twice",
                job.title
            );
        }
    }
}
