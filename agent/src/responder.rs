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

use omni_me_core::assistant::conversation::{PendingQuestion, pending_questions};
use omni_me_core::assistant::{Retrievers, Session, answer_payload, skipped_payload};
use omni_me_core::config::{Feature, ResolvedConfig};
use omni_me_core::db::Database;
use omni_me_core::events::{AnswerStop, EventWriter, NewEvent};
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
                _ = tokio::time::sleep(SWEEP_FLOOR) => self.sweep().await,
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("shutting down");
                    return Stopped::Interrupted;
                }
            }
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
        self.append(&payload.thread_id, || {
            NewEvent::assistant_answer_given(self.writer.device_id(), &payload)
        })
        .await;
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
