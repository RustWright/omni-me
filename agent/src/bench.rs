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
//! ## What the constraint tax is
//!
//! Every case runs twice: once with free tool calling, once with replies
//! constrained to a verb-call schema. Structured decoding is reported both as a
//! large accuracy win and as a suppressor of tool calling, and only running both
//! settles which one *this* tool surface gets. The difference between the two
//! scores is the tax, and it is a measurement rather than an assumption.
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

use omni_me_core::assistant::{Session, StopReason};
use omni_me_core::config::ResolvedConfig;
use omni_me_core::db::Database;
use omni_me_core::llm::LlmClient;

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
    // ⚠️ **Known to fail, and kept because it fails.** No read verb can answer
    // this: `search` is text matching, and the word "routine" appears nowhere in
    // a routine named "Morning". Enumerating a kind of record is a capability the
    // surface does not have. Observed live burning all six turns —
    // `search{query:""} → search{query:"routine"} → …` — and it stays here as the
    // standing evidence for whether a listing verb earns its place.
    Case {
        request: "What routines do I have?",
        expect: Some("search"),
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
}

impl Score {
    fn pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        100.0 * self.correct as f64 / self.total as f64
    }
}

/// Run every case both ways and print a scorecard.
pub async fn run(db: &Database, config: &ResolvedConfig, llm: &dyn LlmClient) {
    if Session::new(db, config, llm).is_err() {
        eprintln!("cannot bench: the LLM feature is off");
        return;
    }

    println!("verb selection, {} cases, both variants\n", CASES.len());
    let free = run_variant(db, config, llm, false).await;
    let constrained = run_variant(db, config, llm, true).await;

    println!("  free-form         {}/{} ({:.0}%)", free.correct, free.total, free.pct());
    println!(
        "  schema-constrained {}/{} ({:.0}%){}",
        constrained.correct,
        constrained.total,
        constrained.pct(),
        if constrained.errors > 0 {
            format!("   ⚠️ {} call(s) errored", constrained.errors)
        } else {
            String::new()
        }
    );

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
    println!("--- {}", if constrained { "schema-constrained" } else { "free-form" });
    let mut score = Score {
        correct: 0,
        total: CASES.len(),
        errors: 0,
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
            (false, true) => "   ",     // answered, but not the way expected
            (true, false) => "NF ",     // right verb, never finished
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
    println!();
    score
}
