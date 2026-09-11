//! The answering loop: find questions that arrived, answer them, record what
//! happened.
//!
//! This is what makes the agent an *interface* rather than a resident process
//! with nothing to do. A question authored on the phone reaches this loop the
//! ordinary way — through sync, into the local database, folded by
//! `AssistantProjection` — and the answer goes back the same way. There is no
//! second channel, and deliberately so: `surrealkv` holds an exclusive lock on
//! its directory, so anything that wanted to hand this process a question
//! out-of-band would have to open a database it cannot open.
//!
//! ## Why it runs inline rather than in a spawned task
//!
//! `Retrievers` borrows the embedder and the reranker, both of which live in
//! `main::run`'s frame. A spawned task would need them `'static`, which means
//! either leaking them or wrapping both in `Arc` for no reason other than to
//! satisfy the spawn. Running the loop where the values already live keeps the
//! ownership honest, and there is nothing else for the main task to do — it used
//! to block on `ctrl_c`.

use std::time::Duration;

use omni_me_core::assistant::check_in::{self, CheckInPolicy};
use omni_me_core::assistant::conversation::{PendingQuestion, pending_questions};
use omni_me_core::assistant::{
    Retrievers, Session, answer_payload, inbox, promotion, proposals, skipped_payload,
};
use omni_me_core::config::{Feature, ResolvedConfig};
use omni_me_core::db::Database;
use omni_me_core::events::{
    AnswerStop, AssistantProposalMadePayload, AssistantQuestionAskedPayload, EventWriter, NewEvent,
    ProposalDecision,
};
use omni_me_core::llm::LlmClient;
use omni_me_core::sync::{PullEvent, PullScheduler};
use tokio::sync::broadcast::error::RecvError;

/// The floor under the pull-driven trigger.
///
/// ⚠️ Not redundant with subscribing to [`PullEvent`]. A broadcast receiver that
/// lags drops messages, a pull that fails emits nothing to act on, and a question
/// that landed during a restart was never announced at all. The pusher's missing
/// interval fallback is this codebase's own precedent for what a single edge
/// trigger costs: there, an append that skipped the nudge did not sync slowly, it
/// did not sync at all.
const SWEEP_FLOOR: Duration = Duration::from_secs(30);

/// How the loop ended.
pub enum Stopped {
    /// SIGINT / Ctrl-C.
    Interrupted,
}

/// Answer questions until interrupted.
pub struct Responder<'a> {
    pub db: &'a Database,
    pub config: &'a ResolvedConfig,
    pub llm: &'a dyn LlmClient,
    pub writer: &'a EventWriter,
    pub retrievers: Retrievers<'a>,
    /// Questions older than this are closed without a model call. See
    /// [`Responder::adjudicate`].
    pub horizon: Duration,
}

impl Responder<'_> {
    pub async fn run(&self, pulls: &PullScheduler) -> Stopped {
        let mut outcomes = pulls.subscribe();
        // Sweep once before waiting on anything: a question that arrived while
        // this process was down has no event left to announce it.
        self.sweep().await;

        loop {
            tokio::select! {
                received = outcomes.recv() => match received {
                    // Only `Applied` can have produced a new question — `Applying`
                    // fires before the fold, so the projection has not run yet.
                    Ok(PullEvent::Applied { .. }) => self.sweep().await,
                    Ok(_) => {}
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(missed = n, "pull events lagged; sweeping anyway");
                        self.sweep().await;
                    }
                    Err(RecvError::Closed) => {
                        tracing::warn!("pull scheduler stopped; falling back to the interval");
                        // Not fatal, and not a reason to stop answering: the
                        // sweep reads the local database, which still holds
                        // whatever the last pull landed.
                        tokio::time::sleep(SWEEP_FLOOR).await;
                        self.sweep().await;
                    }
                },
                _ = tokio::time::sleep(SWEEP_FLOOR) => {
                    // Checked on the same tick as the sweep rather than on a timer
                    // of its own. `is_due` is a date comparison, so asking it
                    // every 30s costs one query and removes a second clock that
                    // could disagree with this one about what day it is.
                    self.maybe_check_in().await;
                    self.sweep().await;
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("shutting down");
                    return Stopped::Interrupted;
                }
            }
        }
    }

    /// Raise the scheduled check-in, if one is due.
    ///
    /// ⚠️ This authors an **ordinary question** and stops. The answering sweep
    /// picks it up like any other, so a check-in inherits the whole path: the
    /// same verbs, the same journal refusal, the same proposal gate, the same
    /// inbox, the same sync. Nothing here decides what the assistant may do — it
    /// only decides that it may be asked.
    async fn maybe_check_in(&self) {
        let policy = CheckInPolicy::from_config(self.config);
        if !policy.enabled {
            return;
        }
        // Read fresh each tick rather than cached: the question is the record of
        // the last run, and it can arrive from another agent over sync.
        let last = match check_in::last_check_in(self.db).await {
            Ok(last) => last,
            Err(e) => {
                // Not fatal, but **not** treated as "never ran" either. Falling
                // back to `None` on a read error would raise a check-in on every
                // tick for as long as the error lasted.
                tracing::warn!(error = %e, "could not read the last check-in; skipping this tick");
                return;
            }
        };
        if !check_in::is_due_now(&policy, last) {
            return;
        }

        let thread_id = ulid::Ulid::new().to_string();
        let message_id = ulid::Ulid::new().to_string();
        let payload = AssistantQuestionAskedPayload {
            thread_id: thread_id.clone(),
            message_id: message_id.clone(),
            text: policy.prompt.clone(),
            title: Some("Daily check-in".to_string()),
            scheduled: true,
        };

        tracing::info!(thread = %thread_id, hour = policy.hour, "raising the daily check-in");
        match NewEvent::assistant_question_asked(self.writer.device_id(), &payload) {
            Ok(event) => match self.writer.append_new(event).await {
                Ok(_) => {}
                // A refusal here is the feature guard doing its job — `feature.llm`
                // went off between the config snapshot and now.
                Err(e) => tracing::warn!(error = %e, "check-in refused"),
            },
            Err(e) => tracing::error!(error = %e, "could not build the check-in question"),
        }
    }

    /// Answer everything waiting, oldest first.
    ///
    /// ⚠️ **Strictly one at a time.** One question is several model calls, and on
    /// a rate-capped endpoint concurrency turns a backlog into a wall of 429s
    /// that the scorecard would read as the model failing. `LLM_MIN_INTERVAL_MS`
    /// exists for the same reason and does not cover this one: it spaces requests
    /// within a run, not runs against each other.
    async fn sweep(&self) {
        // Checked per sweep rather than once at startup: the switch is shared
        // config and arrives over sync like anything else, so an agent that read
        // it once would keep answering after the user turned the feature off on
        // their phone. The writer would refuse the append — but only after the
        // model call had been paid for.
        if !self.config.enabled(Feature::Llm) {
            return;
        }

        let pending = match pending_questions(self.db).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "could not read pending questions");
                return;
            }
        };
        if pending.is_empty() {
            return;
        }
        tracing::info!(waiting = pending.len(), "questions waiting");

        for question in pending {
            self.adjudicate(question).await;
        }
    }

    /// Decide one question's fate, and record it either way.
    ///
    /// Every path here ends in an appended answer event. A question with no
    /// terminal event is indistinguishable from one still being worked on, so
    /// dropping one silently leaves a spinner on the user's phone forever — which
    /// is the failure this whole event pair is shaped to prevent.
    async fn adjudicate(&self, pending: PendingQuestion) {
        let age = pending.age();
        let PendingQuestion {
            question, history, ..
        } = pending;
        let message_id = ulid::Ulid::new().to_string();

        // ⚠️ The horizon is a spend control, not tidiness. An agent that was down
        // for a day comes back to every question asked in the meantime — most of
        // which the user has since re-asked, given up on, or answered themselves.
        // Paying for all of them is the default behaviour, and it is the wrong one.
        if age > self.horizon {
            tracing::info!(
                thread = %question.thread_id,
                message_id = %question.message_id,
                age_secs = age.as_secs(),
                "too old to answer; closing it as stale"
            );
            let payload = skipped_payload(
                &question,
                &message_id,
                AnswerStop::Stale,
                format!(
                    "asked {}m ago, past the {}m answering horizon",
                    age.as_secs() / 60,
                    self.horizon.as_secs() / 60
                ),
            );
            self.append(&payload.thread_id, || {
                NewEvent::assistant_answer_given(self.writer.device_id(), &payload)
            })
            .await;
            return;
        }

        let session = match Session::new(self.db, self.config, self.llm) {
            Ok(s) => s,
            Err(e) => {
                // A session that cannot even be built is a configuration fault,
                // not a model failure — but the question still gets closed, with
                // the reason attached, rather than left hanging.
                tracing::warn!(error = %e, "cannot answer; closing the question");
                let payload =
                    skipped_payload(&question, &message_id, AnswerStop::Failed, e.to_string());
                self.append(&payload.thread_id, || {
                    NewEvent::assistant_answer_given(self.writer.device_id(), &payload)
                })
                .await;
                return;
            }
        };
        let session = match self.retrievers.semantic {
            Some(s) => session.with_semantic_search(s),
            None => session,
        };
        let session = match self.retrievers.reranker {
            Some(r) => session.with_reranker(r),
            None => session,
        };

        tracing::info!(
            thread = %question.thread_id,
            message_id = %question.message_id,
            prior_turns = history.len(),
            "answering"
        );
        let outcome = session.ask_with_history(&history, &question.text).await;
        tracing::info!(
            thread = %question.thread_id,
            verbs = ?outcome.verbs(),
            stopped = ?outcome.stopped,
            elapsed_ms = outcome.elapsed.as_millis(),
            prompt_tokens = outcome.usage.prompt_tokens,
            completion_tokens = outcome.usage.completion_tokens,
            reasoning_tokens = outcome.usage.reasoning_tokens,
            "answered"
        );

        let payload = answer_payload(
            &question,
            &message_id,
            Some(self.llm.model_name().to_string()),
            &outcome,
        );

        // ⚠️ The answer and the proposals it made go out in **one batch**. Split
        // apart, an answer that lands while its proposals do not reads as "I have
        // suggested creating that note" over an empty inbox — the user is told
        // about a decision they cannot find. Derived from the trace rather than
        // from the prose, so nothing here can invent one; see
        // `assistant::answer::proposals`.
        let proposed = proposals(self.config, &outcome);
        if !proposed.is_empty() {
            tracing::info!(
                thread = %question.thread_id,
                proposals = proposed.len(),
                actions = ?proposed.iter().map(|p| p.action.as_str()).collect::<Vec<_>>(),
                "proposals made; waiting on the user"
            );
        }

        let mut events = Vec::with_capacity(1 + proposed.len());
        let mut proposed_ids: Vec<String> = Vec::with_capacity(proposed.len());
        match NewEvent::assistant_answer_given(self.writer.device_id(), &payload) {
            Ok(e) => events.push(e),
            Err(e) => {
                tracing::error!(thread = %payload.thread_id, error = %e, "could not build the answer event");
                return;
            }
        }
        for action in &proposed {
            let proposal_id = ulid::Ulid::new().to_string();
            let made = AssistantProposalMadePayload {
                proposal_id: proposal_id.clone(),
                thread_id: question.thread_id.clone(),
                message_id: message_id.clone(),
                action: action.action.clone(),
                args: action.args.clone(),
                rationale: action.rationale.clone(),
                reversible: action.reversible,
            };
            match NewEvent::assistant_proposal_made(self.writer.device_id(), &made) {
                Ok(e) => {
                    events.push(e);
                    proposed_ids.push(proposal_id);
                }
                Err(e) => {
                    // Drop the proposal, keep the answer. The reverse — refusing
                    // to record the answer — would leave the question pending and
                    // re-answered on every sweep, which costs money and fixes
                    // nothing.
                    tracing::error!(
                        thread = %payload.thread_id,
                        action = %action.action,
                        error = %e,
                        "could not build a proposal event; recording the answer without it"
                    );
                }
            }
        }
        self.append_batch(&payload.thread_id, events).await;

        // ⚠️ Autonomy is applied **after** the batch has landed, never folded into
        // it. A granted action still produces a real proposal first, so the log
        // reads identically whether a person approved it or a standing grant did
        // — same proposal, same decision event, same audit trail. Collapsing the
        // two into a direct write would save an event and lose the only record
        // that says what was carried out and under whose authority.
        for proposal_id in proposed_ids {
            self.maybe_auto_approve(&proposal_id).await;
        }
    }

    /// Carry out a proposal the user has already granted standing permission for.
    ///
    /// Nothing here re-implements approval: it calls the same `inbox::decide` the
    /// phone does, so the reversibility rule, the argument re-validation and the
    /// one-batch atomicity all apply unchanged.
    async fn maybe_auto_approve(&self, proposal_id: &str) {
        let Ok(Some(proposal)) = inbox::get(self.db, proposal_id).await else {
            return;
        };
        match promotion::is_autonomous(self.db, self.config, &proposal.action).await {
            Ok(false) => return,
            Ok(true) => {}
            Err(e) => {
                // Fail closed: an unreadable grant leaves the proposal waiting for
                // a person, which is the safe direction of every doubt here.
                tracing::warn!(error = %e, "could not read autonomy; leaving it for approval");
                return;
            }
        }

        tracing::info!(
            proposal_id = %proposal_id,
            action = %proposal.action,
            "carrying out a granted action without asking"
        );
        if let Err(e) = inbox::decide(
            self.db,
            self.config,
            self.writer,
            proposal_id,
            ProposalDecision::Approved,
            Some("Carried out under a standing grant.".to_string()),
        )
        .await
        {
            // Left pending rather than retried. The user sees it in the inbox and
            // can decide by hand, which beats a loop against a failing write.
            tracing::warn!(error = %e, "could not carry it out; leaving it for approval");
        }
    }

    /// Append one run's events, reporting rather than propagating a failure.
    ///
    /// Nothing upstream can do anything useful with an error here: the model call
    /// is already paid for, and refusing to keep running would take the agent down
    /// over one unwritable answer. A refusal is also a legitimate outcome —
    /// `feature.llm` going off between the sweep's check and this append is
    /// exactly the guard doing its job.
    async fn append_batch(&self, thread_id: &str, events: Vec<NewEvent>) {
        let count = events.len();
        match self.writer.append_batch(events).await {
            Ok(stored) => tracing::info!(
                thread = %thread_id,
                events = count,
                first_event_id = stored.first().map(|e| e.id.as_str()).unwrap_or("-"),
                "answer appended; the pusher should carry it shortly"
            ),
            Err(e) => tracing::warn!(thread = %thread_id, error = %e, "answer refused"),
        }
    }

    /// Append one answer, reporting rather than propagating a failure.
    ///
    /// Nothing upstream can do anything useful with an error here: the model call
    /// is already paid for, and refusing to keep running would take the agent
    /// down over one unwritable answer. A refusal is also a legitimate outcome —
    /// `feature.llm` going off between the sweep's check and this append is
    /// exactly the guard doing its job.
    async fn append(
        &self,
        thread_id: &str,
        build: impl FnOnce() -> Result<NewEvent, serde_json::Error>,
    ) {
        let event = match build() {
            Ok(e) => e,
            Err(e) => {
                tracing::error!(thread = %thread_id, error = %e, "could not build the answer event");
                return;
            }
        };
        match self.writer.append_new(event).await {
            Ok(stored) => tracing::info!(
                thread = %thread_id,
                event_id = %stored.id,
                "answer appended; the pusher should carry it shortly"
            ),
            Err(e) => tracing::warn!(thread = %thread_id, error = %e, "answer refused"),
        }
    }
}
