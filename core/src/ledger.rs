//! In-process pluggable-text-accounting (PTA) engine — wraps `ledger-parser` +
//! `ledger-utils` so downstream features can compute balances and run queries
//! without shelling out to hledger.
//!
//! Validated against the user's real 5,826-transaction production journal in
//! POC 0.1b (desktop) and POC 0.1c (Android arm64); both produced byte-identical
//! results — see project.md session log entries for 2026-05-09.
//!
//! Used by:
//! - Phase 4 R1 financial-health dashboard (balance aggregation across accounts).
//! - Phase 5.7 unified reconciliation review (`Unmatched`-account balance check).
//! - Phase 5.8 statement-reconciliation balance check.
//! - Phase 7.2 R2 filter DSL.
//!
//! Scope deliberately stays *read-side*. Writes go through the event store +
//! journal-file projection.

use ledger_parser::{Ledger, LedgerItem};
use ledger_utils::balance::Balance;
use ledger_utils::simplified_ledger::Ledger as SimplifiedLedger;

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger parse error: {0}")]
    Parse(String),
    #[error("ledger panic during parse: {0}")]
    ParserPanic(String),
    #[error("ledger balance computation failed: {0}")]
    Balance(String),
}

/// Parse a single-file hledger journal. Applies the POC 0.1b content-prep
/// workaround (`trim_end` per line + trailing `"\n\n"`) which is needed because
/// the underlying nom parser can return Incomplete on real-world files that
/// don't end with a blank line.
///
/// Catches parser panics so a malformed input from a future projection bug
/// doesn't take down the calling Tauri command.
pub fn parse(content: &str) -> Result<Ledger, LedgerError> {
    let prepped = prep_content(content);
    let result = std::panic::catch_unwind(|| ledger_parser::parse(&prepped));
    match result {
        Ok(Ok(ledger)) => Ok(ledger),
        Ok(Err(e)) => Err(LedgerError::Parse(format!("{e:?}"))),
        Err(panic) => {
            let msg = panic
                .downcast_ref::<&'static str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err(LedgerError::ParserPanic(msg))
        }
    }
}

/// Compute per-account balances (one `Amount` per commodity per account).
/// Wraps `SimplifiedLedger` + `Balance` so callers don't have to know the
/// two-step conversion lives inside `ledger-utils`.
pub fn balances(content: &str) -> Result<Balance, LedgerError> {
    let ledger = parse(content)?;
    // `SimplifiedLedger` enforces per-transaction balance by *raw* amount — it
    // ignores `@`/`@@` cost and only tolerates a single 2-commodity exchange.
    // Real `ledger` cost-balances, so legitimate cost-annotated entries it
    // accepts — notably zero-cost crypto acquisitions (`0.000088 ETH @@ 0.00
    // CAD`, where the cash leg is 0) — make `SimplifiedLedger` reject the whole
    // journal as unbalanced. `ledger bal` itself is just a per-account,
    // per-commodity sum of posting amounts, so when the strict path rejects we
    // fall back to that: identical results for ordinary journals, correct
    // (ledger-faithful) results for the cost-balanced ones. The rendered journal
    // always has explicit amounts, so no elision is needed here.
    match SimplifiedLedger::try_from(ledger.clone()) {
        Ok(simplified) => Ok(Balance::from(&simplified)),
        Err(_) => Ok(raw_balances(&ledger)),
    }
}

/// Per-account, per-commodity sum of explicit posting amounts — exactly what
/// `ledger bal` reports. Amount-less postings are ignored (the JournalFile
/// projection always renders explicit amounts, so they never occur here).
fn raw_balances(ledger: &Ledger) -> Balance {
    let mut balance = Balance::new();
    for item in &ledger.items {
        if let LedgerItem::Transaction(t) = item {
            for posting in &t.postings {
                if let Some(pa) = &posting.amount {
                    // `AddAssign<&Amount>` accumulates per commodity and drops
                    // entries that net to zero.
                    *balance
                        .account_balances
                        .entry(posting.account.clone())
                        .or_default() += &pa.amount;
                }
            }
        }
    }
    // Drop accounts whose every commodity netted to zero, matching `ledger bal`.
    balance
        .account_balances
        .retain(|_, ab| !ab.amounts.is_empty());
    balance
}

fn prep_content(content: &str) -> String {
    // `ledger-utils` (ledger_parser) parses transactions but errors on `account`
    // directives. The JournalFile projection appends an
    // `account <name>  ; commodity:<c>` block (plus an optional indented
    // `note <label>` sub-directive) for every per-account override
    // (rename / hide / liquid). Those carry no balance information — overrides
    // live in the DB — so strip each `account` block before parsing. Otherwise a
    // single override makes the whole journal unparseable and collapses every
    // balance view (net worth, the Accounts screen, the detected-account list).
    let mut kept: Vec<&str> = Vec::new();
    let mut in_account_block = false;
    for line in content.lines() {
        if in_account_block {
            // Sub-directives of an `account` block are indented; the block ends
            // at the first non-indented line (blank, a new directive, or a txn).
            if line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
                continue;
            }
            in_account_block = false;
        }
        if line.starts_with("account ") {
            in_account_block = true;
            continue;
        }
        kept.push(line.trim_end());
    }
    let mut out = kept.join("\n");
    out.push_str("\n\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    const SIMPLE_JOURNAL: &str = "\
2026-05-16 Coffee
    Assets:Cash             -5.25 CAD
    Expenses:Coffee          5.25 CAD

2026-05-16 Groceries
    Assets:Cash            -87.42 CAD
    Expenses:Groceries      87.42 CAD
";

    #[test]
    fn parses_simple_journal() {
        let ledger = parse(SIMPLE_JOURNAL).unwrap();
        // 2 transactions + 2 empty-line items (between/after) — exact item count
        // depends on parser version; what matters is "no error and at least one
        // transaction".
        let txn_count = ledger
            .items
            .iter()
            .filter(|i| matches!(i, ledger_parser::LedgerItem::Transaction(_)))
            .count();
        assert_eq!(txn_count, 2);
    }

    #[test]
    fn balances_simple_journal() {
        let bal = balances(SIMPLE_JOURNAL).unwrap();
        let cash = bal
            .account_balances
            .get("Assets:Cash")
            .expect("Assets:Cash balance present");
        let cad = cash.amounts.get("CAD").expect("CAD commodity");
        // -5.25 + -87.42 = -92.67
        assert_eq!(cad.quantity, Decimal::from_str("-92.67").unwrap());

        let groceries = bal
            .account_balances
            .get("Expenses:Groceries")
            .expect("Expenses:Groceries balance present");
        let cad = groceries.amounts.get("CAD").unwrap();
        assert_eq!(cad.quantity, Decimal::from_str("87.42").unwrap());
    }

    #[test]
    fn balances_skips_account_directives() {
        // The JournalFile projection appends `account <name>  ; commodity:<c>`
        // blocks (with an optional indented `note`) for per-account overrides
        // (rename / hide / liquid). ledger-utils can't parse them, so prep_content
        // strips them — otherwise one override makes the whole journal
        // unparseable and collapses every balance view. Synthetic data only.
        let journal = "\
2026-05-16 Coffee
    Assets:Cash             -5.25 CAD
    Expenses:Coffee          5.25 CAD

account Assets:Cash  ; commodity:CAD
    note Spending cash

account Assets:Cash  ; commodity:CAD
";
        let bal = balances(journal).unwrap();
        let cash = bal
            .account_balances
            .get("Assets:Cash")
            .expect("Assets:Cash balance present despite account directives");
        assert_eq!(
            cash.amounts.get("CAD").unwrap().quantity,
            Decimal::from_str("-5.25").unwrap()
        );
        assert!(bal.account_balances.contains_key("Expenses:Coffee"));
    }

    #[test]
    fn parse_handles_missing_trailing_blank_line() {
        // No trailing newline — POC 0.1b found ledger-parser's nom parser
        // returns Incomplete here without the prep wrapper.
        let trimmed = "2026-05-16 Coffee\n    Assets:Cash    -5.25 CAD\n    Expenses:Coffee  5.25 CAD";
        assert!(parse(trimmed).is_ok());
    }

    #[test]
    fn parse_handles_trailing_whitespace_per_line() {
        let with_trailing_space = "\
2026-05-16 Coffee
    Assets:Cash             -5.25 CAD
    Expenses:Coffee          5.25 CAD
";
        assert!(parse(with_trailing_space).is_ok());
    }

    #[test]
    fn parse_returns_error_on_malformed_input() {
        let bad = "this is not a ledger journal\nnothing parses here\n";
        // ledger-parser returns Err on free-form text — the wrapper surfaces it
        // as LedgerError::Parse rather than panicking.
        let result = parse(bad);
        assert!(result.is_err(), "free-form text should fail to parse");
    }

    #[test]
    fn balances_handles_multi_commodity_account() {
        let multi = "\
2026-05-16 Crypto trade
    Assets:Crypto              0.001 BTC
    Assets:Cash             -67.50 CAD

2026-05-16 Refund
    Assets:Cash              10.00 CAD
    Income:Refund           -10.00 CAD
";
        let bal = balances(multi).unwrap();
        let cash = bal.account_balances.get("Assets:Cash").unwrap();
        let cad = cash.amounts.get("CAD").unwrap();
        // -67.50 + 10.00 = -57.50
        assert_eq!(cad.quantity, Decimal::from_str("-57.50").unwrap());
        let crypto = bal.account_balances.get("Assets:Crypto").unwrap();
        assert_eq!(
            crypto.amounts.get("BTC").unwrap().quantity,
            Decimal::from_str("0.001").unwrap()
        );
    }

    #[test]
    fn zero_cost_single_commodity_falls_back_to_raw_sum() {
        // A zero-cost crypto acquisition: the cash leg is 0, so the entry has a
        // single non-zero commodity. ledger-utils' strict balancer rejects it,
        // but `ledger bal` sums it fine — the fallback must reproduce that.
        let journal = "\
2022-08-01 ETH buy
    Assets:NonRegistered:ETH   0.000088 ETH

2022-09-01 Coffee
    Assets:Cash    -5.25 CAD
    Expenses:Coffee 5.25 CAD
";
        let bal = balances(journal).unwrap();
        let eth = bal
            .account_balances
            .get("Assets:NonRegistered:ETH")
            .expect("ETH account present via raw-sum fallback");
        assert_eq!(
            eth.amounts.get("ETH").unwrap().quantity,
            Decimal::from_str("0.000088").unwrap()
        );
        // The ordinary transaction in the same journal still sums correctly.
        let cash = bal.account_balances.get("Assets:Cash").unwrap();
        assert_eq!(
            cash.amounts.get("CAD").unwrap().quantity,
            Decimal::from_str("-5.25").unwrap()
        );
    }

    #[test]
    fn round_trips_through_journal_file_renderer() {
        // Reads back what `journal_file::render_transaction` produces. If the
        // renderer ever drifts from a format ledger-parser accepts, this test
        // surfaces it before Phase 4 dashboards start lying to the user.
        use crate::events::{Posting, TransactionRecordedPayload};
        use chrono::NaiveDate;

        let payload = TransactionRecordedPayload {
            txn_id: "01JKTXN".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Coffee".into(),
            postings: vec![
                Posting {
                    account: "Assets:Cash".into(),
                    commodity: "CAD".into(),
                    amount: Decimal::from_str("-5.25").unwrap(),
                    fx_rate: None,
                    tags: vec![],
                },
                Posting {
                    account: "Expenses:Coffee".into(),
                    commodity: "CAD".into(),
                    amount: Decimal::from_str("5.25").unwrap(),
                    fx_rate: None,
                    tags: vec![],
                },
            ],
            tags: vec![],
            attachment: None,
            statement_source: None,
        };
        let rendered = crate::journal_file::render_transaction(&payload);
        let bal = balances(&rendered).unwrap();
        let cash = bal.account_balances.get("Assets:Cash").unwrap();
        assert_eq!(
            cash.amounts.get("CAD").unwrap().quantity,
            Decimal::from_str("-5.25").unwrap()
        );
    }
}
