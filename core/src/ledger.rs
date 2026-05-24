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

use ledger_parser::Ledger;
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
    let simplified = SimplifiedLedger::try_from(ledger)
        .map_err(|e| LedgerError::Balance(format!("simplified ledger: {e}")))?;
    Ok(Balance::from(&simplified))
}

fn prep_content(content: &str) -> String {
    let mut out = content
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
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
