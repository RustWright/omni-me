//! Specialists: one request, routed to the instructions of the domain it is
//! about. The ledger is the first (R4-2); archive and journal follow the same
//! shape. A specialist adds instructions and context to the run; it never adds
//! a verb or loosens a gate.

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
}

const ROUTER_PROMPT: &str = "You route requests sent to a personal assistant. Reply \
\"ledger\" if the latest request involves money: recording, finding, categorizing, tagging, \
clearing or correcting transactions, spending, payments, receipts, accounts or balances. \
Otherwise reply \"general\". Earlier turns are only there to resolve words like \"that\" or \
\"yes\".";

/// Turns of history the router sees, so "yes, record it" routes like the
/// request it answers.
const ROUTER_HISTORY: usize = 2;

fn router_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "specialist": { "type": "string", "enum": ["ledger", "general"] } },
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
    if !config.enabled(Feature::Finances) {
        return Specialist::General;
    }
    let mut prompt = String::from(ROUTER_PROMPT);
    for turn in history.iter().rev().take(ROUTER_HISTORY).rev() {
        let who = match turn.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
        };
        let text: String = turn.text.chars().take(600).collect();
        prompt.push_str(&format!("\n\n{who}: {text}"));
    }
    prompt.push_str(&format!("\n\nLatest request: {question}"));
    match llm.complete_json(&prompt, &router_schema()).await {
        Ok(v) if v["specialist"] == "ledger" => Specialist::Ledger,
        Ok(_) => Specialist::General,
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
pub async fn brief(db: &Database, specialist: Specialist) -> String {
    match specialist {
        Specialist::General => String::new(),
        Specialist::Ledger => {
            let context = match ledger_grounding::conventions_note(db).await {
                Some(note) => format!(
                    "\n\n### The user's conventions (their note \"{}\")\n{}",
                    note.title, note.raw_text
                ),
                None => {
                    let accounts = ledger_grounding::known_accounts(db).await;
                    format!(
                        "\n\n### Accounts in this ledger\n{}",
                        accounts.into_iter().collect::<Vec<_>>().join("\n")
                    )
                }
            };
            format!("{LEDGER_RULES}{context}")
        }
    }
}
