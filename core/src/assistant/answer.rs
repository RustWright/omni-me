//! Turning one run's [`Outcome`] into the event that records it.
//!
//! This lives in `core` rather than in the agent binary because it is the only
//! place that decides what a conversation *remembers* — which is a product
//! question, not a deployment one — and because it is the half worth testing
//! without a model, a database or a network.
//!
//! What it deliberately does not carry into the log: verb arguments, tool
//! results, and the model's intermediate reasoning. All three are re-derivable
//! from the agent's own logs, all three can be long, and every one of them would
//! sync to the phone.

use crate::events::{
    AnswerStop, AnswerUsage, AssistantAnswerGivenPayload, AssistantQuestionAskedPayload, RecordRef,
};
use crate::llm::chat::Usage;

use super::session::{Outcome, StopReason};

impl From<&Usage> for AnswerUsage {
    fn from(u: &Usage) -> Self {
        Self {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            reasoning_tokens: u.reasoning_tokens,
        }
    }
}

/// The records a run actually opened, in the order it opened them.
///
/// Read off the `read` calls in the trace rather than out of the model's prose:
/// these are records it demonstrably held, so a citation built from them cannot
/// be invented. A repeated `read` appears once — the loop already flags repeats,
/// and a citation list is a set of sources, not a call log.
///
/// `title` is left `None` on purpose. The trace records what was asked for, not
/// what came back, and the client reading this event has the record tables in
/// front of it — resolving the title there shows the record's *current* title
/// rather than a copy frozen at answer time.
pub fn records_read(outcome: &Outcome) -> Vec<RecordRef> {
    let mut seen = Vec::new();
    for turn in &outcome.trace {
        if turn.verb.as_deref() != Some("read") {
            continue;
        }
        let (Some(kind), Some(id)) = (
            turn.arguments.get("type").and_then(|v| v.as_str()),
            turn.arguments.get("id").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        if seen
            .iter()
            .any(|r: &RecordRef| r.kind == kind && r.id == id)
        {
            continue;
        }
        seen.push(RecordRef {
            kind: kind.to_string(),
            id: id.to_string(),
            title: None,
        });
    }
    seen
}

/// Build the answer payload for one completed run.
///
/// `message_id` is minted by the caller so the agent can log it before the
/// append, which is what makes an append that fails traceable to a run.
pub fn answer_payload(
    question: &AssistantQuestionAskedPayload,
    message_id: impl Into<String>,
    model: Option<String>,
    outcome: &Outcome,
) -> AssistantAnswerGivenPayload {
    let (stopped, detail) = match &outcome.stopped {
        StopReason::Answered => (AnswerStop::Answered, None),
        StopReason::TurnBudget => (AnswerStop::TurnBudget, None),
        StopReason::Failed(why) => (AnswerStop::Failed, Some(why.clone())),
    };

    // Text only when the run actually answered. A client that rendered a
    // half-finished reply from a budget-exhausted run would be presenting the
    // model's last thought as its conclusion.
    let text = match stopped {
        AnswerStop::Answered => outcome.answer.clone(),
        _ => None,
    };

    AssistantAnswerGivenPayload {
        thread_id: question.thread_id.clone(),
        message_id: message_id.into(),
        in_reply_to: question.message_id.clone(),
        text,
        stopped: stopped.to_string(),
        detail,
        model,
        usage: AnswerUsage::from(&outcome.usage),
        elapsed_ms: outcome.elapsed.as_millis() as u64,
        verbs: outcome
            .verbs()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>(),
        records_read: records_read(outcome),
    }
}

/// The payload for a question the agent declined to send to a model.
///
/// Separate from [`answer_payload`] because there is no [`Outcome`] to describe:
/// nothing ran, nothing was spent. It exists so the question still gets a
/// terminal event — a question with none is indistinguishable from one still
/// being worked on, and the client would spin on it forever.
pub fn skipped_payload(
    question: &AssistantQuestionAskedPayload,
    message_id: impl Into<String>,
    stopped: AnswerStop,
    detail: impl Into<String>,
) -> AssistantAnswerGivenPayload {
    AssistantAnswerGivenPayload {
        thread_id: question.thread_id.clone(),
        message_id: message_id.into(),
        in_reply_to: question.message_id.clone(),
        text: None,
        stopped: stopped.to_string(),
        detail: Some(detail.into()),
        model: None,
        usage: AnswerUsage::default(),
        elapsed_ms: 0,
        verbs: Vec::new(),
        records_read: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::session::TurnRecord;
    use serde_json::json;
    use std::time::Duration;

    fn question() -> AssistantQuestionAskedPayload {
        AssistantQuestionAskedPayload {
            thread_id: "t1".into(),
            message_id: "m1".into(),
            text: "what did I write about rent?".into(),
            title: Some("rent".into()),
        }
    }

    fn turn(verb: Option<&str>, arguments: serde_json::Value) -> TurnRecord {
        TurnRecord {
            verb: verb.map(str::to_string),
            arguments,
            repeated: false,
            usage: Usage::default(),
            latency: Duration::from_millis(100),
        }
    }

    fn outcome(stopped: StopReason, trace: Vec<TurnRecord>) -> Outcome {
        Outcome {
            answer: Some("you noted on 12 Aug that…".into()),
            trace,
            usage: Usage {
                prompt_tokens: 900,
                completion_tokens: 40,
                reasoning_tokens: 700,
                total_tokens: 1640,
            },
            elapsed: Duration::from_millis(8400),
            stopped,
            off_schema: 0,
        }
    }

    #[test]
    fn an_answered_run_records_its_text_verbs_and_cost() {
        let o = outcome(
            StopReason::Answered,
            vec![
                turn(Some("search"), json!({"query": "rent"})),
                turn(Some("read"), json!({"type": "journal", "id": "2026-08-12"})),
                turn(None, json!(null)),
            ],
        );
        let p = answer_payload(&question(), "m2", Some("test-model".into()), &o);

        assert_eq!(p.thread_id, "t1");
        assert_eq!(p.in_reply_to, "m1");
        assert_eq!(p.stopped, "answered");
        assert_eq!(p.text.as_deref(), Some("you noted on 12 Aug that…"));
        assert_eq!(p.verbs, vec!["search".to_string(), "read".to_string()]);
        assert_eq!(p.elapsed_ms, 8400);
        assert_eq!(p.usage.reasoning_tokens, 700);
        assert_eq!(p.records_read.len(), 1);
        assert_eq!(p.records_read[0].kind, "journal");
        assert_eq!(p.records_read[0].id, "2026-08-12");
    }

    /// Reasoning tokens must not be folded into completion tokens. They were
    /// ~95% of output on the baseline model, so a combined figure misstates both
    /// the bill and where the time went.
    #[test]
    fn reasoning_tokens_stay_separate_from_completion_tokens() {
        let o = outcome(StopReason::Answered, vec![]);
        let p = answer_payload(&question(), "m2", None, &o);
        assert_eq!(p.usage.completion_tokens, 40);
        assert_eq!(p.usage.reasoning_tokens, 700);
    }

    /// A run that never reached a conclusion must not have its last words
    /// rendered as one.
    #[test]
    fn a_run_that_did_not_answer_carries_no_text() {
        for stopped in [
            StopReason::TurnBudget,
            StopReason::Failed("502 from upstream".into()),
        ] {
            let o = outcome(stopped.clone(), vec![]);
            let p = answer_payload(&question(), "m2", None, &o);
            assert!(
                p.text.is_none(),
                "{stopped:?} leaked an unfinished reply into `text`"
            );
        }
    }

    #[test]
    fn a_provider_failure_keeps_its_sentence_out_of_the_answer_body() {
        let o = outcome(StopReason::Failed("502 from upstream".into()), vec![]);
        let p = answer_payload(&question(), "m2", None, &o);
        assert_eq!(p.stopped, "failed");
        assert_eq!(p.detail.as_deref(), Some("502 from upstream"));
        assert!(p.text.is_none());
    }

    /// The loop can legitimately re-read the same record. A citation list is a
    /// set of sources, not a call log.
    #[test]
    fn a_record_read_twice_is_cited_once() {
        let o = outcome(
            StopReason::Answered,
            vec![
                turn(Some("read"), json!({"type": "note", "id": "01ABC"})),
                turn(Some("search"), json!({"query": "again"})),
                turn(Some("read"), json!({"type": "note", "id": "01ABC"})),
            ],
        );
        let p = answer_payload(&question(), "m2", None, &o);
        assert_eq!(p.records_read.len(), 1);
    }

    /// A malformed `read` must not become a citation to nothing. The model
    /// controls these arguments, so this is the injection-adjacent case: a
    /// citation is only ever as good as the identity behind it.
    #[test]
    fn a_read_without_a_usable_identity_is_not_cited() {
        let o = outcome(
            StopReason::Answered,
            vec![
                turn(Some("read"), json!({"type": "note"})),
                turn(Some("read"), json!("just a string")),
                turn(Some("read"), json!({"type": 7, "id": 9})),
            ],
        );
        let p = answer_payload(&question(), "m2", None, &o);
        assert!(p.records_read.is_empty());
    }

    #[test]
    fn a_skipped_question_still_gets_a_terminal_event() {
        let p = skipped_payload(&question(), "m2", AnswerStop::Stale, "older than 60m");
        assert_eq!(p.stopped, "stale");
        assert_eq!(p.in_reply_to, "m1");
        assert!(p.text.is_none());
        assert_eq!(p.usage.prompt_tokens, 0);
        assert_eq!(p.detail.as_deref(), Some("older than 60m"));
    }
}
