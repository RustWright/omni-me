//! Account-balance aggregation for the Phase 4.4 account-list screen.
//!
//! Pulls together three sources:
//! - The per-device journal file (source of truth for postings + `P`
//!   directives) — fed through [`ledger::balances`] for per-(account,
//!   commodity) quantities.
//! - The `accounts` projection table — for declared-account metadata
//!   (`display_name`, `last_reconciled_through`, `last_statement_balance`).
//! - `ledger-utils::Prices` over the same parsed journal — for converting
//!   foreign-commodity balances into the user's base currency.
//!
//! The journal's `P` directives come from two converging paths:
//! - Frankfurter daily-rate fetcher writes CAD/USD/EUR (Phase 2.7).
//! - Auto-import batch commit writes manual NGN rates entered at review
//!   time (Phase 3.10.5).
//!
//! Both paths land as the same hledger `P` directive shape, so this module
//! consumes them uniformly through `Prices::insert_from`.
//!
//! Account-set policy lives in [`is_listable_account`] — see that fn for the
//! list rule (which prefixes count as "an account" for display purposes).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use ledger_utils::prices::Prices;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::db::queries::AccountRow;
use crate::ledger::{self, LedgerError};

/// One commodity balance on an account, optionally with its base-currency value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommodityBalance {
    pub commodity: String,
    pub quantity: Decimal,
    /// `Some` when conversion succeeded (commodity == base, or a `P`
    /// directive supplies the rate). `None` when no rate is available — the
    /// UI shows the native amount and skips the row in the aggregated total.
    pub value_in_base: Option<Decimal>,
}

/// One account on the Accounts screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountSummary {
    pub account: String,
    pub display_name: Option<String>,
    pub last_reconciled_through: Option<String>,
    pub last_statement_balance: Option<String>,
    /// One row per commodity held, sorted by commodity name for determinism
    /// (POC 0.1c finding — HashMap iteration order is non-deterministic).
    pub balances: Vec<CommodityBalance>,
    /// Sum of `value_in_base` across all balances where conversion succeeded.
    /// `None` when no balance was convertible — caller renders a "—" badge.
    pub total_in_base: Option<Decimal>,
}

/// Explicit allowlist of accounts that surface on the Accounts screen.
/// Edit this list when an account graduates from "appears in postings" to
/// "I want to see its balance at a glance" (or vice versa).
///
/// Drop-by-default: any account name not in this list is filtered out, so
/// new auto-import-discovered accounts never appear on the screen silently.
///
/// `Unmatched` lives here too — it's the one-word top-level clearing
/// account from `project_unmatched_account_pattern.md`, and its non-zero
/// balance is the reconciliation-pending signal that Phase 4.5 will
/// surface as its own dashboard widget.
///
/// Cycle 4 plans to lift this list into the `accounts` SurrealDB table
/// (driven by a Settings UI that emits `AccountAdded` / future
/// `AccountRemoved` events). Until then it's a code constant.
pub const LISTABLE_ACCOUNTS: &[&str] = &[
    "Assets:Wealthsimple:Cash",
    "Assets:Wise:CAD",
    "Liabilities:CIBC:CreditCard",
    "Unmatched",
];

/// Decide whether `account` should surface on the Accounts screen.
/// Pure lookup against [`LISTABLE_ACCOUNTS`].
pub fn is_listable_account(account: &str) -> bool {
    LISTABLE_ACCOUNTS.contains(&account)
}

/// Compute account summaries from journal content + declared-account
/// metadata. Pure function — no file I/O, no DB access — so it's
/// straightforward to unit-test against fixture strings.
///
/// `as_of` is the date used for FX conversion (latest rate ≤ that date wins
/// per `Prices::get_rate` semantics). Callers typically pass "today".
pub fn account_summaries(
    journal_content: &str,
    declared: &[AccountRow],
    base_currency: &str,
    as_of: NaiveDate,
) -> Result<Vec<AccountSummary>, LedgerError> {
    let parsed = ledger::parse(journal_content)?;
    let mut prices = Prices::new();
    prices.insert_from(&parsed);

    let balance = ledger::balances(journal_content)?;

    // Index declared accounts by name so we can splice metadata in.
    let declared_by_name: BTreeMap<&str, &AccountRow> =
        declared.iter().map(|a| (a.id.as_str(), a)).collect();

    // Collect candidate account names: those in the computed balance plus
    // any declared account that hasn't been touched yet (so it still shows
    // up with a zero balance).
    let mut account_names: BTreeMap<String, ()> = BTreeMap::new();
    for name in balance.account_balances.keys() {
        if is_listable_account(name) {
            account_names.insert(name.clone(), ());
        }
    }
    for name in declared_by_name.keys() {
        if is_listable_account(name) {
            account_names.insert((*name).to_string(), ());
        }
    }

    let mut summaries = Vec::with_capacity(account_names.len());
    for name in account_names.into_keys() {
        let empty_amounts = std::collections::HashMap::new();
        let amounts = balance
            .account_balances
            .get(&name)
            .map(|ab| &ab.amounts)
            .unwrap_or(&empty_amounts);

        let mut balances: Vec<CommodityBalance> = amounts
            .iter()
            .map(|(commodity, amount)| {
                let value_in_base =
                    convert_to_base(&prices, amount.quantity, commodity, base_currency, as_of);
                CommodityBalance {
                    commodity: commodity.clone(),
                    quantity: amount.quantity,
                    value_in_base,
                }
            })
            .collect();
        balances.sort_by(|a, b| a.commodity.cmp(&b.commodity));

        let total_in_base: Option<Decimal> = balances
            .iter()
            .filter_map(|b| b.value_in_base)
            .reduce(|a, b| a + b);

        let declared_meta = declared_by_name.get(name.as_str());
        summaries.push(AccountSummary {
            account: name,
            display_name: declared_meta.and_then(|m| m.display_name.clone()),
            last_reconciled_through: declared_meta.and_then(|m| m.last_reconciled_through.clone()),
            last_statement_balance: declared_meta.and_then(|m| m.last_statement_balance.clone()),
            balances,
            total_in_base,
        });
    }

    Ok(summaries)
}

fn convert_to_base(
    prices: &Prices,
    quantity: Decimal,
    commodity: &str,
    base: &str,
    as_of: NaiveDate,
) -> Option<Decimal> {
    if commodity.eq_ignore_ascii_case(base) {
        return Some(quantity);
    }
    prices.convert(quantity, commodity, base, as_of).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn acct_row(id: &str, commodity: &str, display: Option<&str>) -> AccountRow {
        AccountRow {
            id: id.into(),
            commodity: commodity.into(),
            display_name: display.map(String::from),
            last_reconciled_through: None,
            last_statement_balance: None,
        }
    }

    fn acct_row_reconciled(
        id: &str,
        commodity: &str,
        display: Option<&str>,
        through: &str,
        balance: &str,
    ) -> AccountRow {
        AccountRow {
            id: id.into(),
            commodity: commodity.into(),
            display_name: display.map(String::from),
            last_reconciled_through: Some(through.into()),
            last_statement_balance: Some(balance.into()),
        }
    }

    fn as_of() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 5, 23).unwrap()
    }

    #[test]
    fn is_listable_account_includes_user_allowlist_only() {
        // The four entries currently in LISTABLE_ACCOUNTS — explicit
        // allowlist, drop-by-default for everything else.
        assert!(is_listable_account("Assets:Wealthsimple:Cash"));
        assert!(is_listable_account("Assets:Wise:CAD"));
        assert!(is_listable_account("Liabilities:CIBC:CreditCard"));
        assert!(is_listable_account("Unmatched"));

        // Spending / income categories never qualify.
        assert!(!is_listable_account("Expenses:Groceries"));
        assert!(!is_listable_account("Income:Salary"));
        assert!(!is_listable_account("Equity:OpeningBalances"));

        // Drop-by-default: unknown accounts (typos, sub-accounts not on the
        // list, future names) don't sneak onto the screen silently.
        assert!(!is_listable_account("Assets:Cash")); // not on the list
        assert!(!is_listable_account("Trading:Exchange"));
        assert!(!is_listable_account("Random:Garbage"));
    }

    #[test]
    fn account_summaries_aggregates_cad_passthrough() {
        let journal = "\
2026-05-20 Coffee
    Assets:Wealthsimple:Cash       -5.25 CAD
    Expenses:Coffee                 5.25 CAD

2026-05-20 Groceries
    Assets:Wealthsimple:Cash      -42.18 CAD
    Expenses:Groceries             42.18 CAD
";
        let summaries =
            account_summaries(journal, &[], "CAD", as_of()).expect("balance computation");

        // Only Assets:Wealthsimple:Cash survives the filter; Expenses:* are
        // dropped.
        assert_eq!(summaries.len(), 1);
        let ws = &summaries[0];
        assert_eq!(ws.account, "Assets:Wealthsimple:Cash");
        assert_eq!(ws.balances.len(), 1);
        assert_eq!(ws.balances[0].commodity, "CAD");
        assert_eq!(ws.balances[0].quantity, Decimal::from_str("-47.43").unwrap());
        // CAD == base → value_in_base passes through.
        assert_eq!(
            ws.balances[0].value_in_base,
            Some(Decimal::from_str("-47.43").unwrap())
        );
        assert_eq!(ws.total_in_base, Some(Decimal::from_str("-47.43").unwrap()));
    }

    #[test]
    fn account_summaries_converts_foreign_commodity_via_p_directive() {
        // Wise CAD account holds CAD + USD; P directive supplies the rate.
        // P-directive format is `P date time base rate quote` — the time
        // component is required by ledger-parser even for daily rates (see
        // render_exchange_rate doc-comment for the why).
        let journal = "\
P 2026-05-20 00:00:00 USD 1.37 CAD

2026-05-21 Top-up
    Assets:Wise:CAD                100.00 USD
    Income:External               -100.00 USD

2026-05-21 Coffee
    Assets:Wise:CAD                 10.00 CAD
    Expenses:Coffee                -10.00 CAD
";
        let summaries =
            account_summaries(journal, &[], "CAD", as_of()).expect("balance computation");

        let wise = summaries
            .iter()
            .find(|s| s.account == "Assets:Wise:CAD")
            .expect("Wise account present");

        // Two commodity rows — alphabetical sort means CAD before USD.
        assert_eq!(wise.balances.len(), 2);
        assert_eq!(wise.balances[0].commodity, "CAD");
        assert_eq!(wise.balances[0].quantity, Decimal::from_str("10.00").unwrap());
        assert_eq!(
            wise.balances[0].value_in_base,
            Some(Decimal::from_str("10.00").unwrap())
        );

        assert_eq!(wise.balances[1].commodity, "USD");
        assert_eq!(wise.balances[1].quantity, Decimal::from_str("100.00").unwrap());
        // 100 USD * 1.37 CAD/USD = 137.00 CAD
        assert_eq!(
            wise.balances[1].value_in_base,
            Some(Decimal::from_str("137.00").unwrap())
        );

        // Total = 10 + 137 = 147 CAD
        assert_eq!(wise.total_in_base, Some(Decimal::from_str("147.00").unwrap()));
    }

    #[test]
    fn account_summaries_marks_unconvertible_commodity_with_none() {
        // BTC has no P directive AND each txn is balanced same-commodity, so
        // `ledger-utils::Prices::get_prices_from_transactions` (which needs
        // a 2-posting different-commodity txn) doesn't infer any rate.
        let journal = "\
2026-05-21 BTC airdrop
    Assets:Wealthsimple:Cash         0.003 BTC
    Income:Crypto                   -0.003 BTC

2026-05-21 CAD spending
    Assets:Wealthsimple:Cash       -100.00 CAD
    Expenses:Random                 100.00 CAD
";
        let summaries =
            account_summaries(journal, &[], "CAD", as_of()).expect("balance computation");

        let ws = &summaries[0];
        assert_eq!(ws.account, "Assets:Wealthsimple:Cash");

        let btc = ws
            .balances
            .iter()
            .find(|b| b.commodity == "BTC")
            .expect("BTC row");
        assert_eq!(btc.quantity, Decimal::from_str("0.003").unwrap());
        assert_eq!(btc.value_in_base, None);

        let cad = ws.balances.iter().find(|b| b.commodity == "CAD").unwrap();
        assert_eq!(cad.value_in_base, Some(Decimal::from_str("-100.00").unwrap()));

        // Total reflects only the convertible CAD leg.
        assert_eq!(ws.total_in_base, Some(Decimal::from_str("-100.00").unwrap()));
    }

    #[test]
    fn account_summaries_splices_declared_metadata() {
        let journal = "\
2026-05-20 Open
    Assets:Wealthsimple:Cash       1000.00 CAD
    Equity:OpeningBalances        -1000.00 CAD
";
        let declared = vec![acct_row_reconciled(
            "Assets:Wealthsimple:Cash",
            "CAD",
            Some("Wealthsimple Cash"),
            "2026-05-15",
            "1000.00",
        )];
        let summaries = account_summaries(journal, &declared, "CAD", as_of()).unwrap();

        let ws = summaries
            .iter()
            .find(|s| s.account == "Assets:Wealthsimple:Cash")
            .unwrap();
        assert_eq!(ws.display_name.as_deref(), Some("Wealthsimple Cash"));
        assert_eq!(ws.last_reconciled_through.as_deref(), Some("2026-05-15"));
        assert_eq!(ws.last_statement_balance.as_deref(), Some("1000.00"));
    }

    #[test]
    fn account_summaries_includes_declared_account_with_zero_balance() {
        // No postings touch Liabilities:CIBC:CreditCard but it's declared —
        // it should still show on the screen so the user can see "yep, zero".
        let journal = "\
2026-05-20 Coffee
    Assets:Wealthsimple:Cash       -5.25 CAD
    Expenses:Coffee                 5.25 CAD
";
        let declared = vec![acct_row(
            "Liabilities:CIBC:CreditCard",
            "CAD",
            Some("CIBC Aventura"),
        )];
        let summaries = account_summaries(journal, &declared, "CAD", as_of()).unwrap();

        let cibc = summaries
            .iter()
            .find(|s| s.account == "Liabilities:CIBC:CreditCard");
        assert!(cibc.is_some(), "declared listable account must appear even with zero balance");
        let cibc = cibc.unwrap();
        assert!(cibc.balances.is_empty());
        assert_eq!(cibc.total_in_base, None);
    }

    #[test]
    fn account_summaries_handles_empty_journal() {
        // Fresh-install path: no journal content + no declarations → empty
        // list, not an error.
        let summaries = account_summaries("", &[], "CAD", as_of()).unwrap();
        assert!(summaries.is_empty());
    }

    #[test]
    fn account_summaries_keeps_unmatched_clearing_account() {
        // From project_unmatched_account_pattern.md: non-zero Unmatched is
        // the reconciliation-pending signal. Must surface on the list.
        let journal = "\
2026-05-21 WS top-up (auto-import; counter-leg unknown)
    Assets:Wealthsimple:Cash       250.00 CAD
    Unmatched                     -250.00 CAD
";
        let summaries = account_summaries(journal, &[], "CAD", as_of()).unwrap();
        let unmatched = summaries.iter().find(|s| s.account == "Unmatched");
        assert!(unmatched.is_some(), "Unmatched must remain visible");
        assert_eq!(
            unmatched.unwrap().total_in_base,
            Some(Decimal::from_str("-250.00").unwrap())
        );
    }
}
