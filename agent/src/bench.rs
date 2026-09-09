//! `--bench`: does the model pick the right verb, and what does constraining it cost?
//!
//! ⚠️ **Test scaffolding**, on the same terms as [`super::ask`].
//!
//! ## Why it is multi-turn
//!
//! An earlier version of this measurement scored only the model's *first* tool
//! call, which marked it wrong for calling `list_types` to find out what exists
//! before acting. That is not a wrong answer, it is the first step of a plan —
//! and since the design is a multi-turn agent, a single-turn test measures
//! something the system never does.
//!
//! ## What a scorecard reports, and why it is not only accuracy
//!
//! Verb accuracy alone cannot fill the model roles, because two of them are not
//! defined by accuracy: the interactive reasoner is chosen on a **latency
//! budget** and the high-volume structurer on **cost per call**. So each variant
//! also reports median and worst request latency, and prompt / completion /
//! reasoning tokens summed across cases. Tokens rather than dollars —
//! see [`Score::report_cost`].
//!
//! ## What the constraint tax is
//!
//! Every case runs twice: once with free tool calling, once with replies
//! constrained to a verb-call schema. Structured decoding is reported both as a
//! large accuracy win and as a suppressor of tool calling, and only running both
//! settles which one *this* tool surface gets. The difference between the two
//! scores is the tax, and it is a measurement rather than an assumption.
//!
//! The two halves must differ in **one** thing: how a call travels back. Each
//! carries the same verb documentation — the constrained half in its prompt,
//! since its request cannot carry `tools` — and offers exactly one channel. Open
//! both and the model picks per its own training, which makes the score a
//! property of the model rather than of the constraint; see
//! `Session::constrained`.
//!
//! ⚠️ **A tax is only reported when the constrained half was actually
//! constrained**, and there are two ways it silently is not. Providers differ on
//! whether a response schema is enforced as a grammar or read as a strong hint;
//! and a model trained to emit tool calls can have them parsed out of its
//! completion by the serving stack **even though the request declared no tools**,
//! which turns the constrained arm back into a free-form one while every visible
//! signal still reads as success. Both are counted as off-schema, and a non-zero
//! count withholds the number. Check the endpoint advertises `structured_outputs`
//! *at the pinned quantization* before running: the same provider serves the same
//! model with and without it depending on the tag.
//!
//! ⚠️ Not every endpoint can run the constrained half. One vendor's serving stack
//! applies the grammar from token zero and never terminates with a reasoning
//! model, which is a deployment-config fact about that vendor and not a property
//! of the weights. A constrained score of zero **with errors** means that, and
//! must not be read as the model failing.
//!
//! Both variants therefore run against **one** endpoint per invocation — whichever
//! `[llm]` selects — and the tax is read off a run where both halves completed.
//! Comparing a free-form score from one endpoint against a constrained score from
//! another would attribute a serving-stack difference to the constraint.
//!
//! ⚠️ On a rate-capped endpoint, set `OMNI_AGENT_LLM_MIN_INTERVAL_MS`. Without it
//! the cap returns errors that this scorecard would report as the model failing.
//!
//! ⚠️ **Run it as `cargo run -p omni-me-agent -- --bench`, never a prebuilt
//! binary.** A run takes long enough that the source usually moves underneath it,
//! and a stale binary produces a plausible scorecard measuring code that no longer
//! exists — which happened: a "-20 point constraint tax" was recorded from a
//! binary built eight minutes before the fix it was meant to be testing. `cargo
//! run` rebuilds; a path into `target/debug` does not.

use std::time::Duration;

use omni_me_core::assistant::{Session, StopReason};
use omni_me_core::config::ResolvedConfig;
use omni_me_core::db::Database;
use omni_me_core::llm::{LlmClient, Usage};

/// One case: a request, and the verb a correct run must reach.
struct Case {
    request: &'static str,
    /// The verb that has to appear in the trace. `None` means the right outcome
    /// is prose without tools — there is no write path, so a change request must
    /// end in the model saying so.
    expect: Option<&'static str>,
}

/// Deliberately mixed. Retrieval-only cases would say nothing about whether the
/// model can tell *finding* from *fetching*, or notice that it cannot write.
const CASES: &[Case] = &[
    Case {
        request: "What kinds of things do you have about me in here?",
        expect: Some("list_types"),
    },
    Case {
        request: "What is a routine made of in this app?",
        expect: Some("describe_type"),
    },
    Case {
        request: "Find anything I wrote about rent.",
        expect: Some("search"),
    },
    Case {
        request: "What did I write in my journal on 2026-03-14?",
        expect: Some("read"),
    },
    Case {
        request: "Did I ever mention a dentist appointment?",
        expect: Some("search"),
    },
    // The case `list` exists for: `search` matches text, and the word "routine"
    // appears nowhere in a routine named "Morning" — a record is not guaranteed
    // to contain the name of its own type.
    Case {
        request: "What routines do I have?",
        expect: Some("list"),
    },
    Case {
        request: "Which properties does my journal ask me to fill in?",
        expect: Some("describe_type"),
    },
    Case {
        request: "Add a note reminding me to call the landlord.",
        // No write path exists. Saying so is the correct answer; quietly doing
        // nothing while claiming success is the failure this case catches.
        expect: None,
    },
    Case {
        request: "Show me my note about the grocery list.",
        expect: Some("search"),
    },
    Case {
        request: "Delete all my journal entries.",
        expect: None,
    },
];

struct Score {
    correct: usize,
    total: usize,
    errors: usize,
    /// Replies that ignored the response schema. Zero by definition free-form.
    off_schema: usize,
    /// One entry per case: the wall time of the whole multi-turn request.
    ///
    /// **The request, not the call**, because the interactive role's budget is
    /// how long a person waits for an answer, and that is every turn plus the
    /// verb executions between them. A per-call median would flatter a model
    /// that answers fast but needs six turns to get there.
    latencies: Vec<Duration>,
    /// Summed across every case. Reasoning tokens are inside this and reported
    /// apart, because they are billed and rate-limited as output and were ~95%
    /// of it on the Phase 0 baseline.
    usage: Usage,
    turns: usize,
}

impl Score {
    fn pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        100.0 * self.correct as f64 / self.total as f64
    }

    /// Median and worst case, in seconds.
    ///
    /// **Not a p90.** Ten cases cannot resolve one — nearest-rank would land on
    /// the second-slowest run and dress it as a percentile. The worst case is
    /// the same information without the false precision, and it is the number
    /// that decides whether a model is tolerable on a bad day.
    fn latency_secs(&self) -> Option<(f64, f64)> {
        if self.latencies.is_empty() {
            return None;
        }
        let mut sorted = self.latencies.clone();
        sorted.sort();
        let median = sorted[sorted.len() / 2].as_secs_f64();
        let worst = sorted[sorted.len() - 1].as_secs_f64();
        Some((median, worst))
    }

    fn report_cost(&self) {
        if let Some((median, worst)) = self.latency_secs() {
            println!("  latency  median {median:.1}s   worst {worst:.1}s");
        }
        // Tokens, never dollars. Prices move, and a rate table compiled into
        // this binary would go stale silently — a wrong number that still looks
        // like a measurement. Multiply by the catalogue price when reading.
        println!(
            "  tokens   prompt {}  completion {} (of which reasoning {})  turns {}",
            self.usage.prompt_tokens,
            self.usage.completion_tokens,
            self.usage.reasoning_tokens,
            self.turns,
        );
    }
}

/// Run every case both ways and print a scorecard.
///
/// `constrained_only` runs the schema arm alone. It exists for endpoints that
/// serve `response_format` and no `tools` parameter at all, where the free-form
/// arm cannot run and a both-arms invocation aborts in pre-flight rather than
/// measuring anything — DeepInfra's Llama-4 endpoints are the case in point.
/// Accuracy, latency and tokens are all still real; only the tax is unavailable,
/// and it is reported as *not applicable* rather than *not measurable*, because a
/// deliberate omission and a broken endpoint are different findings.
pub async fn run(
    db: &Database,
    config: &ResolvedConfig,
    llm: &dyn LlmClient,
    constrained_only: bool,
) {
    if Session::new(db, config, llm).is_err() {
        eprintln!("cannot bench: the LLM feature is off");
        return;
    }

    if constrained_only {
        println!(
            "verb selection, {} cases, CONSTRAINED ARM ONLY\n",
            CASES.len()
        );
        let constrained = run_variant(db, config, llm, true).await;
        println!(
            "  schema-constrained {}/{} ({:.0}%){}",
            constrained.correct,
            constrained.total,
            constrained.pct(),
            match (constrained.errors, constrained.off_schema) {
                (0, 0) => String::new(),
                (e, 0) => format!("   ⚠️ {e} call(s) errored"),
                (0, s) => format!("   ⚠️ {s} repl(ies) ignored the schema"),
                (e, s) => format!("   ⚠️ {e} errored, {s} ignored the schema"),
            }
        );
        println!(
            "\n  constraint tax: NOT APPLICABLE — the free-form arm was not run, by \
             request. There is nothing wrong with this endpoint; it simply offers no \
             `tools` parameter, so an unconstrained arm does not exist to compare \
             against. The scores, latency and token counts above are real."
        );
        return;
    }

    println!("verb selection, {} cases, both variants\n", CASES.len());
    let free = run_variant(db, config, llm, false).await;
    let constrained = run_variant(db, config, llm, true).await;

    println!(
        "  free-form         {}/{} ({:.0}%){}",
        free.correct,
        free.total,
        free.pct(),
        match free.errors {
            0 => String::new(),
            e => format!("   ⚠️ {e} call(s) errored"),
        }
    );
    println!(
        "  schema-constrained {}/{} ({:.0}%){}",
        constrained.correct,
        constrained.total,
        constrained.pct(),
        match (constrained.errors, constrained.off_schema) {
            (0, 0) => String::new(),
            (e, 0) => format!("   ⚠️ {e} call(s) errored"),
            (0, s) => format!("   ⚠️ {s} repl(ies) ignored the schema"),
            (e, s) => format!("   ⚠️ {e} errored, {s} ignored the schema"),
        }
    );

    if free.errors == free.total && constrained.errors == constrained.total {
        // Nothing completed at all. Naming a cause here would be inventing one —
        // an unreachable endpoint, an exhausted key and a rejected pin all look
        // identical from inside the scorecard. The per-case lines above carry the
        // actual errors.
        println!(
            "\n  constraint tax: NOT MEASURABLE — every call in BOTH halves errored, so \
             this run says nothing about the model. Read the per-case errors above: \
             an unreachable endpoint, a rejected pin and an exhausted key all land here."
        );
        return;
    }
    if free.errors == free.total {
        // The mirror of the guard below, and the one the model slate forced into
        // existence: an endpoint can lack `tools` while offering
        // `structured_outputs`, so the *free-form* half is the one that cannot
        // run. Llama-4 on DeepInfra is exactly this. Without this branch the
        // subtraction below reads a 0% free-form score as the model failing and
        // reports the constrained score itself as a large positive "tax" — a
        // fabricated number, from a half that never happened.
        //
        // Such a run is still worth doing: the constrained scorecard, its
        // latency and its token counts are all real. Only the *difference*
        // between the halves is unavailable.
        println!(
            "\n  constraint tax: NOT MEASURABLE on this endpoint — every free-form call \
             errored while the constrained half ran, which is what a stack that serves \
             `response_format` but not `tools` looks like. The constrained numbers above \
             stand on their own."
        );
        return;
    }
    if constrained.errors == constrained.total {
        // The whole constrained half failed. That is almost always the endpoint,
        // not the model — see this module's header — and reporting a tax here
        // would be reporting a number about the wrong thing.
        println!(
            "\n  constraint tax: NOT MEASURABLE on this endpoint — every constrained \
             call errored, which is a serving-stack limitation rather than a model one."
        );
        return;
    }
    if constrained.off_schema > 0 {
        // The other way this half can be unsound, and the quieter one: the
        // endpoint accepted `response_format` and did not enforce it, so the
        // "constrained" run was never constrained. Providers differ on whether a
        // schema is a guarantee or a strong hint, and a scorecard cannot show
        // which you got. The scores stay printed — the run is still evidence
        // about the endpoint — but a difference between them is not a tax.
        println!(
            "\n  constraint tax: NOT MEASURABLE — {} constrained repl{} ignored the \
             schema, so this endpoint treats it as a hint rather than a grammar. \
             Subtracting these two numbers would describe nothing.",
            constrained.off_schema,
            if constrained.off_schema == 1 {
                "y"
            } else {
                "ies"
            },
        );
        return;
    }
    if free.errors > 0 || constrained.errors > 0 {
        // Partial failure, and the gap the guards above left open: they each fire
        // only when a half errored *entirely*, so a half that errored four times
        // out of ten sailed through and printed a number.
        //
        // An errored case scores as incorrect, which is right for accuracy and
        // wrong for a difference: the two halves then completed different numbers
        // of cases, and subtracting them blends "the constraint hurt" with "the
        // endpoint dropped calls". `deepseek-v4-pro` reported a confident
        // "-40 percentage points" this way, of which the constraint's share was
        // unknown and possibly zero.
        println!(
            "\n  constraint tax: NOT MEASURABLE — {} free-form and {} constrained call(s) \
             errored, so the halves did not complete the same work. An errored case counts \
             as incorrect, which is right for the scores above and fatal for their \
             difference. Re-run; if the errors persist they are the finding.",
            free.errors, constrained.errors,
        );
        return;
    }
    println!(
        "\n  constraint tax: {:+.0} percentage points",
        constrained.pct() - free.pct()
    );
}

async fn run_variant(
    db: &Database,
    config: &ResolvedConfig,
    llm: &dyn LlmClient,
    constrained: bool,
) -> Score {
    println!(
        "--- {}",
        if constrained {
            "schema-constrained"
        } else {
            "free-form"
        }
    );
    let mut score = Score {
        correct: 0,
        total: CASES.len(),
        errors: 0,
        off_schema: 0,
        latencies: Vec::with_capacity(CASES.len()),
        usage: Usage::default(),
        turns: 0,
    };

    for (i, case) in CASES.iter().enumerate() {
        let mut session = Session::new(db, config, llm).expect("checked above");
        if constrained {
            session = session.constrained();
        }
        let outcome = session.ask(case.request).await;
        let verbs = outcome.verbs();

        let failed = matches!(outcome.stopped, StopReason::Failed(_));
        if failed {
            score.errors += 1;
        }
        score.off_schema += outcome.off_schema;
        // A failed case still spent time and tokens getting there, so it is
        // counted: the cost of a model is what it actually bills, not what it
        // bills on the runs that worked.
        score.latencies.push(outcome.elapsed);
        score.usage += outcome.usage;
        score.turns += outcome.trace.len();

        // Reaching the right verb is necessary but **not sufficient**: the run
        // also has to finish. Scoring on "the verb appeared somewhere" alone
        // marked a run that burned all six turns and never answered as correct —
        // it was "what routines do I have?", which no read verb can answer
        // because there is nothing that enumerates. A benchmark that calls that
        // a pass cannot find the missing capability.
        let answered = outcome.stopped == StopReason::Answered;
        let reached = match case.expect {
            // Exploring first is legitimate, so anywhere in the trace counts.
            Some(verb) => verbs.contains(&verb),
            // "No verb" means it did not reach for a tool it does not have.
            // Calling a read verb and then declining is fine.
            None => true,
        };
        let ok = reached && answered;
        score.correct += ok as usize;

        let why = match (reached, answered) {
            (true, true) => "OK ",
            (false, true) => "   ", // answered, but not the way expected
            (true, false) => "NF ", // right verb, never finished
            (false, false) => "   ",
        };
        println!(
            "  {:02} {} want={:<14} turns={} path={}",
            i,
            why,
            case.expect.unwrap_or("(prose)"),
            outcome.trace.len(),
            if verbs.is_empty() {
                "(answered directly)".to_string()
            } else {
                verbs.join(" -> ")
            },
        );
    }
    score.report_cost();
    println!();
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score_with(latencies: &[u64]) -> Score {
        Score {
            correct: 0,
            total: latencies.len(),
            errors: 0,
            off_schema: 0,
            latencies: latencies
                .iter()
                .map(|ms| Duration::from_millis(*ms))
                .collect(),
            usage: Usage::default(),
            turns: 0,
        }
    }

    #[test]
    fn latency_is_reported_from_the_sorted_samples_not_the_run_order() {
        // Arrival order is whatever the endpoint did; the statistic must not
        // depend on it.
        let forwards = score_with(&[1_000, 2_000, 9_000]).latency_secs();
        let backwards = score_with(&[9_000, 1_000, 2_000]).latency_secs();
        assert_eq!(forwards, backwards);
        assert_eq!(forwards, Some((2.0, 9.0)));
    }

    /// One pathological run must not decide a latency budget — the whole reason
    /// the median is reported next to the worst case rather than a mean.
    #[test]
    fn a_single_outlier_moves_the_worst_case_but_not_the_median() {
        let (median, worst) = score_with(&[1_000, 1_000, 1_000, 1_000, 60_000])
            .latency_secs()
            .expect("samples exist");
        assert_eq!(median, 1.0);
        assert_eq!(worst, 60.0);
    }

    #[test]
    fn a_run_with_no_completed_cases_reports_no_latency_rather_than_zero() {
        // Zero would read as "instant" on a scorecard. Absent reads as absent.
        assert_eq!(score_with(&[]).latency_secs(), None);
    }

    /// Reasoning tokens are *inside* `completion_tokens`, and summing must not
    /// quietly change that — the split is what tells us where the money went.
    #[test]
    fn usage_sums_across_cases_and_keeps_reasoning_separate() {
        let a = Usage {
            prompt_tokens: 100,
            completion_tokens: 40,
            reasoning_tokens: 30,
            total_tokens: 140,
        };
        let b = Usage {
            prompt_tokens: 200,
            completion_tokens: 10,
            reasoning_tokens: 5,
            total_tokens: 210,
        };
        let summed: Usage = [a, b].into_iter().sum();
        assert_eq!(summed.prompt_tokens, 300);
        assert_eq!(summed.completion_tokens, 50);
        assert_eq!(summed.reasoning_tokens, 35);
        assert_eq!(summed.total_tokens, 350);
    }
}
