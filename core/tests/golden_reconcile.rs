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

use omni_me_core::events::TransactionRecordedPayload;
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
    let mut journal = String::new();
    for txn in &imported.transactions {
        let payload = TransactionRecordedPayload::new(
            txn.txn_id.clone(),
            txn.date,
            txn.description.clone(),
            txn.postings.clone(),
        )
        .with_tags(txn.top_tags.clone());
        journal.push_str(&render_transaction(&payload));
        journal.push('\n');
    }
    let balance = omni_me_core::ledger::balances(&journal).expect("rendered journal balances");
    let mut path_a: BalanceTable = BTreeMap::new();
    for (account, ab) in &balance.account_balances {
        for (commodity, amount) in &ab.amounts {
            path_a.insert((account.clone(), commodity.clone()), norm(amount.quantity));
        }
    }

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
