//! Durability guardrail 1 — CI golden-reconcile.
//!
//! Imports a small **synthetic** fixture journal (fictional banks) through the
//! real production reconcile pipeline and asserts the projected per-account,
//! per-commodity balances against a **frozen expected table**. Any future
//! parser / grammar / renderer / balance-engine drift that moves a balance fails
//! this test in CI (it rides the existing `cargo test -p omni-me-core` step — no
//! workflow change, no external `ledger` binary needed).
//!
//! Two independent computations guard the whole chain. Path A is the full
//! production path: `parse_journal` -> the canonical `TransactionRecordedPayload::new`
//! builder -> `journal_file::render_transaction` -> `ledger::balances` (the exact
//! library the app uses to evaluate balances). Path B is a direct per-posting sum
//! of the parsed drafts, bypassing the render/re-parse round-trip. Path A == frozen
//! catches render/balance drift; Path B == frozen catches parser/elision drift.
//!
//! The frozen table is hand-derived (simple per-account sums, eyeball-verifiable on
//! the tiny fixture) and was cross-checked once locally against the `ledger` binary
//! bundled with paisa.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use chrono::NaiveDate;
use omni_me_core::accounts::unmatched_posting;
use omni_me_core::events::{Posting, TransactionRecordedPayload};
use omni_me_core::reconciliation::combine_for_merge;
use omni_me_core::journal_file::render_transaction;
use omni_me_core::journal_import::parse_journal;
use rust_decimal::Decimal;

type BalanceTable = BTreeMap<(String, String), String>;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden/main.ledger")
}

/// The authoritative per-(account, commodity) balances for the fixture, derived
/// by hand from the seven transactions. Values are normalized decimal strings so
/// scale (`42.10` vs `42.1`) never causes a spurious mismatch.
fn expected() -> BalanceTable {
    [
        ("Assets:NonRegistered:CAD", "CAD", "3580.90"),
        ("Assets:NonRegistered:USD", "USD", "100.00"),
        ("Assets:NonRegistered:NWG", "NWG", "6"),
        ("Liabilities:Credit Card:CAD", "CAD", "-5.25"),
        ("Income:Employment:Salary", "CAD", "-3000.00"),
        ("Expenses:Groceries", "CAD", "42.10"),
        ("Expenses:Coffee", "CAD", "5.25"),
        ("Equity:OpeningBalance", "CAD", "-1000.00"),
    ]
    .into_iter()
    .map(|(acct, com, amt)| {
        (
            (acct.to_string(), com.to_string()),
            norm(Decimal::from_str(amt).unwrap()),
        )
    })
    .collect()
}

/// Normalize a decimal to a scale-independent string (`-1000.00` -> `-1000`).
fn norm(d: Decimal) -> String {
    d.normalize().to_string()
}

/// Render payloads through the production journal writer and read the balances
/// back with the same library the app uses. Zero balances are dropped so an
/// account that nets out (an `Unmatched` pair, say) doesn't show up as a
/// difference between two otherwise identical tables — the same convention
/// Path B has always applied to its direct sum.
fn render_and_balance(payloads: &[TransactionRecordedPayload]) -> BalanceTable {
    let mut journal = String::new();
    for payload in payloads {
        journal.push_str(&render_transaction(payload));
        journal.push('\n');
    }
    let balance = omni_me_core::ledger::balances(&journal).expect("rendered journal balances");
    let mut out = BalanceTable::new();
    for (account, ab) in &balance.account_balances {
        for (commodity, amount) in &ab.amounts {
            if amount.quantity.is_zero() {
                continue;
            }
            out.insert((account.clone(), commodity.clone()), norm(amount.quantity));
        }
    }
    out
}

#[test]
fn golden_reconcile_matches_frozen_balances() {
    let imported = parse_journal(&fixture_path()).expect("fixture parses");
    assert!(
        imported.parse_errors.is_empty(),
        "fixture must parse cleanly: {:?}",
        imported.parse_errors
    );
    assert!(
        imported.balance_failures.is_empty(),
        "fixture must balance cleanly: {:?}",
        imported.balance_failures
    );

    // Path B — direct sum of the parsed drafts' postings (independent of the
    // render/re-parse round-trip).
    let mut direct: BTreeMap<(String, String), Decimal> = BTreeMap::new();
    for txn in &imported.transactions {
        for p in &txn.postings {
            *direct
                .entry((p.account.clone(), p.commodity.clone()))
                .or_default() += p.amount;
        }
    }
    let path_b: BalanceTable = direct
        .into_iter()
        .filter(|(_, q)| !q.is_zero())
        .map(|(k, q)| (k, norm(q)))
        .collect();

    // Path A — the full production path: canonical builder -> render -> the app's
    // balance library.
    let payloads: Vec<TransactionRecordedPayload> = imported
        .transactions
        .iter()
        .map(|txn| {
            TransactionRecordedPayload::new(
                txn.txn_id.clone(),
                txn.date,
                txn.description.clone(),
                txn.postings.clone(),
            )
            .with_tags(txn.top_tags.clone())
        })
        .collect();
    let path_a = render_and_balance(&payloads);

    let frozen = expected();
    assert_eq!(
        path_b, frozen,
        "parser/elision drift: parsed drafts no longer sum to the frozen balances"
    );
    assert_eq!(
        path_a, frozen,
        "render/balance drift: the full reconcile path no longer reproduces the frozen balances"
    );
}

/// Merging two halves of a reconciliation must not move a single balance.
///
/// `merge_transactions` strips both `Unmatched` legs and concatenates the rest.
/// The resulting `TransactionsMerged` event is replayed into the SurrealDB
/// projection *and* the on-disk hledger file, and nothing downstream re-derives
/// what the balances ought to be — so a merge that drops or duplicates a leg
/// surfaces only as numbers that quietly disagree with the bank.
///
/// This renders the same money twice through the production writer: once as the
/// two unreconciled halves, once as the merged entry. The balance library must
/// reach the same table both times. It is the end-to-end counterpart to the
/// unit tests on `combine_for_merge` and `plan_merge` — those check the
/// arithmetic, this checks that the arithmetic survives a render / re-parse
/// round-trip, which is where the money chain has drifted before.
#[test]
fn merging_two_halves_leaves_every_balance_unchanged() {
    let date = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
    let amount = Decimal::from_str("42.10").unwrap();

    // The statement half: money left the account, the other side is unknown.
    let statement = vec![
        Posting {
            account: "Assets:NonRegistered:CAD".to_string(),
            commodity: "CAD".to_string(),
            amount: -amount,
            fx_rate: None,
            tags: vec![],
        },
        unmatched_posting(amount, "CAD"),
    ];
    // The manual half: the user recorded what it was for.
    let manual = vec![
        Posting {
            account: "Expenses:Groceries".to_string(),
            commodity: "CAD".to_string(),
            amount,
            fx_rate: None,
            tags: vec![],
        },
        unmatched_posting(-amount, "CAD"),
    ];

    let unreconciled = vec![
        TransactionRecordedPayload::new(
            "merge-a".to_string(),
            date,
            "NORTHWIND WITHDRAWAL".to_string(),
            statement.clone(),
        ),
        TransactionRecordedPayload::new(
            "merge-b".to_string(),
            date,
            "groceries".to_string(),
            manual.clone(),
        ),
    ];

    let merged = vec![TransactionRecordedPayload::new(
        "merge-a".to_string(),
        date,
        "groceries".to_string(),
        combine_for_merge(&statement, &manual),
    )];

    let before = render_and_balance(&unreconciled);
    let after = render_and_balance(&merged);

    assert_eq!(
        after, before,
        "merging moved a balance: the two halves and the merged entry must \
         reconcile to the same table"
    );
    // And the pair really did net out, rather than both tables being empty.
    assert_eq!(
        before.get(&("Expenses:Groceries".to_string(), "CAD".to_string())),
        Some(&norm(amount)),
        "fixture is not exercising the balances it claims to"
    );
    // Read this off the rendered text, not off `after`. A pair of leftover
    // `Unmatched` legs cancels to zero, and `render_and_balance` drops zero
    // balances — so the balance table cannot see the difference between a
    // merge that stripped them and one that forgot to.
    let merged_journal = render_transaction(&merged[0]);
    assert!(
        !merged_journal.contains("Unmatched"),
        "the merged entry still carries an Unmatched leg:\n{merged_journal}"
    );
}
