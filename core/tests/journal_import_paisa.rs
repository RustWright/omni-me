//! Phase 6.4 — Pre-cleanup import test against `.reference/paisa/`.
//!
//! Exercises `journal_import::parse_journal` against the user's real
//! historical hledger journal (~5,826 transactions through Sept 2025,
//! validated through POC 0.1b). Runs the full include-glob walk, the
//! elision-filling logic, and the A2 rewriter against real-world-messy
//! data before the external cleanup pass touches it.
//!
//! `#[ignore]`-gated by default — depends on `.reference/paisa/main.ledger`
//! being present on the developer's machine. Skips gracefully when absent
//! so CI doesn't fail. Run with:
//!
//! ```bash
//! cargo test -p omni-me-core --test journal_import_paisa -- --ignored
//! ```
//!
//! Companion to Phase 6.5 (post-cleanup test), which will run against the
//! user's cleaned journal once the external cleanup session is done.

use std::collections::HashSet;
use std::path::PathBuf;

use omni_me_core::events::Tag;
use omni_me_core::journal_import::{apply_a2_rewriter, parse_journal};

fn paisa_root() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .join(".reference/paisa/main.ledger");
    if root.exists() {
        Some(root)
    } else {
        eprintln!(
            "skipping: .reference/paisa/main.ledger not found at {} \
             — Phase 6.4 needs the user's real journal to run.",
            root.display()
        );
        None
    }
}

#[test]
#[ignore = "needs .reference/paisa/ on disk"]
fn parses_paisa_journal_at_scale() {
    let Some(root) = paisa_root() else { return };
    let imported =
        parse_journal(&root).expect("parse_journal should never hard-error on Some(path)");

    // POC 0.1b validated 5,826 transactions on this same dataset. The exact
    // count depends on whether the journal has grown since; assert a wide
    // lower bound + report the actual numbers for diagnostic value.
    eprintln!(
        "files_parsed = {}, transactions = {}, accounts = {}, commodities = {}, parse_errors = {}, balance_failures = {}",
        imported.files_parsed,
        imported.transactions.len(),
        imported.per_account.len(),
        imported.commodities.len(),
        imported.parse_errors.len(),
        imported.balance_failures.len(),
    );

    assert!(
        imported.files_parsed >= 50,
        "expected to walk into many included files, got {}",
        imported.files_parsed
    );
    assert!(
        imported.transactions.len() >= 5_000,
        "expected at least 5k transactions, got {}",
        imported.transactions.len()
    );
    assert!(
        imported.per_account.len() >= 10,
        "expected the chart of accounts to have at least 10 accounts"
    );

    // Per-file parse errors should be rare on a journal POC 0.1b already
    // validated. Surface them but don't fail — Phase 6 lives downstream of
    // the user's external cleanup, which will fix any anomalies.
    for err in imported.parse_errors.iter().take(5) {
        eprintln!("parse error: {}: {}", err.path.display(), err.message);
    }
    for fail in imported.balance_failures.iter().take(5) {
        eprintln!("balance failure: {fail}");
    }

    // Every transaction must have a stable id + non-empty postings.
    let mut ids: HashSet<&str> = HashSet::new();
    for txn in &imported.transactions {
        assert!(!txn.postings.is_empty());
        assert!(txn.txn_id.starts_with("import-"));
        assert!(ids.insert(&txn.txn_id), "duplicate txn_id surfaced");
    }
}

#[test]
#[ignore = "needs .reference/paisa/ on disk"]
fn paisa_a2_rewriter_runs_clean() {
    let Some(root) = paisa_root() else { return };
    let mut imported = parse_journal(&root).expect("parse_journal should succeed");
    let before_business_accounts = imported
        .per_account
        .iter()
        .filter(|p| p.account.starts_with("Expenses:Business:"))
        .count();

    let rewritten = apply_a2_rewriter(&mut imported.transactions);
    eprintln!(
        "A2 rewriter rewrote {rewritten} postings; {before_business_accounts} Expenses:Business:* accounts were present pre-rewrite"
    );

    // Post-rewrite, no posting should still carry the legacy prefix.
    for txn in &imported.transactions {
        for posting in &txn.postings {
            assert!(
                !posting.account.starts_with("Expenses:Business:"),
                "leftover Business prefix on {} in txn {}",
                posting.account,
                txn.txn_id
            );
        }
    }

    // If any business postings were rewritten, at least one should now carry
    // the `type:business` tag.
    if rewritten > 0 {
        let any_tagged = imported
            .transactions
            .iter()
            .flat_map(|t| &t.postings)
            .any(|p| {
                p.tags.iter().any(|t| {
                    matches!(t, Tag::KeyValue { key, value } if key == "type" && value == "business")
                })
            });
        assert!(any_tagged, "rewriter ran but no type:business tag landed");
    }
}
