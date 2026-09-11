//! Autonomy: the evidence for granting it, and the grant itself.
//!
//! `docs/src/assistant.md`'s destination is one rule — **reversible things it may
//! do freely; irreversible things it always asks about** — and this module is
//! where that stops being a sentence and becomes a refusal.
//!
//! ## The evidence is a query, not a new record
//!
//! Every proposal already records what was proposed and what the user decided, so
//! the approval rate per action type is derivable from what the inbox has been
//! doing all along. Nothing is written to track it. That matters beyond
//! tidiness: a separately-maintained counter is a second place to be wrong, and
//! it would be wrong *quietly*, because a plausible-looking rate is exactly the
//! kind of number nobody re-derives.
//!
//! ## Who grants
//!
//! ⚠️ **The user, always.** There is no verb, no action and no proposal that
//! produces a grant. An assistant able to ask for more power — however politely,
//! however gated — inverts the model this whole phase is built on. What it may do
//! is behave well enough, for long enough, that the record speaks for it.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::assistant::actions;
use crate::config::ResolvedConfig;
use crate::db::Database;
use crate::events::{
    AutonomyGrantedPayload, AutonomyRevokedPayload, EventError, EventType, EventWriter, NewEvent,
    WriteError,
};

/// How a single action type has fared at the approval gate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionRecord {
    pub action: String,
    pub approved: u32,
    pub rejected: u32,
    /// Still waiting. Counted separately rather than folded into either side —
    /// an undecided proposal is not evidence of anything.
    pub pending: u32,
    /// Whether the assistant may currently carry this out without asking.
    pub granted: bool,
    /// What the action declares today. Grant is refused unless this is true.
    pub reversible: bool,
}

impl ActionRecord {
    /// Share of decided proposals that were approved, or `None` when none have
    /// been decided.
    ///
    /// ⚠️ `None` rather than `0.0`. A rate of zero and "never used" are different
    /// facts and would drive opposite decisions; collapsing them into one number
    /// is how an unused action ends up looking like a badly-behaved one.
    pub fn approval_rate(&self) -> Option<f64> {
        let decided = self.approved + self.rejected;
        (decided > 0).then(|| f64::from(self.approved) / f64::from(decided))
    }

    pub fn decided(&self) -> u32 {
        self.approved + self.rejected
    }
}

/// Why a grant did not happen.
#[derive(Debug, thiserror::Error)]
pub enum GrantError {
    #[error("`{0}` is not an action this build can carry out")]
    UnknownAction(String),
    /// The rule, enforced rather than advised.
    #[error(
        "`{0}` cannot be granted autonomy: it is irreversible, and irreversible actions always ask"
    )]
    Irreversible(String),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Event(#[from] EventError),
}

/// The approval record for every action this build knows, with its grant state.
///
/// Includes actions that have never been proposed, reporting zeroes: "this exists
/// and has never come up" is the answer a person needs before deciding, and an
/// action missing from the list entirely reads as one that does not exist.
pub async fn records(
    db: &Database,
    config: &ResolvedConfig,
) -> Result<Vec<ActionRecord>, EventError> {
    let granted = granted_actions(db).await?;

    let mut resp = db
        .query("SELECT action, decision FROM assistant_proposals")
        .await?
        .check()?;
    let names: Vec<Option<String>> = resp.take("action").unwrap_or_default();
    let decisions: Vec<Option<String>> = resp.take("decision").unwrap_or_default();

    let mut out = Vec::new();
    for action in actions::available(config) {
        let mut record = ActionRecord {
            action: action.name.to_string(),
            approved: 0,
            rejected: 0,
            pending: 0,
            granted: granted.contains(&action.name.to_string()),
            reversible: action.reversible,
        };
        for (i, name) in names.iter().enumerate() {
            if name.as_deref() != Some(action.name) {
                continue;
            }
            match decisions.get(i).cloned().flatten().as_deref() {
                None => record.pending += 1,
                Some("approved") => record.approved += 1,
                Some("rejected") => record.rejected += 1,
                // A decision a newer build wrote. Counted as neither approved nor
                // rejected: guessing which it resembles would put an invented
                // number in front of a permission decision.
                Some(_) => {}
            }
        }
        out.push(record);
    }
    Ok(out)
}

/// The action names currently granted autonomy.
pub async fn granted_actions(db: &Database) -> Result<Vec<String>, EventError> {
    let mut resp = db
        .query("SELECT action FROM assistant_autonomy WHERE granted = true")
        .await?
        .check()?;
    Ok(resp.take("action").unwrap_or_default())
}

/// Whether one action may currently be carried out without asking.
///
/// ⚠️ **Both halves are checked, every time.** A grant is not enough on its own:
/// the action must *still* declare itself reversible. A build that changed an
/// action's reversibility must not have old grants keep applying to it, and
/// checking only the stored flag is how that would happen silently.
pub async fn is_autonomous(
    db: &Database,
    config: &ResolvedConfig,
    action_name: &str,
) -> Result<bool, EventError> {
    let Some(action) = actions::lookup(config, action_name) else {
        return Ok(false);
    };
    if !action.reversible {
        return Ok(false);
    }
    Ok(granted_actions(db).await?.iter().any(|a| a == action_name))
}

/// Allow one action to be carried out without asking.
///
/// Refuses an irreversible action outright. That is the published rule's whole
/// enforcement — a check anywhere downstream would be one a future caller could
/// route around, while a grant that cannot be written cannot be honoured by
/// anything.
pub async fn grant(
    config: &ResolvedConfig,
    writer: &EventWriter,
    action_name: &str,
) -> Result<(), GrantError> {
    let action = actions::lookup(config, action_name)
        .ok_or_else(|| GrantError::UnknownAction(action_name.to_string()))?;
    if !action.reversible {
        return Err(GrantError::Irreversible(action_name.to_string()));
    }

    let payload = AutonomyGrantedPayload {
        action: action.name.to_string(),
        reversible: action.reversible,
    };
    let event = NewEvent {
        id: None,
        event_type: EventType::AutonomyGranted.to_string(),
        aggregate_id: action.name.to_string(),
        timestamp: Utc::now(),
        device_id: writer.device_id().to_string(),
        payload: serde_json::to_value(&payload).map_err(|e| {
            GrantError::Write(WriteError::Event(EventError::Validation(e.to_string())))
        })?,
    };
    writer.append_new(event).await?;
    Ok(())
}

/// Take the permission back.
///
/// ⚠️ Deliberately does **not** check the action against the registry. Revoking a
/// grant for an action this build no longer knows must still work — otherwise a
/// renamed or withdrawn action could leave a permission nothing can reach, which
/// is the one direction of failure a permission system must not have.
pub async fn revoke(
    writer: &EventWriter,
    action_name: &str,
    reason: Option<String>,
) -> Result<(), GrantError> {
    let payload = AutonomyRevokedPayload {
        action: action_name.to_string(),
        reason,
    };
    let event = NewEvent {
        id: None,
        event_type: EventType::AutonomyRevoked.to_string(),
        aggregate_id: action_name.to_string(),
        timestamp: Utc::now(),
        device_id: writer.device_id().to_string(),
        payload: serde_json::to_value(&payload).map_err(|e| {
            GrantError::Write(WriteError::Event(EventError::Validation(e.to_string())))
        })?,
    };
    writer.append_new(event).await?;
    Ok(())
}

/// When a grant or revocation was last decided, for display.
pub async fn decided_at(
    db: &Database,
    action_name: &str,
) -> Result<Option<DateTime<Utc>>, EventError> {
    let mut resp = db
        .query(
            "SELECT <string> decided_at AS decided_at
             FROM type::record('assistant_autonomy', $action)",
        )
        .bind(("action", action_name.to_string()))
        .await?
        .check()?;
    let rows: Vec<Option<String>> = resp.take("decided_at").unwrap_or_default();
    Ok(rows
        .first()
        .cloned()
        .flatten()
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&Utc)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::config::{ConfigMap, Feature};
    use crate::events::{
        AssistantProjection, AssistantProposalMadePayload, BeliefsProjection, EventStore,
        NotesProjection, ProjectionRunner, ProposalDecision, SurrealEventStore,
    };

    async fn harness() -> (Database, ResolvedConfig, EventWriter) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("promotion.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);

        let config = ResolvedConfig::new(ConfigMap::new(), ConfigMap::new());
        let store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new(db.clone()));
        let projections = ProjectionRunner::new(
            db.clone(),
            vec![
                Box::new(AssistantProjection),
                Box::new(NotesProjection),
                Box::new(BeliefsProjection),
            ],
        );
        projections.init_all().await.expect("init projections");
        let writer = EventWriter::from_config(store, projections, &config, "phone");
        (db, config, writer)
    }

    async fn propose_and_decide(
        db: &Database,
        config: &ResolvedConfig,
        writer: &EventWriter,
        id: &str,
        decision: Option<ProposalDecision>,
    ) {
        let payload = AssistantProposalMadePayload {
            proposal_id: id.into(),
            thread_id: "t1".into(),
            message_id: "m1".into(),
            action: "note.create".into(),
            args: serde_json::json!({ "title": "t", "body": "b" }),
            rationale: "because".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .unwrap();
        if let Some(d) = decision {
            crate::assistant::inbox::decide(db, config, writer, id, d, None)
                .await
                .unwrap();
        }
    }

    fn record_for<'a>(records: &'a [ActionRecord], name: &str) -> &'a ActionRecord {
        records.iter().find(|r| r.action == name).expect(name)
    }

    #[tokio::test]
    async fn every_known_action_appears_even_with_no_history() {
        let (db, config, _writer) = harness().await;
        let all = records(&db, &config).await.unwrap();
        let names: Vec<&str> = all.iter().map(|r| r.action.as_str()).collect();
        assert!(names.contains(&"note.create"), "{names:?}");
        assert!(names.contains(&"belief.record"), "{names:?}");

        let note = record_for(&all, "note.create");
        assert_eq!((note.approved, note.rejected, note.pending), (0, 0, 0));
        assert!(!note.granted, "nothing is granted by default");
    }

    /// ⚠️ Never used and never approved are different facts, and they would drive
    /// opposite decisions. A rate of `0.0` for an action nobody has tried would
    /// read as one the assistant keeps getting wrong.
    #[tokio::test]
    async fn an_unused_action_has_no_rate_rather_than_a_rate_of_zero() {
        let (db, config, _writer) = harness().await;
        let all = records(&db, &config).await.unwrap();
        assert_eq!(record_for(&all, "note.create").approval_rate(), None);
    }

    #[tokio::test]
    async fn the_rate_counts_decided_proposals_and_ignores_pending_ones() {
        let (db, config, writer) = harness().await;
        propose_and_decide(
            &db,
            &config,
            &writer,
            "p1",
            Some(ProposalDecision::Approved),
        )
        .await;
        propose_and_decide(
            &db,
            &config,
            &writer,
            "p2",
            Some(ProposalDecision::Approved),
        )
        .await;
        propose_and_decide(
            &db,
            &config,
            &writer,
            "p3",
            Some(ProposalDecision::Rejected),
        )
        .await;
        propose_and_decide(&db, &config, &writer, "p4", None).await;

        let all = records(&db, &config).await.unwrap();
        let note = record_for(&all, "note.create");
        assert_eq!((note.approved, note.rejected, note.pending), (2, 1, 1));
        assert_eq!(note.decided(), 3, "a pending proposal is not evidence");
        assert!((note.approval_rate().unwrap() - 2.0 / 3.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn granting_then_revoking_moves_the_permission_both_ways() {
        let (db, config, writer) = harness().await;
        assert!(!is_autonomous(&db, &config, "note.create").await.unwrap());

        grant(&config, &writer, "note.create").await.expect("grant");
        assert!(is_autonomous(&db, &config, "note.create").await.unwrap());
        assert!(record_for(&records(&db, &config).await.unwrap(), "note.create").granted);

        revoke(&writer, "note.create", Some("changed my mind".into()))
            .await
            .expect("revoke");
        assert!(!is_autonomous(&db, &config, "note.create").await.unwrap());
    }

    /// ⛔ The published rule, enforced rather than advised: *irreversible things
    /// it always asks about*. Refused at the write site, so no downstream check
    /// can be routed around.
    #[tokio::test]
    async fn an_irreversible_action_cannot_be_granted() {
        let (_db, config, writer) = harness().await;
        // Every action today is reversible, so this asserts against the whole set
        // rather than a fixture — the day an irreversible one is added, this test
        // covers it without being edited.
        for action in actions::available(&config) {
            let outcome = grant(&config, &writer, action.name).await;
            if action.reversible {
                assert!(outcome.is_ok(), "{} should be grantable", action.name);
            } else {
                assert!(
                    matches!(outcome, Err(GrantError::Irreversible(_))),
                    "{} is irreversible and must be refused",
                    action.name
                );
            }
        }
    }

    #[tokio::test]
    async fn granting_an_action_that_does_not_exist_says_so() {
        let (_db, config, writer) = harness().await;
        assert!(matches!(
            grant(&config, &writer, "nonsense.act").await,
            Err(GrantError::UnknownAction(_))
        ));
    }

    /// ⚠️ A permission must always be reachable to take back. Refusing to revoke
    /// an unknown action would strand a grant for a renamed or withdrawn one —
    /// the one direction of failure a permission system cannot have.
    #[tokio::test]
    async fn a_grant_for_an_unknown_action_can_still_be_revoked() {
        let (db, _config, writer) = harness().await;
        revoke(&writer, "withdrawn.action", None)
            .await
            .expect("revoking must never depend on the registry");
        assert!(
            !granted_actions(&db)
                .await
                .unwrap()
                .contains(&"withdrawn.action".to_string())
        );
    }

    /// ⚠️ Both halves are checked on every use. If an action's reversibility
    /// changes in a later build, an old grant must stop applying — otherwise the
    /// rule silently stops holding for exactly the actions it matters most for.
    #[tokio::test]
    async fn a_grant_does_not_apply_to_an_action_the_config_hides() {
        let (db, config, writer) = harness().await;
        grant(&config, &writer, "note.create").await.unwrap();

        let mut global = ConfigMap::new();
        global.insert(
            Feature::Notes.key(),
            crate::config::ConfigValue::Bool(false),
        );
        let notes_off = ResolvedConfig::new(global, ConfigMap::new());
        assert!(
            !is_autonomous(&db, &notes_off, "note.create").await.unwrap(),
            "an action its feature hides must not run autonomously"
        );
    }
}
