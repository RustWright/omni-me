//! `--bench-review`: seat B on its own job, the scheduled belief review.
//!
//! Seat B was chosen on seat A's retrieval cases, and its real work is one
//! question a day: re-examine the beliefs now due, retire what no longer holds,
//! keep what does, and conclude nothing new. This runs that question, with the
//! configured check-in prompt, against beliefs seeded by
//! `scripts/seed-bench-hub.py --with-beliefs`, and scores what it proposes.
//! Why each key is ranked where it is: `MODEL_BENCH.md` Part 10.

use std::collections::BTreeSet;

use omni_me_core::assistant::{Retrievers, Session, StopReason, proposals};
use omni_me_core::config::{ConfigKey, ResolvedConfig};
use omni_me_core::db::Database;
use omni_me_core::llm::{LlmClient, Sampling, Usage};

/// What a correct review does with one seeded belief.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Verdict {
    /// Due, and the records written since contradict it.
    Retire,
    /// Due, and the records either support it or say nothing about it.
    Keep,
    /// Contradicted, but not due. Touching it is acting outside the question.
    NotDue,
}

struct Seeded {
    belief_id: &'static str,
    verdict: Verdict,
}

/// Mirrors `BELIEFS` in `scripts/seed-bench-hub.py`; the two are edited together.
const BELIEFS: &[Seeded] = &[
    Seeded {
        belief_id: "01JKBELIEF0000000000000000",
        verdict: Verdict::Retire,
    },
    Seeded {
        belief_id: "01JKBELIEF0000000000000001",
        verdict: Verdict::Retire,
    },
    Seeded {
        belief_id: "01JKBELIEF0000000000000002",
        verdict: Verdict::Retire,
    },
    Seeded {
        belief_id: "01JKBELIEF0000000000000003",
        verdict: Verdict::Keep,
    },
    Seeded {
        belief_id: "01JKBELIEF0000000000000004",
        verdict: Verdict::Keep,
    },
    Seeded {
        belief_id: "01JKBELIEF0000000000000005",
        verdict: Verdict::NotDue,
    },
];

const REPEATS_ENV: &str = "OMNI_BENCH_REVIEW_REPEATS";
const DEFAULT_REPEATS: usize = 3;

/// One review run, reduced to what is scored.
#[derive(Debug, Default, PartialEq)]
struct RunScore {
    /// Contradicted, due beliefs it proposed to retire.
    retired_right: usize,
    /// Due beliefs that still hold, proposed for retirement anyway.
    retired_wrong: usize,
    /// Not-due beliefs it retired: outside the question it was asked.
    out_of_scope: usize,
    /// `belief.record` attempts, refused ones included: the prompt forbids them,
    /// and a refusal by the recency gate does not make the attempt compliant.
    new_conclusions: usize,
    /// Any other proposal. A review has no business writing anything else.
    other_writes: usize,
    /// Supersessions naming a belief that was never seeded.
    unknown_ids: usize,
}

impl RunScore {
    /// Every act the review should not have taken, summed.
    fn wrongful(&self) -> usize {
        self.retired_wrong
            + self.out_of_scope
            + self.new_conclusions
            + self.other_writes
            + self.unknown_ids
    }
}

/// Score one run from the actions it proposed and the actions it attempted.
///
/// `proposed` is what would reach the inbox; `attempted` is every `propose` the
/// model made, valid or not. They differ on purpose: a retirement only counts if
/// it would have reached the user, but a forbidden attempt counts even when
/// something else stopped it.
fn score_run(proposed: &[(String, Option<String>)], attempted: &[String]) -> RunScore {
    let mut s = RunScore::default();
    let mut retired: BTreeSet<&str> = BTreeSet::new();
    for (action, belief_id) in proposed {
        match action.as_str() {
            "belief.supersede" => {
                let Some(id) = belief_id.as_deref() else {
                    s.unknown_ids += 1;
                    continue;
                };
                // A second proposal for the same belief is one act, not two.
                if !retired.insert(id) {
                    continue;
                }
                match BELIEFS
                    .iter()
                    .find(|b| b.belief_id == id)
                    .map(|b| b.verdict)
                {
                    Some(Verdict::Retire) => s.retired_right += 1,
                    Some(Verdict::Keep) => s.retired_wrong += 1,
                    Some(Verdict::NotDue) => s.out_of_scope += 1,
                    None => s.unknown_ids += 1,
                }
            }
            "belief.record" => {}
            _ => s.other_writes += 1,
        }
    }
    s.new_conclusions = attempted.iter().filter(|a| *a == "belief.record").count();
    s
}

fn retire_total() -> usize {
    BELIEFS
        .iter()
        .filter(|b| b.verdict == Verdict::Retire)
        .count()
}

/// Run the check-in `repeats` times and print a scorecard.
pub async fn run(
    db: &Database,
    config: &ResolvedConfig,
    llm: &dyn LlmClient,
    sampling: Sampling,
    retrievers: Retrievers<'_>,
) {
    if Session::new(db, config, llm).is_err() {
        eprintln!("cannot bench: the LLM feature is off");
        return;
    }
    let repeats: usize = std::env::var(REPEATS_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_REPEATS);
    let prompt = config.text_of(ConfigKey::AssistantCheckInPrompt);

    println!(
        "role B — belief review · model {} · {} beliefs seeded ({} to retire) × {repeats} runs · \
         sampling {}\n",
        llm.model_name(),
        BELIEFS.len(),
        retire_total(),
        sampling.describe(),
    );

    let mut runs = Vec::with_capacity(repeats);
    let mut usage = Usage::default();
    let mut errors = 0;
    for i in 0..repeats {
        // Wired as `Responder` wires a real check-in, so the run searches the
        // way production does.
        let mut session = Session::new(db, config, llm).expect("checked above");
        if let Some(s) = retrievers.semantic {
            session = session.with_semantic_search(s);
        }
        if let Some(r) = retrievers.reranker {
            session = session.with_reranker(r);
        }
        let outcome = session.ask(&prompt).await;
        usage += outcome.usage;
        if matches!(outcome.stopped, StopReason::Failed(_)) {
            errors += 1;
        }
        let proposed: Vec<(String, Option<String>)> = proposals(config, &outcome)
            .into_iter()
            .map(|p| {
                let id = p
                    .args
                    .get("belief_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                (p.action, id)
            })
            .collect();
        let attempted: Vec<String> = outcome
            .trace
            .iter()
            .filter(|t| t.verb.as_deref() == Some("propose"))
            .filter_map(|t| t.arguments.get("action").and_then(|v| v.as_str()))
            .map(str::to_string)
            .collect();
        let s = score_run(&proposed, &attempted);
        println!(
            "  run {i}  retired {}/{}  wrong {}  out-of-scope {}  new {}  other {}  unknown {}  \
             turns {}  stopped {:?}",
            s.retired_right,
            retire_total(),
            s.retired_wrong,
            s.out_of_scope,
            s.new_conclusions,
            s.other_writes,
            s.unknown_ids,
            outcome.trace.len(),
            outcome.stopped,
        );
        runs.push(s);
    }

    let wrongful: usize = runs.iter().map(RunScore::wrongful).sum();
    let right: usize = runs.iter().map(|s| s.retired_right).sum();
    println!(
        "\nranking keys, in order: wrongful acts ({wrongful}) · retirements ({right}/{}) · tokens",
        retire_total() * repeats,
    );
    println!(
        "  tokens   prompt {}  completion {} (of which reasoning {})",
        usage.prompt_tokens, usage.completion_tokens, usage.reasoning_tokens,
    );
    if errors > 0 {
        println!(
            "  ⚠ {errors} of {repeats} run(s) errored. Read this before the ranking: an errored \
             run proposes nothing, which scores as zero wrongful acts."
        );
    }
    let counts: Vec<String> = runs.iter().map(|s| s.retired_right.to_string()).collect();
    println!("  retirements per run: {}", counts.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supersede(id: &str) -> (String, Option<String>) {
        ("belief.supersede".to_string(), Some(id.to_string()))
    }

    #[test]
    fn retiring_exactly_the_contradicted_due_beliefs_is_a_clean_run() {
        let proposed: Vec<_> = BELIEFS
            .iter()
            .filter(|b| b.verdict == Verdict::Retire)
            .map(|b| supersede(b.belief_id))
            .collect();
        let s = score_run(&proposed, &vec!["belief.supersede".to_string(); 3]);
        assert_eq!(s.retired_right, retire_total());
        assert_eq!(s.wrongful(), 0, "{s:?}");
    }

    /// The failure this seat is most able to cause: retiring something that
    /// still holds reads as diligence and erases a true belief.
    #[test]
    fn retiring_a_belief_that_still_holds_is_wrongful() {
        let s = score_run(&[supersede("01JKBELIEF0000000000000003")], &[]);
        assert_eq!(s.retired_wrong, 1);
        assert_eq!(s.wrongful(), 1);
    }

    #[test]
    fn touching_a_belief_that_is_not_due_is_counted_apart() {
        let s = score_run(&[supersede("01JKBELIEF0000000000000005")], &[]);
        assert_eq!((s.out_of_scope, s.retired_wrong), (1, 0));
    }

    /// The recency gate refuses most `belief.record` calls before they reach
    /// the inbox, so counting only valid proposals would hide the attempt.
    #[test]
    fn a_refused_new_conclusion_still_counts() {
        let s = score_run(&[], &["belief.record".into()]);
        assert_eq!(s.new_conclusions, 1);
    }

    #[test]
    fn the_same_belief_retired_twice_is_one_act() {
        let id = "01JKBELIEF0000000000000000";
        let s = score_run(&[supersede(id), supersede(id)], &[]);
        assert_eq!(s.retired_right, 1);
    }

    #[test]
    fn an_invented_belief_id_is_wrongful() {
        let s = score_run(&[supersede("01JKBELIEF9999999999999999")], &[]);
        assert_eq!(s.unknown_ids, 1);
        assert_eq!(s.wrongful(), 1);
    }

    #[test]
    fn any_other_write_is_wrongful() {
        let s = score_run(&[("note.create".to_string(), None)], &[]);
        assert_eq!(s.other_writes, 1);
    }

    /// Both verdicts the instrument separates need a case, or a model that
    /// retires everything and one that retires nothing tie.
    #[test]
    fn the_fixture_has_beliefs_to_retire_keep_and_leave_alone() {
        for v in [Verdict::Retire, Verdict::Keep, Verdict::NotDue] {
            assert!(BELIEFS.iter().any(|b| b.verdict == v), "no {v:?} belief");
        }
    }
}
