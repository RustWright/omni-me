//! `--ask`: put one question through the assistant and print what happened.
//!
//! A local dev tool that stays: it prints the trace (verbs chosen, turns, cost) that model
//! re-evaluation reads, and `scripts/bench-openrouter.sh` relies on `--ask --constrained`.
//! It cannot run on the box, where the resident agent holds the `surrealkv` lock. Nothing
//! should be built on this output format.

use omni_me_core::assistant::{Outcome, Retrievers, Session, StopReason};
use omni_me_core::config::ResolvedConfig;
use omni_me_core::db::Database;
use omni_me_core::llm::LlmClient;

/// Run one question and print the trace, the answer, and the cost.
///
/// `constrained` runs it under the verb-call schema. Both retrievers are optional and printed
/// in the header, because a trace where retrieval quietly halved gets misread later.
pub async fn run(
    db: &Database,
    config: &ResolvedConfig,
    llm: &dyn LlmClient,
    question: &str,
    constrained: bool,
    retrievers: Retrievers<'_>,
) {
    let session = match Session::new(db, config, llm) {
        // As the resident agent runs, so a trace shows what a real answer does.
        Ok(s) => s.with_specialists(),
        Err(e) => {
            eprintln!("cannot ask: {e}");
            return;
        }
    };
    let session = if constrained {
        session.constrained()
    } else {
        session
    };
    let session = match retrievers.semantic {
        Some(s) => session.with_semantic_search(s),
        None => session,
    };
    let session = match retrievers.reranker {
        Some(r) => session.with_reranker(r),
        None => session,
    };

    // Named, because the two variants fail in different ways and a trace with no
    // label is the kind of evidence that gets misfiled later.
    println!(
        "? {question}   [{}, {}]\n",
        if constrained {
            "schema-constrained"
        } else {
            "free-form"
        },
        match (retrievers.semantic.is_some(), retrievers.reranker.is_some()) {
            (true, true) => "hybrid retrieval + reranked",
            (true, false) => "hybrid retrieval",
            (false, true) => "keyword, reranked",
            (false, false) => "keyword only",
        }
    );
    let outcome = session.ask(question).await;
    print_outcome(&outcome);
}

pub fn print_outcome(outcome: &Outcome) {
    for (i, turn) in outcome.trace.iter().enumerate() {
        let what = match &turn.verb {
            Some(v) => {
                let args = compact(&turn.arguments);
                let repeat = if turn.repeated { "  [repeat]" } else { "" };
                format!("{v}{args}{repeat}")
            }
            None => "final answer".to_string(),
        };
        println!(
            "  turn {}  {:<48} {:>5}ms  {}p/{}c/{}r",
            i + 1,
            what,
            turn.latency.as_millis(),
            turn.usage.prompt_tokens,
            turn.usage.completion_tokens,
            turn.usage.reasoning_tokens,
        );
    }

    println!();
    match &outcome.stopped {
        StopReason::Answered => {
            println!("{}", outcome.answer.as_deref().unwrap_or("(no text)"));
        }
        StopReason::TurnBudget => {
            println!(
                "(gave up after {} turns without answering)",
                outcome.trace.len()
            );
        }
        StopReason::Failed(why) => println!("(failed: {why})"),
    }

    // Reasoning tokens reported separately, always: they are billed and
    // rate-limited as output and were the majority of it on the baseline model,
    // so a combined figure would misstate both cost and where the time went.
    println!(
        "\n{} turns · {:.1}s · {} prompt / {} completion / {} reasoning tokens",
        outcome.trace.len(),
        outcome.elapsed.as_secs_f64(),
        outcome.usage.prompt_tokens,
        outcome.usage.completion_tokens,
        outcome.usage.reasoning_tokens,
    );

    // Printed here so one cheap question can rule an endpoint out before a bench
    // is spent on it. A stack that compiled the schema into a grammar cannot
    // return anything but the envelope; one that read it as a suggestion can,
    // and nothing else in this output would show the difference.
    if outcome.off_schema > 0 {
        println!(
            "\n⚠️  {} repl{} ignored the response schema. This endpoint did not enforce \
             the grammar, so a constraint tax measured here would describe nothing.",
            outcome.off_schema,
            if outcome.off_schema == 1 { "y" } else { "ies" },
        );
    }
}

/// Arguments on one line, short enough to keep the trace scannable.
fn compact(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Null => String::new(),
        v => {
            let s = serde_json::to_string(v).unwrap_or_default();
            if s == "{}" {
                return String::new();
            }
            let s: String = s.chars().take(60).collect();
            format!(" {s}")
        }
    }
}
