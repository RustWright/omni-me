//! The approval inbox: reading proposals back, and deciding them.
//!
//! The decision half lives in core rather than in the Tauri command layer for the
//! same reason [`crate::events::EventWriter`] does — it is the only place where
//! approving a proposal turns into real events, and a second implementation
//! elsewhere would be a second chance to get the atomicity wrong.
//!
//! ## Why approval is one batch
//!
//! [`decide`] appends the action's events **and** the decision marker through a
//! single [`EventWriter::append_batch`]. `commit_batch` in the auto-import path
//! established this and the reason is not tidiness: split into two appends, a
//! crash between them leaves the effect applied and the proposal still pending,
//! so the next approval applies it a second time. One batch makes that
//! unreachable rather than unlikely.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::assistant::actions::{self, ActionType};
use crate::config::ResolvedConfig;
use crate::db::Database;
use crate::events::{
    AssistantProposalDecidedPayload, EventError, EventType, EventWriter, ProposalDecision,
    WriteError,
};

/// One proposal, as the inbox shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub proposal_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub action: String,
    pub args: serde_json::Value,
    pub rationale: String,
    /// What the action declared **when it was proposed**, not what this build
    /// says now. See `AssistantProposalMadePayload::reversible`.
    pub reversible: bool,
    pub created_at: String,
    /// `None` while pending. A value this build cannot parse is still a decision:
    /// the proposal is out of the inbox either way.
    pub decision: Option<String>,
}

impl Proposal {
    pub fn is_pending(&self) -> bool {
        self.decision.is_none()
    }
}

/// Why a proposal could not be acted on.
#[derive(Debug, thiserror::Error)]
pub enum DecideError {
    #[error("no proposal with id `{0}`")]
    NotFound(String),
    /// Already approved or rejected. Not an error the user caused — two devices
    /// showing the same inbox is the ordinary case — so callers surface it as
    /// "already handled" rather than as a failure.
    #[error("proposal `{0}` was already decided")]
    AlreadyDecided(String),
    /// The action was withdrawn, renamed, or its feature switched off since the
    /// proposal was made.
    #[error("`{0}` is not an action this build can carry out")]
    UnknownAction(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Event(#[from] EventError),
}

/// Everything still waiting on the user, newest first.
pub async fn pending(db: &Database) -> Result<Vec<Proposal>, EventError> {
    query_proposals(db, "WHERE decision IS NONE").await
}

/// One proposal by id, decided or not.
pub async fn get(db: &Database, proposal_id: &str) -> Result<Option<Proposal>, EventError> {
    let rows = query_proposals_bound(db, "WHERE proposal_id = $id", Some(proposal_id)).await?;
    Ok(rows.into_iter().next())
}

/// Every proposal made on one thread, so a conversation can show what came of it.
pub async fn for_thread(db: &Database, thread_id: &str) -> Result<Vec<Proposal>, EventError> {
    query_proposals_bound(db, "WHERE thread_id = $id", Some(thread_id)).await
}

async fn query_proposals(db: &Database, filter: &str) -> Result<Vec<Proposal>, EventError> {
    query_proposals_bound(db, filter, None).await
}

async fn query_proposals_bound(
    db: &Database,
    filter: &str,
    id: Option<&str>,
) -> Result<Vec<Proposal>, EventError> {
    // ⚠️ `created_at` is selected as well as ordered on — SurrealDB v3 rejects an
    // `ORDER BY` over a field the projection does not return, and names it a
    // "missing order idiom", which does not read as the cause.
    let sql = format!(
        "SELECT proposal_id, thread_id, message_id, action, args, rationale, reversible,
                <string> created_at AS created_at, decision
         FROM assistant_proposals {filter} ORDER BY created_at DESC"
    );
    let mut resp = match id {
        Some(id) => db.query(sql).bind(("id", id.to_string())).await?.check()?,
        None => db.query(sql).await?.check()?,
    };

    let ids: Vec<String> = resp.take("proposal_id").unwrap_or_default();
    let threads: Vec<Option<String>> = resp.take("thread_id").unwrap_or_default();
    let messages: Vec<Option<String>> = resp.take("message_id").unwrap_or_default();
    let actions_: Vec<Option<String>> = resp.take("action").unwrap_or_default();
    let args: Vec<Option<serde_json::Value>> = resp.take("args").unwrap_or_default();
    let rationales: Vec<Option<String>> = resp.take("rationale").unwrap_or_default();
    let reversible: Vec<Option<bool>> = resp.take("reversible").unwrap_or_default();
    let created: Vec<Option<String>> = resp.take("created_at").unwrap_or_default();
    let decisions: Vec<Option<String>> = resp.take("decision").unwrap_or_default();

    let mut out = Vec::with_capacity(ids.len());
    for (i, proposal_id) in ids.iter().enumerate() {
        // A row whose detail has not arrived yet is skipped, not surfaced as a
        // half-proposal. It can only exist in the window where a decision was
        // folded before the proposal it decides (see the projection's schema
        // note), it is never pending, and there is nothing to show about it.
        let (Some(thread_id), Some(message_id), Some(action), Some(rationale), Some(created_at)) = (
            threads.get(i).cloned().flatten(),
            messages.get(i).cloned().flatten(),
            actions_.get(i).cloned().flatten(),
            rationales.get(i).cloned().flatten(),
            created.get(i).cloned().flatten(),
        ) else {
            continue;
        };

        out.push(Proposal {
            proposal_id: proposal_id.clone(),
            thread_id,
            message_id,
            action,
            args: args
                .get(i)
                .cloned()
                .flatten()
                .unwrap_or(serde_json::Value::Null),
            rationale,
            reversible: reversible.get(i).copied().flatten().unwrap_or(false),
            created_at,
            decision: decisions.get(i).cloned().flatten(),
        });
    }
    Ok(out)
}

/// Approve or reject one proposal.
///
/// On approval the action's events and the decision marker go out in **one**
/// batch; see this module's header for why that is not negotiable. On rejection
/// there is nothing to apply, so the batch is the marker alone.
///
/// ⚠️ Pendingness is checked, and the check is a narrow guard rather than a
/// guarantee. Two devices approving the same proposal at the same moment both see
/// it pending and both apply it — the log is honest about that (two decisions,
/// two applications, both attributable) and closing it properly needs a
/// conditional append the event store does not offer. Worth knowing before
/// someone assumes exactly-once here.
pub async fn decide(
    db: &Database,
    config: &ResolvedConfig,
    writer: &EventWriter,
    proposal_id: &str,
    decision: ProposalDecision,
    reason: Option<String>,
) -> Result<Proposal, DecideError> {
    let proposal = get(db, proposal_id)
        .await?
        .ok_or_else(|| DecideError::NotFound(proposal_id.to_string()))?;

    if !proposal.is_pending() {
        return Err(DecideError::AlreadyDecided(proposal_id.to_string()));
    }

    let mut events = Vec::new();
    if decision == ProposalDecision::Approved {
        let action: &ActionType = actions::lookup(config, &proposal.action)
            .ok_or_else(|| DecideError::UnknownAction(proposal.action.clone()))?;
        // Re-validated against the live declaration, deliberately. A proposal may
        // be approved long after it was made, by a build whose action has moved;
        // carrying out arguments that no longer validate is worse than refusing
        // and telling the user why.
        events = actions::build_events(action, &proposal.args, writer.device_id())
            .map_err(DecideError::Invalid)?;
    }

    // ⚠️ Ids are minted **here**, not left to the store. The marker has to name
    // the events the approval authored, and those events have to go out in the
    // same batch as the marker — so the id cannot be something the append tells
    // us afterwards. `EventStore::append` honours a supplied id and only invents
    // one when it is absent, which is what makes this safe rather than clever.
    let applied_event_ids: Vec<String> = events
        .iter_mut()
        .map(|e| {
            let id = e.id.get_or_insert_with(|| ulid::Ulid::new().to_string());
            id.clone()
        })
        .collect();

    let marker_payload = AssistantProposalDecidedPayload {
        proposal_id: proposal_id.to_string(),
        decision: decision.to_string(),
        applied_event_ids,
        reason,
    };
    events.push(crate::events::NewEvent {
        id: None,
        event_type: EventType::AssistantProposalDecided.to_string(),
        aggregate_id: proposal_id.to_string(),
        timestamp: Utc::now(),
        device_id: writer.device_id().to_string(),
        payload: serde_json::to_value(&marker_payload)
            .map_err(|e| DecideError::Invalid(e.to_string()))?,
    });

    writer.append_batch(events).await?;

    get(db, proposal_id)
        .await?
        .ok_or_else(|| DecideError::NotFound(proposal_id.to_string()))
}

/// The timestamp helper the inbox uses to age a pending proposal.
pub fn parse_created_at(proposal: &Proposal) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&proposal.created_at)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::config::{ConfigMap, ConfigValue, Feature};
    use crate::events::{
        AssistantProposalMadePayload, BeliefsProjection, EventStore, NewEvent, NotesProjection,
        ProjectionRunner, RoutinesProjection, SurrealEventStore,
    };

    async fn harness(features_off: &[Feature]) -> (Database, ResolvedConfig, EventWriter) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inbox.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);

        let mut global = ConfigMap::new();
        for f in features_off {
            global.insert(f.key(), ConfigValue::Bool(false));
        }
        let config = ResolvedConfig::new(global, ConfigMap::new());

        let store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new(db.clone()));
        let projections = ProjectionRunner::new(
            db.clone(),
            vec![
                Box::new(crate::events::AssistantProjection),
                Box::new(NotesProjection),
                Box::new(BeliefsProjection),
                Box::new(RoutinesProjection),
            ],
        );
        projections.init_all().await.expect("init projections");
        let writer = EventWriter::from_config(store, projections, &config, "phone");
        (db, config, writer)
    }

    /// Put a proposal in the inbox the way the agent does.
    async fn propose(writer: &EventWriter, proposal_id: &str, args: serde_json::Value) {
        let payload = AssistantProposalMadePayload {
            proposal_id: proposal_id.into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "note.create".into(),
            args,
            rationale: "You said you would forget.".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .expect("propose");
    }

    fn good_args() -> serde_json::Value {
        serde_json::json!({ "title": "Renew passport", "body": "Expires May." })
    }

    async fn note_titles(db: &Database) -> Vec<String> {
        let mut resp = db
            .query("SELECT title FROM generic_notes")
            .await
            .unwrap()
            .check()
            .unwrap();
        resp.take("title").unwrap_or_default()
    }

    #[tokio::test]
    async fn a_proposal_waits_in_the_inbox_until_it_is_decided() {
        let (db, _config, writer) = harness(&[]).await;
        propose(&writer, "p1", good_args()).await;

        let waiting = pending(&db).await.unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].action, "note.create");
        assert!(waiting[0].is_pending());
        assert!(
            note_titles(&db).await.is_empty(),
            "proposing must not create the note"
        );
    }

    #[tokio::test]
    async fn approving_applies_the_action_and_clears_the_inbox() {
        let (db, config, writer) = harness(&[]).await;
        propose(&writer, "p1", good_args()).await;

        let decided = decide(
            &db,
            &config,
            &writer,
            "p1",
            ProposalDecision::Approved,
            None,
        )
        .await
        .expect("approve");

        assert_eq!(decided.decision.as_deref(), Some("approved"));
        assert_eq!(note_titles(&db).await, vec!["Renew passport".to_string()]);
        assert!(pending(&db).await.unwrap().is_empty());
    }

    /// The marker names what approving actually did — the audit link in the
    /// direction a person asks for it.
    #[tokio::test]
    async fn the_decision_records_the_events_it_authored() {
        let (db, config, writer) = harness(&[]).await;
        propose(&writer, "p1", good_args()).await;
        decide(
            &db,
            &config,
            &writer,
            "p1",
            ProposalDecision::Approved,
            None,
        )
        .await
        .unwrap();

        let mut resp = db
            .query("SELECT applied_event_ids FROM type::record('assistant_proposals', 'p1')")
            .await
            .unwrap()
            .check()
            .unwrap();
        let applied: Vec<Option<Vec<String>>> = resp.take("applied_event_ids").unwrap_or_default();
        let ids = applied.first().cloned().flatten().unwrap_or_default();
        assert_eq!(ids.len(), 1, "one note event");

        let mut resp = db
            .query("SELECT event_type FROM type::record('events', $id)")
            .bind(("id", ids[0].clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let kinds: Vec<String> = resp.take("event_type").unwrap_or_default();
        assert_eq!(
            kinds.first().map(String::as_str),
            Some("generic_note_created"),
            "the recorded id must name the event the approval actually authored"
        );
    }

    #[tokio::test]
    async fn rejecting_changes_nothing_but_still_leaves_the_inbox() {
        let (db, config, writer) = harness(&[]).await;
        propose(&writer, "p1", good_args()).await;

        let decided = decide(
            &db,
            &config,
            &writer,
            "p1",
            ProposalDecision::Rejected,
            Some("I already did it".into()),
        )
        .await
        .expect("reject");

        assert_eq!(decided.decision.as_deref(), Some("rejected"));
        assert!(note_titles(&db).await.is_empty());
        assert!(pending(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn deciding_twice_is_refused_rather_than_applied_twice() {
        let (db, config, writer) = harness(&[]).await;
        propose(&writer, "p1", good_args()).await;
        decide(
            &db,
            &config,
            &writer,
            "p1",
            ProposalDecision::Approved,
            None,
        )
        .await
        .unwrap();

        let again = decide(
            &db,
            &config,
            &writer,
            "p1",
            ProposalDecision::Approved,
            None,
        )
        .await;
        assert!(
            matches!(again, Err(DecideError::AlreadyDecided(_))),
            "{again:?}"
        );
        assert_eq!(
            note_titles(&db).await.len(),
            1,
            "a second approval must not create a second note"
        );
    }

    #[tokio::test]
    async fn deciding_something_that_does_not_exist_says_so() {
        let (db, config, writer) = harness(&[]).await;
        let out = decide(
            &db,
            &config,
            &writer,
            "nope",
            ProposalDecision::Approved,
            None,
        )
        .await;
        assert!(matches!(out, Err(DecideError::NotFound(_))), "{out:?}");
    }

    /// ⚠️ A proposal may be approved long after it was made. Arguments that no
    /// longer validate are refused with a reason rather than carried out, because
    /// a half-understood write is worse than a refusal the user can read.
    #[tokio::test]
    async fn arguments_that_no_longer_validate_are_refused_at_approval() {
        let (db, config, writer) = harness(&[]).await;
        propose(
            &writer,
            "p1",
            serde_json::json!({ "title": "no body here" }),
        )
        .await;

        let out = decide(
            &db,
            &config,
            &writer,
            "p1",
            ProposalDecision::Approved,
            None,
        )
        .await;
        assert!(matches!(out, Err(DecideError::Invalid(_))), "{out:?}");
        assert!(note_titles(&db).await.is_empty());
        assert_eq!(
            pending(&db).await.unwrap().len(),
            1,
            "a refused approval leaves it pending, not silently consumed"
        );
    }

    /// ⚠️ The reason `AssistantProposalDecided` is ungated. Switching the
    /// assistant off must not trap a pending proposal in the inbox forever —
    /// `AutoImportBatchDismissed` does exactly that today, and this is the wart
    /// deliberately not copied.
    #[tokio::test]
    async fn a_pending_proposal_can_still_be_rejected_with_the_assistant_switched_off() {
        let (db, _config, writer) = harness(&[]).await;
        propose(&writer, "p1", good_args()).await;

        // Now the user turns the assistant off. Same database, a writer whose
        // feature snapshot no longer includes it — the boot state of the next
        // launch.
        let store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new(db.clone()));
        let projections = ProjectionRunner::new(
            db.clone(),
            vec![
                Box::new(crate::events::AssistantProjection),
                Box::new(NotesProjection),
                Box::new(BeliefsProjection),
                Box::new(RoutinesProjection),
            ],
        );
        let mut global = ConfigMap::new();
        global.insert(Feature::Llm.key(), ConfigValue::Bool(false));
        let off = ResolvedConfig::new(global, ConfigMap::new());
        let writer_off = EventWriter::from_config(store, projections, &off, "phone");

        decide(
            &db,
            &off,
            &writer_off,
            "p1",
            ProposalDecision::Rejected,
            None,
        )
        .await
        .expect("a pending proposal must stay dismissible");
        assert!(pending(&db).await.unwrap().is_empty());
    }

    /// The mirror of the case above: the guard sits on the *effect*. Approving a
    /// note action with Notes switched off is refused by `EventWriter`, which is
    /// where that decision belongs.
    #[tokio::test]
    async fn approving_is_still_refused_when_the_actions_own_feature_is_off() {
        let (db, _config, writer) = harness(&[]).await;
        propose(&writer, "p1", good_args()).await;

        let off = {
            let mut global = ConfigMap::new();
            global.insert(Feature::Notes.key(), ConfigValue::Bool(false));
            ResolvedConfig::new(global, ConfigMap::new())
        };
        let out = decide(&db, &off, &writer, "p1", ProposalDecision::Approved, None).await;
        // The action is invisible with its feature off, so it never reaches the
        // writer's guard — refused one step earlier, and for a reason the user
        // can act on.
        assert!(matches!(out, Err(DecideError::UnknownAction(_))), "{out:?}");
        assert!(note_titles(&db).await.is_empty());
    }

    /// Phase E through the Phase D gate: a belief exists only because a person
    /// accepted it, and it keeps the evidence the run actually read.
    #[tokio::test]
    async fn approving_a_belief_records_it_with_its_evidence() {
        let (db, config, writer) = harness(&[]).await;

        let payload = AssistantProposalMadePayload {
            proposal_id: "p-belief".into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "belief.record".into(),
            args: serde_json::json!({
                "statement": "You underestimate how long admin tasks take.",
                "confidence": "medium",
                "evidence": [{ "kind": "journal", "id": "2026-08-14" }],
            }),
            rationale: "You asked what patterns I could see.".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .expect("propose");

        decide(
            &db,
            &config,
            &writer,
            "p-belief",
            ProposalDecision::Approved,
            None,
        )
        .await
        .expect("approve");

        let mut resp = db
            .query("SELECT statement, confidence, evidence FROM beliefs")
            .await
            .unwrap()
            .check()
            .unwrap();
        let statements: Vec<Option<String>> = resp.take("statement").unwrap_or_default();
        let confidences: Vec<Option<String>> = resp.take("confidence").unwrap_or_default();
        assert_eq!(statements.len(), 1, "exactly one belief");
        assert!(
            statements[0].clone().unwrap_or_default().contains("admin"),
            "{statements:?}"
        );
        assert_eq!(confidences[0].clone(), Some("medium".into()));
    }

    /// ⚠️ Declining a belief must leave **nothing** behind. A rejected conclusion
    /// that still lands in the memory would make the gate decorative.
    #[tokio::test]
    async fn declining_a_belief_records_nothing() {
        let (db, config, writer) = harness(&[]).await;
        let payload = AssistantProposalMadePayload {
            proposal_id: "p-belief".into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "belief.record".into(),
            args: serde_json::json!({ "statement": "You are disorganised.", "confidence": "low" }),
            rationale: "You asked.".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .unwrap();

        decide(
            &db,
            &config,
            &writer,
            "p-belief",
            ProposalDecision::Rejected,
            Some("that is not true".into()),
        )
        .await
        .expect("reject");

        let mut resp = db
            .query("SELECT belief_id FROM beliefs")
            .await
            .unwrap()
            .check()
            .unwrap();
        let ids: Vec<String> = resp.take("belief_id").unwrap_or_default();
        assert!(ids.is_empty(), "a declined belief must not exist: {ids:?}");
    }

    /// The widened action set through the same gate, end to end: approving a
    /// routine proposal has to reach the routines projection, not just the log.
    #[tokio::test]
    async fn approving_a_routine_creates_it() {
        let (db, config, writer) = harness(&[]).await;

        let payload = AssistantProposalMadePayload {
            proposal_id: "p-routine".into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "routine.create".into(),
            args: serde_json::json!({ "name": "Morning", "frequency": "daily" }),
            rationale: "You said you wanted one.".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .unwrap();

        decide(
            &db,
            &config,
            &writer,
            "p-routine",
            ProposalDecision::Approved,
            None,
        )
        .await
        .expect("approve");

        let mut resp = db
            .query("SELECT name, frequency FROM routine_groups")
            .await
            .unwrap()
            .check()
            .unwrap();
        let names: Vec<String> = resp.take("name").unwrap_or_default();
        let freqs: Vec<String> = resp.take("frequency").unwrap_or_default();
        assert_eq!(names, vec!["Morning".to_string()]);
        assert_eq!(freqs, vec!["daily".to_string()]);
    }

    /// ⚠️ Re-validated at approval, so a frequency that was legal when proposed
    /// but is not now cannot slip through into the projection.
    #[tokio::test]
    async fn a_routine_proposal_with_a_bad_frequency_is_refused_at_approval() {
        let (db, config, writer) = harness(&[]).await;

        let payload = AssistantProposalMadePayload {
            proposal_id: "p-bad".into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "routine.create".into(),
            args: serde_json::json!({ "name": "Morning", "frequency": "fortnightly" }),
            rationale: "because".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .unwrap();

        let out = decide(
            &db,
            &config,
            &writer,
            "p-bad",
            ProposalDecision::Approved,
            None,
        )
        .await;
        assert!(matches!(out, Err(DecideError::Invalid(_))), "{out:?}");

        let mut resp = db
            .query("SELECT name FROM routine_groups")
            .await
            .unwrap()
            .check()
            .unwrap();
        let names: Vec<String> = resp.take("name").unwrap_or_default();
        assert!(names.is_empty(), "nothing should have been created");
    }

    /// ⚠️ The row id the projection builds is `{item_id}-{date}-done`, so this
    /// asserts the *date reached it intact* rather than merely that a row
    /// exists. A completion landing on the wrong day is indistinguishable from
    /// one the user never made, and it would not be undone by ticking the item
    /// in the app either — that undo deletes a different key.
    #[tokio::test]
    async fn approving_a_completion_ticks_the_day_it_named() {
        let (db, config, writer) = harness(&[]).await;

        let payload = AssistantProposalMadePayload {
            proposal_id: "p-done".into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "routine.complete".into(),
            args: serde_json::json!({
                "item_ids": ["item-1"],
                "group_id": "group-1",
                "date": "2026-08-14",
                "evidence": [{ "kind": "journal", "id": "2026-08-14" }],
            }),
            rationale: "You wrote on the 14th that you ran before work.".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .unwrap();

        decide(
            &db,
            &config,
            &writer,
            "p-done",
            ProposalDecision::Approved,
            None,
        )
        .await
        .expect("approve");

        let mut resp = db
            .query("SELECT item_id, date, skipped FROM routine_completions")
            .await
            .unwrap()
            .check()
            .unwrap();
        let items: Vec<String> = resp.take("item_id").unwrap_or_default();
        let dates: Vec<String> = resp.take("date").unwrap_or_default();
        let skipped: Vec<bool> = resp.take("skipped").unwrap_or_default();
        assert_eq!(items, vec!["item-1".to_string()]);
        assert_eq!(dates, vec!["2026-08-14".to_string()], "wrong day ticked");
        assert_eq!(skipped, vec![false]);
    }

    /// ⚠️ **The only test that proves a batch is worth anything.** `build_events`
    /// returning three events is not the claim; three *rows* is. Approval writes
    /// through the same `EventWriter` and the same projection fold a single tick
    /// uses, so the way this breaks is a batch that appends all three events and
    /// projects one row — which no unit test on `build_events` can see.
    #[tokio::test]
    async fn approving_a_batch_ticks_every_item_it_named() {
        let (db, config, writer) = harness(&[]).await;

        let payload = AssistantProposalMadePayload {
            proposal_id: "p-batch".into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "routine.complete".into(),
            args: serde_json::json!({
                "item_ids": ["item-1", "item-2", "item-3"],
                "group_id": "group-1",
                "date": "2026-08-14",
                "evidence": [{ "kind": "journal", "id": "2026-08-14" }],
            }),
            rationale: "You wrote that you stretched, ran and showered before work.".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .unwrap();

        decide(
            &db,
            &config,
            &writer,
            "p-batch",
            ProposalDecision::Approved,
            None,
        )
        .await
        .expect("approve");

        let mut resp = db
            .query("SELECT item_id, date FROM routine_completions ORDER BY item_id")
            .await
            .unwrap()
            .check()
            .unwrap();
        let items: Vec<String> = resp.take("item_id").unwrap_or_default();
        let dates: Vec<String> = resp.take("date").unwrap_or_default();
        assert_eq!(
            items,
            vec![
                "item-1".to_string(),
                "item-2".to_string(),
                "item-3".to_string()
            ],
            "one approval must tick every item it named"
        );
        assert!(
            dates.iter().all(|d| d == "2026-08-14"),
            "every row shares the proposed day, got {dates:?}"
        );
    }

    /// A skip is a decision the user made, and the reason is the whole reason it
    /// is not just an untouched item — so it has to survive to the row.
    #[tokio::test]
    async fn approving_a_skip_keeps_the_reason() {
        let (db, config, writer) = harness(&[]).await;

        let payload = AssistantProposalMadePayload {
            proposal_id: "p-skip".into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "routine.skip".into(),
            args: serde_json::json!({
                "item_ids": ["item-1"],
                "group_id": "group-1",
                "date": "2026-08-14",
                "reason": "Travelling.",
            }),
            rationale: "You said you were away that week.".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .unwrap();

        decide(
            &db,
            &config,
            &writer,
            "p-skip",
            ProposalDecision::Approved,
            None,
        )
        .await
        .expect("approve");

        let mut resp = db
            .query("SELECT skipped, reason FROM routine_completions")
            .await
            .unwrap()
            .check()
            .unwrap();
        let skipped: Vec<bool> = resp.take("skipped").unwrap_or_default();
        let reasons: Vec<Option<String>> = resp.take("reason").unwrap_or_default();
        assert_eq!(skipped, vec![true]);
        assert_eq!(reasons, vec![Some("Travelling.".to_string())]);
    }

    /// ⚠️ Re-validated at approval, and the future is the one date no record can
    /// evidence. A proposal made yesterday for "tomorrow" must not become a tick
    /// simply because it sat in the inbox.
    #[tokio::test]
    async fn a_completion_dated_in_the_future_is_refused_at_approval() {
        let (db, config, writer) = harness(&[]).await;

        let tomorrow = (chrono::Utc::now().date_naive() + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let payload = AssistantProposalMadePayload {
            proposal_id: "p-future".into(),
            thread_id: "t1".into(),
            message_id: "m2".into(),
            action: "routine.complete".into(),
            args: serde_json::json!({
                "item_ids": ["item-1"],
                "group_id": "group-1",
                "date": tomorrow,
            }),
            rationale: "because".into(),
            reversible: true,
        };
        writer
            .append_new(NewEvent::assistant_proposal_made("agent", &payload).unwrap())
            .await
            .unwrap();

        let out = decide(
            &db,
            &config,
            &writer,
            "p-future",
            ProposalDecision::Approved,
            None,
        )
        .await;
        assert!(matches!(out, Err(DecideError::Invalid(_))), "{out:?}");

        let mut resp = db
            .query("SELECT item_id FROM routine_completions")
            .await
            .unwrap()
            .check()
            .unwrap();
        let items: Vec<String> = resp.take("item_id").unwrap_or_default();
        assert!(items.is_empty(), "nothing should have been ticked");
    }

    #[tokio::test]
    async fn a_threads_proposals_are_findable_from_the_conversation() {
        let (db, _config, writer) = harness(&[]).await;
        propose(&writer, "p1", good_args()).await;
        assert_eq!(for_thread(&db, "t1").await.unwrap().len(), 1);
        assert!(for_thread(&db, "t-other").await.unwrap().is_empty());
    }
}
