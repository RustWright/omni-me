//! `--bench-cases`: the requests that failed on the phone, scored on what the
//! run proposed rather than on which verbs it reached.
//!
//! The corpus is whatever this process pulled, which on the box is the dev log:
//! a clone of real data. So the checks are properties that hold on any ledger
//! (an account it names must already exist; "today" must be today) rather than
//! answers written down in advance. Nothing is applied: a proposal here is a
//! call in the trace, and this process never turns it into an event.
//! Background: `ASSISTANT_RESEARCH.md`, step 1 of the build order.

use std::collections::BTreeSet;

use omni_me_core::assistant::session::today_iso;
use omni_me_core::assistant::{Outcome, Retrievers, Session, StopReason};
use omni_me_core::config::ResolvedConfig;
use omni_me_core::db::{Database, queries};
use omni_me_core::llm::LlmClient;
use serde_json::Value;

const REPEATS_ENV: &str = "OMNI_BENCH_CASES_REPEATS";
const DEFAULT_REPEATS: usize = 3;

/// One property of a run. Every case also gets [`UNIVERSAL`].
#[derive(Debug, Clone, Copy)]
enum Check {
    /// It ended with a reply, not by running out of turns or failing.
    Answered,
    /// The reply does not claim a change that no proposal backs.
    NoUnbackedClaim,
    /// At least this many proposals of this action.
    Proposes(&'static str, usize),
    /// No proposal of this action.
    NeverProposes(&'static str),
    /// No proposal at all.
    ProposesNothing,
    /// Every recorded transaction is dated today where the user is.
    EntryDatedToday,
    /// Every account a recorded transaction names already exists in the ledger.
    EntryAccountsExist,
    /// Every category it assigns is one the ledger already uses.
    CategoryInUse,
    /// Every transaction it acts on is the newest whose description has this.
    TargetsNewest(&'static str),
    /// A note it creates has at least this many non-empty body lines.
    NoteBodyLines(usize),
    /// A routine is created with at least this many steps, inside the create
    /// or as separate add_item proposals.
    RoutineSteps(usize),
}

const UNIVERSAL: &[Check] = &[Check::Answered, Check::NoUnbackedClaim];

struct Case {
    /// Which finding it reproduces, so a failure reads back to its cause.
    id: &'static str,
    request: &'static str,
    checks: &'static [Check],
}

/// The 2026-10-11 capability run on the S9, request for request.
const CASES: &[Case] = &[
    Case {
        id: "R3-10 routine",
        // Evening rather than the phone run's weekday morning: dev already holds the
        // morning routine that run created, and declining a duplicate is correct.
        request: "Build me an evening wind-down routine based on what I've written in my \
                  journal about evenings and sleep. Keep it to 4 to 6 steps with rough \
                  durations.",
        checks: &[Check::Proposes("routine.create", 1), Check::RoutineSteps(4)],
    },
    Case {
        id: "R3-11 research-then-write",
        request: "Create a note titled 'Reading list' with three books I've mentioned in my \
                  journal that I want to read.",
        checks: &[Check::Proposes("note.create", 1), Check::NoteBodyLines(3)],
    },
    Case {
        id: "R3-5 R4-2 record",
        request: "I paid 14.50 CAD cash at Tim Hortons today for lunch. Record it.",
        checks: &[
            Check::Proposes("transaction.record", 1),
            Check::EntryDatedToday,
            Check::EntryAccountsExist,
        ],
    },
    Case {
        id: "journal refusal",
        request: "Add an entry to my journal for today saying that I tested the assistant on \
                  my phone.",
        checks: &[Check::NeverProposes("journal.append")],
    },
    Case {
        id: "R3-9 newest + categorize",
        request: "Find my most recent Uber transaction, categorize it as Transport, and tag it \
                  work.",
        checks: &[
            Check::Proposes("transaction.categorize", 1),
            Check::Proposes("transaction.tag", 1),
            Check::TargetsNewest("uber"),
            Check::CategoryInUse,
        ],
    },
    Case {
        id: "note create",
        request: "Create a note titled 'Assistant test' with the line: first line.",
        checks: &[Check::Proposes("note.create", 1)],
    },
    Case {
        id: "belief",
        request: "Based on my journal, what do you conclude about my relationship with \
                  exercise? Record it as a belief if you are confident.",
        checks: &[],
    },
    Case {
        id: "belief retire",
        request: "The belief that I read in the mornings on weekdays and late at night \
                  on weekends no longer holds. Retire it.",
        // Retired on dev on 2026-10-11, so "already retired" is the right answer
        // now; only the universal checks apply.
        checks: &[],
    },
    Case {
        id: "R3-9 newest + clear",
        request: "Mark my most recent Uber transaction as cleared.",
        checks: &[Check::TargetsNewest("uber")],
    },
    Case {
        id: "spend question",
        request: "How much did I spend on groceries in September 2026, and at which stores?",
        checks: &[Check::ProposesNothing],
    },
    Case {
        id: "R3-9 newest payslip",
        request: "What was the net pay on my most recent payslip?",
        checks: &[Check::ProposesNothing],
    },
    Case {
        id: "topics, no proposals",
        request: "Without drawing any conclusions about me, list the 8 to 10 topics I write \
                  about most often in my journal, each with one line on how often it comes up \
                  and over what years. Do not propose anything.",
        checks: &[Check::ProposesNothing],
    },
];

/// What the checks compare against, read once from the pulled ledger.
struct Ledger {
    accounts: BTreeSet<String>,
    categories: BTreeSet<String>,
    /// Transactions newest first, as (id, date, description).
    newest_first: Vec<(String, String, String)>,
    today: String,
}

impl Ledger {
    async fn read(db: &Database, config: &ResolvedConfig) -> Result<Self, String> {
        let rows = queries::query_candidate_transactions(db)
            .await
            .map_err(|e| e.to_string())?;
        let mut accounts: BTreeSet<String> = queries::list_accounts(db)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|a| a.id)
            .collect();
        let mut categories = BTreeSet::new();
        let mut newest_first = Vec::with_capacity(rows.len());
        for row in rows {
            if let Value::Array(postings) = row.postings.clone().into_json_value() {
                for p in postings {
                    if let Some(a) = p.get("account").and_then(Value::as_str) {
                        accounts.insert(a.to_string());
                    }
                }
            }
            if let Some(c) = &row.category {
                categories.insert(c.clone());
            }
            newest_first.push((row.id, row.date, row.description));
        }
        Ok(Self {
            accounts,
            categories,
            newest_first,
            today: today_iso(config),
        })
    }

    /// Every match on the newest date: a bank row and its receipt share one.
    fn newest_matching(&self, needle: &str) -> Vec<&str> {
        let needle = needle.to_lowercase();
        let matching: Vec<&(String, String, String)> = self
            .newest_first
            .iter()
            .filter(|(_, _, d)| d.to_lowercase().contains(&needle))
            .collect();
        let Some((_, newest, _)) = matching.first() else {
            return Vec::new();
        };
        matching
            .iter()
            .filter(|(_, date, _)| date == newest)
            .map(|(id, _, _)| id.as_str())
            .collect()
    }
}

/// Each proposal the run made and the loop accepted, as (action, args).
fn proposals(outcome: &Outcome) -> Vec<(String, Value)> {
    outcome
        .trace
        .iter()
        .filter(|t| t.verb.as_deref() == Some("propose") && !t.refused)
        .filter_map(|t| {
            let action = t.arguments.get("action")?.as_str()?.to_string();
            Some((
                action,
                t.arguments.get("args").cloned().unwrap_or(Value::Null),
            ))
        })
        .collect()
}

/// The date and posting accounts of an hledger entry.
fn entry_parts(entry: &str) -> (Option<String>, Vec<String>) {
    let mut lines = entry.lines().filter(|l| !l.trim().is_empty());
    let date = lines
        .next()
        .and_then(|l| l.split_whitespace().next())
        .map(str::to_string);
    let accounts = lines
        .filter(|l| l.starts_with([' ', '\t']))
        .filter_map(|l| {
            let l = l.trim();
            let end = l.find("  ").or_else(|| l.find('\t')).unwrap_or(l.len());
            let a = l[..end].trim();
            (!a.is_empty() && !a.starts_with(';')).then(|| a.to_string())
        })
        .collect();
    (date, accounts)
}

/// Transaction ids an action's arguments name.
fn txn_ids(args: &Value) -> Vec<String> {
    let mut out: Vec<String> = args
        .get("txn_ids")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if let Some(id) = args.get("txn_id").and_then(Value::as_str) {
        out.push(id.to_string());
    }
    out
}

/// The proposals of one action.
fn of<'a>(
    made: &'a [(String, Value)],
    action: &'a str,
) -> impl Iterator<Item = &'a (String, Value)> {
    made.iter().filter(move |(a, _)| a == action)
}

/// `Ok` or the reason it failed.
fn check(
    c: Check,
    outcome: &Outcome,
    made: &[(String, Value)],
    ledger: &Ledger,
) -> Result<(), String> {
    match c {
        Check::Answered => {
            let text = outcome.answer.as_deref().unwrap_or("").trim();
            if matches!(outcome.stopped, StopReason::Answered) && !text.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "stopped {:?}, answer {} chars",
                    outcome.stopped,
                    text.len()
                ))
            }
        }
        Check::NoUnbackedClaim => {
            match omni_me_core::assistant::claims::claimed_change(
                outcome.answer.as_deref().unwrap_or(""),
            ) {
                Some(c) if made.is_empty() => Err(format!("says {c:?} with nothing proposed")),
                _ => Ok(()),
            }
        }
        Check::Proposes(action, n) => {
            let got = of(made, action).count();
            if got >= n {
                Ok(())
            } else {
                Err(format!("{action}: {got} of {n}"))
            }
        }
        Check::NeverProposes(action) => match of(made, action).count() {
            0 => Ok(()),
            n => Err(format!("{action} proposed {n}x")),
        },
        Check::ProposesNothing => match made.len() {
            0 => Ok(()),
            n => Err(format!("{n} proposal(s)")),
        },
        Check::EntryDatedToday => {
            for (_, args) in of(made, "transaction.record") {
                let entry = args.get("entry").and_then(Value::as_str).unwrap_or("");
                let (date, _) = entry_parts(entry);
                if date.as_deref() != Some(ledger.today.as_str()) {
                    return Err(format!("dated {date:?}, today is {}", ledger.today));
                }
            }
            Ok(())
        }
        Check::EntryAccountsExist => {
            let mut unknown = BTreeSet::new();
            for (_, args) in of(made, "transaction.record") {
                let entry = args.get("entry").and_then(Value::as_str).unwrap_or("");
                for a in entry_parts(entry).1 {
                    if !ledger.accounts.contains(&a) {
                        unknown.insert(a);
                    }
                }
            }
            if unknown.is_empty() {
                Ok(())
            } else {
                Err(format!("accounts not in the ledger: {unknown:?}"))
            }
        }
        Check::CategoryInUse => {
            for (_, args) in of(made, "transaction.categorize") {
                let cat = args.get("category").and_then(Value::as_str).unwrap_or("");
                if !ledger.categories.contains(cat) && !ledger.accounts.contains(cat) {
                    return Err(format!("category {cat:?} is new to the ledger"));
                }
            }
            Ok(())
        }
        Check::TargetsNewest(needle) => {
            let newest = ledger.newest_matching(needle);
            if newest.is_empty() {
                return Err(format!("no {needle:?} transaction in this corpus"));
            }
            let touched: Vec<String> = made
                .iter()
                .filter(|(a, _)| a.starts_with("transaction."))
                .flat_map(|(_, args)| txn_ids(args))
                .collect();
            if touched.iter().any(|id| !newest.contains(&id.as_str())) {
                Err(format!("acted on {touched:?}, newest is {newest:?}"))
            } else {
                Ok(())
            }
        }
        Check::RoutineSteps(n) => {
            let inside: usize = of(made, "routine.create")
                .map(|(_, a)| a.get("steps").and_then(Value::as_array).map_or(0, Vec::len))
                .sum();
            let separate = of(made, "routine.add_item").count();
            if inside + separate >= n {
                Ok(())
            } else {
                Err(format!("{} step(s), wanted {n}", inside + separate))
            }
        }
        Check::NoteBodyLines(n) => {
            for (_, args) in of(made, "note.create") {
                let body = args.get("body").and_then(Value::as_str).unwrap_or("");
                let lines = body.lines().filter(|l| !l.trim().is_empty()).count();
                if lines < n {
                    return Err(format!("note body has {lines} line(s), wanted {n}"));
                }
            }
            Ok(())
        }
    }
}

pub async fn run(
    db: &Database,
    config: &ResolvedConfig,
    llm: &dyn LlmClient,
    retrievers: Retrievers<'_>,
) {
    let ledger = match Ledger::read(db, config).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bench: could not read the ledger: {e}");
            return;
        }
    };
    let repeats: usize = std::env::var(REPEATS_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_REPEATS);
    println!(
        "case bench: {} cases x {repeats} runs · model {} · ledger {} accounts, {} \
         transactions · today {}\n",
        CASES.len(),
        llm.model_name(),
        ledger.accounts.len(),
        ledger.newest_first.len(),
        ledger.today
    );

    let (mut passed, mut total) = (0usize, 0usize);
    let mut tokens = 0u64;
    let mut cached = 0u64;
    for case in CASES {
        let mut case_passes = 0;
        for run in 1..=repeats {
            let session = match Session::new(db, config, llm) {
                Ok(s) => s.with_specialists(),
                Err(e) => {
                    eprintln!("cannot bench: {e}");
                    return;
                }
            };
            let session = match retrievers.semantic {
                Some(s) => session.with_semantic_search(s),
                None => session,
            };
            let session = match retrievers.reranker {
                Some(r) => session.with_reranker(r),
                None => session,
            };
            let outcome = session.ask(case.request).await;
            tokens +=
                u64::from(outcome.usage.prompt_tokens) + u64::from(outcome.usage.completion_tokens);
            cached += u64::from(outcome.usage.cached_tokens);
            let made = proposals(&outcome);
            let failures: Vec<String> = UNIVERSAL
                .iter()
                .chain(case.checks)
                .filter_map(|c| check(*c, &outcome, &made, &ledger).err())
                .collect();
            let actions: Vec<&str> = made.iter().map(|(a, _)| a.as_str()).collect();
            println!(
                "{} {:<26} run {run}  {:>3} turns {:>5.1}s  proposed {:?}",
                if failures.is_empty() { "PASS" } else { "FAIL" },
                case.id,
                outcome.trace.len(),
                outcome.elapsed.as_secs_f64(),
                actions
            );
            for f in &failures {
                println!("       - {f}");
            }
            if !failures.is_empty() {
                let reply: String = outcome
                    .answer
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(160)
                    .collect();
                println!("       verbs {:?}", outcome.verbs());
                println!("       reply {:?}", reply);
            }
            if failures.is_empty() {
                case_passes += 1;
            }
        }
        passed += case_passes;
        total += repeats;
    }
    println!(
        "\n{passed}/{total} runs passed every check ({:.0}%) · {tokens} tokens, {cached} of \
         the prompt served from the provider's cache",
        100.0 * passed as f64 / total.max(1) as f64
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_yields_its_date_and_every_posting_account() {
        let entry = "2026-10-11 Tim Hortons\n    Expenses:Food:Dining  14.50 CAD\n    \
                     Assets:Cash\n    ; a comment\n";
        let (date, accounts) = entry_parts(entry);
        assert_eq!(date.as_deref(), Some("2026-10-11"));
        assert_eq!(accounts, ["Expenses:Food:Dining", "Assets:Cash"]);
    }

    #[test]
    fn single_and_listed_transaction_ids_are_both_read() {
        let args = serde_json::json!({"txn_ids": ["a", "b"], "txn_id": "c"});
        assert_eq!(txn_ids(&args), ["a", "b", "c"]);
    }
}
