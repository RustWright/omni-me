//! Grounding a proposed ledger write in the ledger it is written to (R4-2).
//!
//! Two checks run when `transaction.record` is proposed, and a failure goes back
//! to the model as an error on the same turn, so it can fix the entry: every
//! account must already exist, and a payee recorded before must keep its account.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::accounts::UNMATCHED_ACCOUNT;
use crate::db::{Database, queries};

/// The title of the note holding the ledger's conventions, matched without
/// case. A title rather than a tag: nothing writes note tags yet.
pub const CONVENTIONS_TITLE: &str = "Ledger conventions";

/// How many closest accounts an error names.
const SUGGESTIONS: usize = 5;

/// How many past entries a payee check looks at.
const PAST_ENTRIES: u32 = 5;

const TOP_LEVELS: &[&str] = &["Assets", "Liabilities", "Expenses", "Income", "Equity"];

/// The conventions note, newest if several carry the title.
pub async fn conventions_note(db: &Database) -> Option<queries::GenericNoteRow> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS id, title, raw_text, tags, summary, legacy_properties,
                    <string> created_at AS created_at, <string> updated_at AS updated_at
             FROM generic_notes WHERE string::lowercase(title) = string::lowercase($title)
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(("title", CONVENTIONS_TITLE))
        .await
        .ok()?;
    let rows: Vec<queries::GenericNoteRow> = resp.take(0).ok()?;
    rows.into_iter().next()
}

/// Every full account name written in `text`, such as `Expenses:Food:Dining`.
pub fn accounts_named_in(text: &str) -> BTreeSet<String> {
    text.split(|c: char| c.is_whitespace() || "`,;()[]|*\"'".contains(c))
        .map(|w| w.trim_end_matches(['.', ':']))
        .filter(|w| {
            w.split_once(':')
                .is_some_and(|(top, rest)| TOP_LEVELS.contains(&top) && !rest.is_empty())
        })
        .map(str::to_string)
        .collect()
}

/// The accounts a proposal may name.
///
/// With a conventions note, its accounts plus the declared ones: the ledger
/// itself may hold names the importers invented, which are not his convention.
/// Without one, every account the ledger already uses.
pub async fn known_accounts(db: &Database) -> BTreeSet<String> {
    let mut known: BTreeSet<String> = queries::list_accounts(db)
        .await
        .map(|rows| rows.into_iter().map(|a| a.id).collect())
        .unwrap_or_default();
    known.insert(UNMATCHED_ACCOUNT.to_string());
    match conventions_note(db).await {
        Some(note) => known.extend(accounts_named_in(&note.raw_text)),
        None => known.extend(ledger_accounts(db).await),
    }
    known
}

async fn ledger_accounts(db: &Database) -> BTreeSet<String> {
    let rows = queries::query_candidate_transactions(db)
        .await
        .unwrap_or_default();
    let mut out = BTreeSet::new();
    for row in rows {
        if let Value::Array(postings) = row.postings.into_json_value() {
            out.extend(
                postings
                    .iter()
                    .filter_map(|p| p.get("account")?.as_str().map(str::to_string)),
            );
        }
    }
    out
}

/// The known accounts closest to `wanted`: same top level, most name parts in
/// common, then shortest.
pub fn closest<'a>(wanted: &str, known: &'a BTreeSet<String>) -> Vec<&'a str> {
    let words = |a: &str| -> BTreeSet<String> {
        a.split(|c: char| c == ':' || c.is_whitespace())
            .skip(1)
            .map(str::to_lowercase)
            .collect()
    };
    let top = wanted.split(':').next().unwrap_or("");
    let want = words(wanted);
    let mut scored: Vec<(usize, usize, &str)> = known
        .iter()
        .filter(|k| k.split(':').next() == Some(top))
        .map(|k| {
            let shared = words(k)
                .iter()
                .filter(|w| {
                    want.iter()
                        .any(|x| x.contains(w.as_str()) || w.contains(x.as_str()))
                })
                .count();
            (shared, k.len(), k.as_str())
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(SUGGESTIONS)
        .map(|(_, _, k)| k)
        .collect()
}

/// An importer's account hint held to the known accounts: kept when known;
/// otherwise the one known account under the same top level whose last part
/// matches (`Expenses:Groceries` to `Expenses:Food:Groceries`); otherwise `None`,
/// so the importer's own default leaves it for review.
pub fn ground_hint(hint: &str, known: &BTreeSet<String>) -> Option<String> {
    if known.contains(hint) {
        return Some(hint.to_string());
    }
    let top = hint.split(':').next()?;
    let leaf = hint.rsplit(':').next()?.to_lowercase();
    let mut same: Vec<&String> = known
        .iter()
        .filter(|k| k.split(':').next() == Some(top))
        .filter(|k| k.rsplit(':').next().map(str::to_lowercase).as_deref() == Some(leaf.as_str()))
        .collect();
    match (same.pop(), same.is_empty()) {
        (Some(one), true) => Some(one.clone()),
        _ => None,
    }
}

/// The spending accounts an extractor may choose from.
pub async fn spending_accounts(db: &Database) -> Vec<String> {
    known_accounts(db)
        .await
        .into_iter()
        .filter(|a| a.starts_with("Expenses:"))
        .collect()
}

/// Hold every account hint an extraction carries to the known accounts. A hint
/// that does not ground takes the payee's account when past entries for the
/// payee all used one, so a known payee keeps its account here as in proposals.
pub async fn ground_hints(
    db: &Database,
    description: &str,
    postings: &mut [crate::extraction::ExtractedPosting],
) {
    let known = known_accounts(db).await;
    let mut payee_account: Option<Option<String>> = None;
    for p in postings {
        let Some(hint) = p.account_hint.take() else {
            continue;
        };
        p.account_hint = ground_hint(&hint, &known);
        if p.account_hint.is_none() && hint.starts_with("Expenses:") {
            if payee_account.is_none() {
                payee_account = Some(past_payee_account(db, description).await);
            }
            p.account_hint = payee_account.clone().flatten();
        }
    }
}

/// The one spending account past entries for this payee used, if exactly one.
async fn past_payee_account(db: &Database, description: &str) -> Option<String> {
    let payee = payee_word(description)?;
    let used: BTreeSet<String> = similar_entries(db, description)
        .await
        .into_iter()
        .filter(|(_, d, _)| payee_word(d).as_deref() == Some(payee.as_str()))
        .flat_map(|(_, _, accounts)| accounts)
        .filter(|a| a.starts_with("Expenses:"))
        .collect();
    let mut used = used.into_iter();
    match (used.next(), used.next()) {
        (Some(one), None) => Some(one),
        _ => None,
    }
}

/// Past entries whose description shares words with `description`, newest
/// first, as (date, description, accounts).
async fn similar_entries(db: &Database, description: &str) -> Vec<(String, String, Vec<String>)> {
    let Ok(mut resp) = db
        .query(
            "SELECT date, description, postings, created_at FROM transactions
             WHERE removed = false AND superseded_by IS NONE AND description @0,OR@ $q
             ORDER BY date DESC, created_at DESC LIMIT $n",
        )
        .bind(("q", description.to_string()))
        .bind(("n", PAST_ENTRIES))
        .await
    else {
        return Vec::new();
    };
    #[derive(SurrealValue)]
    struct Row {
        date: String,
        description: String,
        postings: surrealdb::types::Value,
    }
    let rows: Vec<Row> = resp.take(0).unwrap_or_default();
    rows.into_iter()
        .map(|r| {
            let accounts = match r.postings.into_json_value() {
                Value::Array(ps) => ps
                    .iter()
                    .filter_map(|p| p.get("account")?.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            };
            (r.date, r.description, accounts)
        })
        .collect()
}

/// The first word of a description, lowercased: the part a payee keeps.
fn payee_word(description: &str) -> Option<String> {
    description
        .split_whitespace()
        .next()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| w.len() > 2)
}

/// Check a `transaction.record` entry against the ledger. `Err` is the JSON the
/// model gets back instead of a proposal.
pub async fn check_record(db: &Database, entry: &str) -> Result<(), Value> {
    let parsed = match crate::journal_import::parse_one_entry(entry) {
        Ok(p) => p,
        // Syntax was already checked by the action's own validator.
        Err(_) => return Ok(()),
    };
    let known = known_accounts(db).await;
    let accounts: Vec<&str> = parsed.postings.iter().map(|p| p.account.as_str()).collect();
    let known_refs: std::collections::HashSet<&str> = known.iter().map(String::as_str).collect();
    let unknown = crate::accounts::unknown_accounts(accounts.iter().copied(), &known_refs);
    if !unknown.is_empty() {
        let suggestions: Vec<Value> = unknown
            .iter()
            .map(|a| json!({ "account": a, "closest": closest(a, &known) }))
            .collect();
        return Err(json!({
            "error": "these accounts are not in this ledger; use an existing account, and if \
                      none fits, tell the user rather than inventing one",
            "unknown": suggestions,
        }));
    }

    // A payee seen before keeps its spending account unless he says otherwise.
    let Some(payee) = payee_word(&parsed.description) else {
        return Ok(());
    };
    let past: Vec<_> = similar_entries(db, &parsed.description)
        .await
        .into_iter()
        .filter(|(_, d, _)| payee_word(d).as_deref() == Some(payee.as_str()))
        .collect();
    let spending = |accts: &[String]| -> BTreeSet<String> {
        accts
            .iter()
            .filter(|a| a.starts_with("Expenses:") || a.starts_with("Income:"))
            .cloned()
            .collect()
    };
    let mine = spending(&accounts.iter().map(|a| a.to_string()).collect::<Vec<_>>());
    let used_before: BTreeSet<String> = past.iter().flat_map(|(_, _, a)| spending(a)).collect();
    if !used_before.is_empty() && mine.is_disjoint(&used_before) {
        let examples: Vec<Value> = past
            .iter()
            .map(|(d, desc, a)| json!({ "date": d, "description": desc, "accounts": a }))
            .collect();
        return Err(json!({
            "error": "this payee was recorded before under a different account; follow the \
                      past entries below unless the user asked for a different one",
            "past_entries": examples,
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_names_are_read_out_of_prose_and_lists() {
        let text = "Use `Expenses:Food:Dining` for meals. Cash is Assets:NonRegistered:CAD, \
                    tagged product: wallet.\n- Income:Employment:Salary\nNot an account: Note:This.";
        let got = accounts_named_in(text);
        let want: BTreeSet<String> = [
            "Assets:NonRegistered:CAD",
            "Expenses:Food:Dining",
            "Income:Employment:Salary",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn the_closest_accounts_share_the_top_level_and_the_most_words() {
        let known: BTreeSet<String> = [
            "Expenses:Food:Dining",
            "Expenses:Food:Groceries",
            "Expenses:Transport:RideShare",
            "Assets:NonRegistered:CAD",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let got = closest("Expenses:Food", &known);
        assert_eq!(
            &got[..2],
            ["Expenses:Food:Dining", "Expenses:Food:Groceries"]
        );
        assert!(!got.contains(&"Assets:NonRegistered:CAD"));
        assert_eq!(
            closest("Expenses:Transportation:Rideshare", &known)[0],
            "Expenses:Transport:RideShare"
        );
    }

    #[test]
    fn an_importer_hint_is_kept_mapped_or_dropped() {
        let known: BTreeSet<String> = [
            "Expenses:Food:Groceries",
            "Expenses:Transport:RideShare",
            "Expenses:Tax:Income",
            "Expenses:Tax:Transaction",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let g = |h: &str| ground_hint(h, &known);
        assert_eq!(
            g("Expenses:Food:Groceries").as_deref(),
            Some("Expenses:Food:Groceries")
        );
        assert_eq!(
            g("Expenses:Groceries").as_deref(),
            Some("Expenses:Food:Groceries")
        );
        assert_eq!(
            g("Expenses:Transportation:Rideshare").as_deref(),
            Some("Expenses:Transport:RideShare")
        );
        assert_eq!(g("Expenses:Coffee"), None, "nothing like it");
        assert_eq!(g("Assets:Groceries"), None, "a different top level");
    }

    #[test]
    fn the_payee_is_the_first_real_word() {
        assert_eq!(payee_word("Tim Hortons lunch").as_deref(), Some("tim"));
        assert_eq!(payee_word("UBER *TRIP").as_deref(), Some("uber"));
        assert_eq!(payee_word("A b"), None);
    }

    #[tokio::test]
    async fn an_invented_account_is_refused_with_the_closest_real_ones() {
        let db = crate::db::test_db().await;
        use crate::events::Projection;
        crate::events::BudgetProjection
            .init_schema(&db)
            .await
            .unwrap();
        crate::events::NotesProjection
            .init_schema(&db)
            .await
            .unwrap();
        db.query(
            "CREATE generic_notes:conv SET title = 'Ledger conventions', \
             raw_text = 'Meals: Expenses:Food:Dining. Cash: Assets:NonRegistered:CAD.', \
             tags = [], created_at = time::now(), updated_at = time::now()",
        )
        .await
        .unwrap();
        let entry = "2026-10-11 Tim Hortons lunch\n    Expenses:Food  14.50 CAD\n    \
                     Assets:Cash  -14.50 CAD\n";
        let err = check_record(&db, entry).await.unwrap_err();
        let unknown = err["unknown"].as_array().unwrap();
        assert_eq!(unknown.len(), 2);
        assert_eq!(unknown[0]["closest"][0], "Expenses:Food:Dining");

        let ok = "2026-10-11 Tim Hortons lunch\n    Expenses:Food:Dining  14.50 CAD\n    \
                  Assets:NonRegistered:CAD  -14.50 CAD\n";
        assert!(check_record(&db, ok).await.is_ok());
    }

    /// Receipt import runs on the server, so the server's own projections must
    /// hold what grounding reads. They once held documents only, and every
    /// account an emailed receipt named was dropped.
    #[tokio::test]
    async fn the_server_can_see_the_conventions_its_receipts_are_held_to() {
        use crate::events::{EventStore, NewEvent, ProjectionRunner, SurrealEventStore, registry};
        let db = crate::db::test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), registry::build_projections_server());
        runner.init_all().await.unwrap();
        let note = store
            .append(NewEvent {
                id: None,
                event_type: "generic_note_created".into(),
                aggregate_id: "01JKNOTE0000000000000CONV1".into(),
                timestamp: chrono::Utc::now(),
                device_id: "d1".into(),
                payload: json!({
                    "note_id": "01JKNOTE0000000000000CONV1",
                    "title": "Ledger conventions",
                    "raw_text": "Restaurants: Expenses:Food:Dining.",
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[note]).await.unwrap();

        assert_eq!(spending_accounts(&db).await, vec!["Expenses:Food:Dining"]);
    }

    #[tokio::test]
    async fn an_ungrounded_receipt_line_takes_the_payees_past_account() {
        let db = crate::db::test_db().await;
        use crate::events::Projection;
        crate::events::BudgetProjection
            .init_schema(&db)
            .await
            .unwrap();
        crate::events::NotesProjection
            .init_schema(&db)
            .await
            .unwrap();
        db.query(
            "CREATE generic_notes:conv SET title = 'Ledger conventions', \
             raw_text = 'Domains: Expenses:Business:Domains. Fees: Expenses:Fees.', \
             tags = [], created_at = time::now(), updated_at = time::now();
             CREATE transactions:p CONTENT { removed: false, superseded_by: NONE, \
             date: '2026-01-05', description: 'Porkbun renewal', cleared: false, \
             postings: [{ account: 'Expenses:Business:Domains', amount: '11.08', commodity: 'USD' }], \
             tags_top: [], merged_ids: [], created_at: time::now(), updated_at: time::now() };",
        )
        .await
        .unwrap();
        assert_eq!(
            spending_accounts(&db).await,
            vec!["Expenses:Business:Domains", "Expenses:Fees"]
        );
        let line = |hint: &str| crate::extraction::ExtractedPosting {
            account_hint: Some(hint.to_string()),
            commodity: "USD".to_string(),
            amount: rust_decimal::Decimal::ONE,
            line_label: None,
        };
        let mut postings = vec![line("Expenses:Web:Registration"), line("Expenses:Fees")];
        ground_hints(&db, "Porkbun", &mut postings).await;
        assert_eq!(
            postings[0].account_hint.as_deref(),
            Some("Expenses:Business:Domains")
        );
        assert_eq!(postings[1].account_hint.as_deref(), Some("Expenses:Fees"));

        let mut unknown_payee = vec![line("Expenses:Web:Registration")];
        ground_hints(&db, "Namecheap", &mut unknown_payee).await;
        assert_eq!(unknown_payee[0].account_hint, None);
    }
}
