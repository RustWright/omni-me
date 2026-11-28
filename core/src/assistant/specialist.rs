//! Specialists: one request, routed to the instructions of the domain it is
//! about. The ledger is the first, ruling 21 (RULINGS.md), and planning the second (112). A
//! specialist adds instructions and context to the run; it never adds a verb or loosens a gate.

use serde_json::{Value, json};

use super::conversation::{Role, Turn};
use super::ledger_grounding;
use crate::config::{Feature, ResolvedConfig};
use crate::db::Database;
use crate::llm::LlmClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Specialist {
    General,
    Ledger,
    Planning,
}

impl Specialist {
    fn label(self) -> &'static str {
        match self {
            Specialist::General => "general",
            Specialist::Ledger => "ledger",
            Specialist::Planning => "planning",
        }
    }

    /// When the router should pick it, as the router prompt says it.
    fn when(self) -> &'static str {
        match self {
            Specialist::General => "",
            Specialist::Ledger => {
                "if the latest request involves money: recording, finding, categorizing, \
                 tagging, clearing or correcting transactions, spending, payments, receipts, \
                 accounts or balances"
            }
            Specialist::Planning => {
                "if the latest request is about planning a day: a day plan, what to do when, \
                 fitting work around the calendar, moving or changing planned blocks, or how \
                 much free time is left"
            }
        }
    }

    fn feature(self) -> Option<Feature> {
        match self {
            Specialist::General => None,
            Specialist::Ledger => Some(Feature::Finances),
            Specialist::Planning => Some(Feature::Planning),
        }
    }
}

/// Every specialist the router may pick, in the order its prompt lists them.
const SPECIALISTS: &[Specialist] = &[Specialist::Ledger, Specialist::Planning];

/// Turns of history the router sees, so "yes, record it" routes like the
/// request it answers.
const ROUTER_HISTORY: usize = 2;

fn enabled(config: &ResolvedConfig) -> Vec<Specialist> {
    SPECIALISTS
        .iter()
        .copied()
        .filter(|s| s.feature().is_some_and(|f| config.enabled(f)))
        .collect()
}

fn router_prompt(choices: &[Specialist]) -> String {
    let mut prompt = String::from("You route requests sent to a personal assistant.");
    for s in choices {
        prompt.push_str(&format!(" Reply \"{}\" {}.", s.label(), s.when()));
    }
    prompt.push_str(
        " Otherwise reply \"general\". Earlier turns are only there to resolve words like \
         \"that\" or \"yes\".",
    );
    prompt
}

fn router_schema(choices: &[Specialist]) -> Value {
    let mut names: Vec<&str> = choices.iter().map(|s| s.label()).collect();
    names.push(Specialist::General.label());
    json!({
        "type": "object",
        "properties": { "specialist": { "type": "string", "enum": names } },
        "required": ["specialist"]
    })
}

/// Which specialist takes the request. Any failure routes to general: a
/// misroute costs context, never safety.
pub async fn route(
    llm: &dyn LlmClient,
    config: &ResolvedConfig,
    history: &[Turn],
    question: &str,
) -> Specialist {
    let choices = enabled(config);
    if choices.is_empty() {
        return Specialist::General;
    }
    let mut prompt = router_prompt(&choices);
    for turn in history.iter().rev().take(ROUTER_HISTORY).rev() {
        let who = match turn.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
        };
        let text: String = turn.text.chars().take(600).collect();
        prompt.push_str(&format!("\n\n{who}: {text}"));
    }
    prompt.push_str(&format!("\n\nLatest request: {question}"));
    match llm.complete_json(&prompt, &router_schema(&choices)).await {
        Ok(v) => choices
            .iter()
            .copied()
            .find(|s| v["specialist"] == s.label())
            .unwrap_or(Specialist::General),
        Err(e) => {
            tracing::warn!(error = %e, "routing failed; answering as general");
            Specialist::General
        }
    }
}

const LEDGER_RULES: &str = "\n\n## Ledger work\n\
This request is about the user's ledger, which is kept in hledger syntax by their own \
conventions. Follow them exactly:\n\
- Use only accounts the ledger already has. Never invent an account, a category or a tag \
spelling. If nothing fits, say so instead of proposing.\n\
- Before recording or changing a transaction, search past transactions with the same payee \
and copy how they were recorded: accounts, tags and description style.\n\
- \"Most recent\" means the latest date: list transactions and compare dates rather than \
trusting search order.";

/// The instructions and context a specialist adds to the system prompt.
pub async fn brief(db: &Database, config: &ResolvedConfig, specialist: Specialist) -> String {
    match specialist {
        Specialist::General => String::new(),
        Specialist::Planning => {
            let context = crate::planning::brief::context(db, config, chrono::Utc::now()).await;
            format!("{}{context}", crate::planning::brief::RULES)
        }
        Specialist::Ledger => {
            // The list is what a proposal is checked against, so it is always sent;
            // the note then only has to say when each account applies.
            let accounts = ledger_grounding::known_accounts(db).await;
            let mut context = format!(
                "\n\n### Accounts in this ledger\n{}",
                accounts.into_iter().collect::<Vec<_>>().join("\n")
            );
            if let Some(note) = ledger_grounding::conventions_note(db).await {
                context.push_str(&format!(
                    "\n\n### The user's conventions (their note \"{}\")\n{}",
                    note.title, note.raw_text
                ));
            }
            format!("{LEDGER_RULES}{context}")
        }
    }
}
