//! `--ask`: put one question through the assistant and print what happened.
//!
//! ⚠️ **Test scaffolding.** It exists because the verbs have no real interface
//! yet — the chat surface that replaces it is separate, later work. Nothing
//! should be built on this output format.
//!
//! What it prints is the *trace*, not just the answer, because during Phase B
//! the interesting question is which verbs the model chose and what it paid, not
//! whether the prose reads well.

use omni_me_core::assistant::{Outcome, Session, StopReason};
use omni_me_core::config::ResolvedConfig;
use omni_me_core::db::Database;
use omni_me_core::llm::LlmClient;

/// Run one question and print the trace, the answer, and the cost.
pub async fn run(db: &Database, config: &ResolvedConfig, llm: &dyn LlmClient, question: &str) {
    let session = match Session::new(db, config, llm) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot ask: {e}");
            return;
        }
    };

    println!("? {question}\n");
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
