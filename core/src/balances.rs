//! Account-balance aggregation for the Phase 4.4 account-list screen.
//!
//! Pulls together three sources:
//! - The per-device journal file (source of truth for postings + `P`
//!   directives) — fed through [`ledger::balances`] for per-(account,
//!   commodity) quantities.
//! - The `accounts` projection table — for declared-account metadata
//!   (`display_name`, `hidden`, `is_liquid`).
//! - `ledger-utils::Prices` over the same parsed journal — for converting
//!   foreign-commodity balances into the user's base currency.
//!
//! The journal's `P` directives come from two converging paths:
//! - Frankfurter daily-rate fetcher writes CAD/USD/EUR (Phase 2.7).
//! - Auto-import batch commit writes manual AED rates entered at review
//!   time (Phase 3.10.5).
//!
//! Both paths land as the same hledger `P` directive shape, so this module
//! consumes them uniformly through `Prices::insert_from`.
//!
//! Account-set policy is the caller-supplied roster passed to
//! [`account_summaries`] — a list of account names to surface. The public
//! engine defaults to an empty roster; the user's real roster is delivered at
//! the client via the settings-file rail (`tauri-app` `ROSTER_FILE`).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use ledger_utils::balance::Balance;
use ledger_utils::prices::Prices;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::db::queries::AccountRow;
use crate::ledger::{self, LedgerError};
use crate::query::{QueryTxn, group_account_by_tag};

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
    /// One row per commodity held, sorted by commodity name for determinism
    /// (POC 0.1c finding — HashMap iteration order is non-deterministic).
    pub balances: Vec<CommodityBalance>,
    /// Sum of `value_in_base` across all balances where conversion succeeded.
    /// `None` when no balance was convertible — caller renders a "—" badge.
    pub total_in_base: Option<Decimal>,
    /// 3.10: the user has marked this account a liquid (spendable) asset
    /// (opt-in; default `false`). The dashboard sums these for the
    /// liquidity-aware "Can I afford X?" verdict.
    pub is_liquid: bool,
}

// Account-set policy is now the `roster` argument to `account_summaries` (a
// caller-supplied list of account names). Drop-by-default still holds: any
// account not named in the roster is filtered out, so auto-import-discovered
// accounts never appear silently. The public engine passes an empty roster;
// the user's real roster lives in the private overlay and is delivered to the
// client via the settings-file rail (`tauri-app` ROSTER_FILE). `Unmatched`
// (the one-word clearing account from project_unmatched_account_pattern.md)
// surfaces only when the roster includes it.

/// Compute account summaries from journal content + declared-account
/// metadata. Pure function — no file I/O, no DB access — so it's
/// straightforward to unit-test against fixture strings.
///
/// `as_of` is the date used for FX conversion (latest rate ≤ that date wins
/// per `Prices::get_rate` semantics). Callers typically pass "today".
///
/// `roster` is the drop-by-default allowlist of account names to surface; an
/// empty roster yields an empty list (the public engine's default).
pub fn account_summaries(
    journal_content: &str,
    declared: &[AccountRow],
    base_currency: &str,
    as_of: NaiveDate,
    roster: &[String],
) -> Result<Vec<AccountSummary>, LedgerError> {
    let artifacts = ledger::parse_artifacts(journal_content)?;
    Ok(account_summaries_from(
        &artifacts.balance,
        &artifacts.prices,
        declared,
        base_currency,
        as_of,
        roster,
    ))
}

/// Parsed-input variant of [`account_summaries`]: works off pre-computed
/// `balance` + `prices` (e.g. the Tauri-side journal cache) so a batch of read
/// commands sharing one journal parse it only once. Infallible — all parsing
/// (the only failure source) happened upstream.
pub fn account_summaries_from(
    balance: &Balance,
    prices: &Prices,
    declared: &[AccountRow],
    base_currency: &str,
    as_of: NaiveDate,
    roster: &[String],
) -> Vec<AccountSummary> {
    // Index declared accounts by name so we can splice metadata in.
    let declared_by_name: BTreeMap<&str, &AccountRow> =
        declared.iter().map(|a| (a.id.as_str(), a)).collect();

    // Collect candidate account names: those in the computed balance plus
    // any declared account that hasn't been touched yet (so it still shows
    // up with a zero balance).
    // Drop-by-default: only accounts named in the caller-supplied roster
    // surface. Public engine passes an empty roster → empty Accounts screen.
    let listable: std::collections::HashSet<&str> = roster.iter().map(String::as_str).collect();

    let mut account_names: BTreeMap<String, ()> = BTreeMap::new();
    for name in balance.account_balances.keys() {
        if listable.contains(name.as_str()) {
            account_names.insert(name.clone(), ());
        }
    }
    for name in declared_by_name.keys() {
        if listable.contains(*name) {
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
                    convert_to_base(prices, amount.quantity, commodity, base_currency, as_of);
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
            balances,
            total_in_base,
            is_liquid: declared_meta.is_some_and(|m| m.is_liquid),
        });
    }

    summaries
}

pub(crate) fn convert_to_base(
    prices: &Prices,
    quantity: Decimal,
    commodity: &str,
    base: &str,
    as_of: NaiveDate,
) -> Option<Decimal> {
    // Exact comparison, matching every other consumer. `Prices::get_rate` and
    // `account_summaries_from` both key on the exact string, so case-folding
    // *here alone* was the inconsistency: `cad` passed through as base (silently
    // asserting a 1:1 rate) while `Cad` looked up a pair that never exists. Both
    // still render as separate rows from `CAD` regardless, so folding here only
    // hid half the problem. Commodity is free text from the frontend; if it ever
    // needs normalizing, that belongs at the write boundary, not in one reader.
    if commodity == base {
        return Some(quantity);
    }
    prices.convert(quantity, commodity, base, as_of).ok()
}

// --- Auto-detected account sets (3.9) ---------------------------------------
//
// The journal already records every account ever posted to, so the account
// list never has to be hand-maintained. `auto_roster` derives the Accounts
// screen (balance-bearing types only, so net worth stays correct);
// `known_accounts` derives the full autocomplete set (all types + hierarchy).

/// The top-level hledger account type — the segment before the first `:`
/// (e.g. `Assets:Northwind:Cash` → `Assets`). A name with no `:` is its own
/// type (e.g. `Unmatched`).
pub fn account_type(name: &str) -> &str {
    name.split_once(':').map_or(name, |(top, _)| top)
}

/// Accounts that hold real money and belong on the Accounts screen / in net
/// worth: `Assets` and `Liabilities`, plus the single `Unmatched` clearing
/// account (`project_unmatched_account_pattern`). Excludes `Expenses` /
/// `Income` / `Equity`, which are flow/category accounts, not balances.
fn is_balance_bearing(name: &str) -> bool {
    matches!(account_type(name), "Assets" | "Liabilities") || name == "Unmatched"
}

/// Insert `name` and every ancestor prefix into `set` (`A:B:C` → `A`, `A:B`,
/// `A:B:C`) so hierarchical autocomplete can suggest intermediate nodes.
fn insert_with_ancestors(set: &mut std::collections::BTreeSet<String>, name: &str) {
    let mut prefix = String::new();
    for seg in name.split(':') {
        if !prefix.is_empty() {
            prefix.push(':');
        }
        prefix.push_str(seg);
        set.insert(prefix.clone());
    }
}

/// Derive the Accounts-screen roster automatically (3.9 "auto-include by
/// type"). Replaces the hand-maintained allowlist with: every balance-bearing
/// account *seen in the journal*, unioned with declared balance-bearing
/// accounts (so a just-declared, zero-balance account still shows), minus any
/// the user has hidden. The result is the same drop-by-default allowlist that
/// [`account_summaries`] already consumes — so its signature is unchanged.
///
/// Tolerant of a malformed journal: an unparseable file yields no *seen*
/// accounts (declared ones still surface) and [`account_summaries`] reports the
/// real parse error.
pub fn auto_roster(journal_content: &str, declared: &[AccountRow], hidden: &[String]) -> Vec<String> {
    // Tolerant of a malformed journal: fall back to an empty balance (so only
    // declared accounts surface), matching the pre-cache `if let Ok(balance)`.
    let balance = ledger::balances(journal_content).unwrap_or_else(|_| Balance::new());
    auto_roster_from(&balance, declared, hidden)
}

/// Parsed-input variant of [`auto_roster`] — works off a pre-computed
/// `balance` (the Tauri-side journal cache) instead of re-parsing.
pub fn auto_roster_from(balance: &Balance, declared: &[AccountRow], hidden: &[String]) -> Vec<String> {
    let hidden: std::collections::HashSet<&str> = hidden.iter().map(String::as_str).collect();
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for name in balance.account_balances.keys() {
        if is_balance_bearing(name) && !hidden.contains(name.as_str()) {
            set.insert(name.clone());
        }
    }
    for row in declared {
        if is_balance_bearing(&row.id) && !hidden.contains(row.id.as_str()) {
            set.insert(row.id.clone());
        }
    }
    set.into_iter().collect()
}

/// The full account-name set for autocomplete (3.9 data layer): every account
/// posted to in the journal (all types) ∪ declared accounts ∪ each name's
/// ancestor segments, sorted + deduped. Powers the shared `AccountInput`
/// typeahead so the user never maintains an account list by hand.
pub fn known_accounts(journal_content: &str, declared: &[AccountRow]) -> Vec<String> {
    let balance = ledger::balances(journal_content).unwrap_or_else(|_| Balance::new());
    known_accounts_from(&balance, declared)
}

/// Parsed-input variant of [`known_accounts`] — works off a pre-computed
/// `balance` (the Tauri-side journal cache) instead of re-parsing.
pub fn known_accounts_from(balance: &Balance, declared: &[AccountRow]) -> Vec<String> {
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for name in balance.account_balances.keys() {
        insert_with_ancestors(&mut set, name);
    }
    for row in declared {
        insert_with_ancestors(&mut set, &row.id);
    }
    set.into_iter().collect()
}

// --- Per-account tag breakdown (Accounts drill-down) ------------------------
//
// Under the MECE account grammar a balance-bearing account (e.g.
// `Assets:NonRegistered:CAD`) pools money across institutions/products, which
// live as posting tags rather than account segments. This slices one account
// by a chosen posting tag so the user can see the per-institution / per-product
// split that the account name deliberately hides.

/// One tag-value slice of a single account's holdings: the tag value
/// (institution / product / `(unassigned)`) with its per-commodity balances and
/// base-currency total. Mirrors [`AccountSummary`]'s money shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountTagBreakdown {
    pub value: String,
    pub balances: Vec<CommodityBalance>,
    pub total_in_base: Option<Decimal>,
}

/// Break one account's holdings down by a posting tag (`tag_key`, e.g.
/// `"institution"`). Sums the account's postings per (tag value, commodity) via
/// [`crate::query::group_account_by_tag`], then values each commodity in
/// `base_currency` using the journal's `P` directives — the same `Prices` path
/// [`account_summaries`] uses, so the slices reconcile to the account's own
/// base total. Postings come from the caller's projection-derived `txns`; the
/// journal is read only for FX rates, so an empty/rate-free journal still
/// returns native quantities (with `value_in_base = None`).
pub fn account_tag_breakdown(
    journal_content: &str,
    txns: &[QueryTxn],
    account: &str,
    tag_key: &str,
    base_currency: &str,
    as_of: NaiveDate,
) -> Result<Vec<AccountTagBreakdown>, LedgerError> {
    let artifacts = ledger::parse_artifacts(journal_content)?;
    Ok(account_tag_breakdown_from(
        &artifacts.prices,
        txns,
        account,
        tag_key,
        base_currency,
        as_of,
    ))
}

/// Parsed-input variant of [`account_tag_breakdown`] — takes a pre-built
/// `prices` table (the Tauri-side journal cache) instead of re-parsing the
/// journal just to read its `P` directives.
pub fn account_tag_breakdown_from(
    prices: &Prices,
    txns: &[QueryTxn],
    account: &str,
    tag_key: &str,
    base_currency: &str,
    as_of: NaiveDate,
) -> Vec<AccountTagBreakdown> {
    group_account_by_tag(txns, account, tag_key)
        .into_iter()
        .map(|group| {
            let balances: Vec<CommodityBalance> = group
                .amounts
                .into_iter()
                .map(|(commodity, quantity)| {
                    let value_in_base =
                        convert_to_base(prices, quantity, &commodity, base_currency, as_of);
                    CommodityBalance {
                        commodity,
                        quantity,
                        value_in_base,
                    }
                })
                .collect();
            let total_in_base = balances
                .iter()
                .filter_map(|b| b.value_in_base)
                .reduce(|a, b| a + b);
            AccountTagBreakdown {
                value: group.value,
                balances,
                total_in_base,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Tag;
    use crate::query::QueryPosting;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn acct_row(id: &str, commodity: &str, display: Option<&str>) -> AccountRow {
        AccountRow {
            id: id.into(),
            commodity: commodity.into(),
            display_name: display.map(String::from),
            hidden: false,
            is_liquid: false,
        }
    }

    fn as_of() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 5, 23).unwrap()
    }

    /// The user-style roster the existing fixtures were written against.
    fn roster() -> Vec<String> {
        [
            "Assets:Northwind:Cash",
            "Assets:Globepay:CAD",
            "Liabilities:Summit:CreditCard",
            "Unmatched",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn parsed_input_variants_match_content_path() {
        // The Tauri journal cache hands the parsed-input variants a Balance +
        // Prices built once via `parse_artifacts`; routing reads through that
        // cache must be byte-identical to the content-taking functions parsing
        // the journal themselves. A journal with a foreign commodity + a P
        // directive exercises both the balance table and the price table.
        let journal = "\
P 2026-05-20 00:00:00 USD 1.37 CAD

2026-05-21 Top-up
    Assets:Globepay:CAD               100.00 USD
    Income:External                  -100.00 USD

2026-05-21 Coffee
    Assets:Northwind:Cash             -10.00 CAD
    Expenses:Coffee                    10.00 CAD
";
        let declared = vec![acct_row("Assets:Globepay:CAD", "CAD", Some("Globepay"))];
        let artifacts = crate::ledger::parse_artifacts(journal).expect("parse artifacts");

        assert_eq!(
            account_summaries(journal, &declared, "CAD", as_of(), &roster()).unwrap(),
            account_summaries_from(
                &artifacts.balance,
                &artifacts.prices,
                &declared,
                "CAD",
                as_of(),
                &roster()
            ),
            "account_summaries: cached artifacts must match the content path",
        );
        assert_eq!(
            auto_roster(journal, &declared, &[]),
            auto_roster_from(&artifacts.balance, &declared, &[]),
            "auto_roster: cached balance must match the content path",
        );
        assert_eq!(
            known_accounts(journal, &declared),
            known_accounts_from(&artifacts.balance, &declared),
            "known_accounts: cached balance must match the content path",
        );
    }

    #[test]
    fn account_summaries_filters_to_roster_drop_by_default() {
        let journal = "\
2026-05-20 Coffee
    Assets:Northwind:Cash       -5.25 CAD
    Expenses:Coffee                 5.25 CAD
";
        // Roster omits Assets:Northwind:Cash on purpose → nothing surfaces,
        // proving membership (not mere presence in postings) is required.
        let narrow = vec!["Unmatched".to_string()];
        let summaries = account_summaries(journal, &[], "CAD", as_of(), &narrow).unwrap();
        assert!(summaries.is_empty(), "no roster account touched → empty list");

        // Full roster → the Northwind account surfaces; Expenses:Coffee (never in the
        // roster) is still dropped.
        let summaries = account_summaries(journal, &[], "CAD", as_of(), &roster()).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].account, "Assets:Northwind:Cash");
    }

    #[test]
    fn account_summaries_aggregates_cad_passthrough() {
        let journal = "\
2026-05-20 Coffee
    Assets:Northwind:Cash       -5.25 CAD
    Expenses:Coffee                 5.25 CAD

2026-05-20 Groceries
    Assets:Northwind:Cash      -42.18 CAD
    Expenses:Groceries             42.18 CAD
";
        let summaries =
            account_summaries(journal, &[], "CAD", as_of(), &roster()).expect("balance computation");

        // Only Assets:Northwind:Cash survives the filter; Expenses:* are
        // dropped.
        assert_eq!(summaries.len(), 1);
        let northwind = &summaries[0];
        assert_eq!(northwind.account, "Assets:Northwind:Cash");
        assert_eq!(northwind.balances.len(), 1);
        assert_eq!(northwind.balances[0].commodity, "CAD");
        assert_eq!(northwind.balances[0].quantity, Decimal::from_str("-47.43").unwrap());
        // CAD == base → value_in_base passes through.
        assert_eq!(
            northwind.balances[0].value_in_base,
            Some(Decimal::from_str("-47.43").unwrap())
        );
        assert_eq!(northwind.total_in_base, Some(Decimal::from_str("-47.43").unwrap()));
    }

    #[test]
    fn account_summaries_converts_foreign_commodity_via_p_directive() {
        // Globepay CAD account holds CAD + USD; P directive supplies the rate.
        // P-directive format is `P date time base rate quote` — the time
        // component is required by ledger-parser even for daily rates (see
        // render_exchange_rate doc-comment for the why).
        let journal = "\
P 2026-05-20 00:00:00 USD 1.37 CAD

2026-05-21 Top-up
    Assets:Globepay:CAD                100.00 USD
    Income:External               -100.00 USD

2026-05-21 Coffee
    Assets:Globepay:CAD                 10.00 CAD
    Expenses:Coffee                -10.00 CAD
";
        let summaries =
            account_summaries(journal, &[], "CAD", as_of(), &roster()).expect("balance computation");

        let globepay = summaries
            .iter()
            .find(|s| s.account == "Assets:Globepay:CAD")
            .expect("Globepay account present");

        // Two commodity rows — alphabetical sort means CAD before USD.
        assert_eq!(globepay.balances.len(), 2);
        assert_eq!(globepay.balances[0].commodity, "CAD");
        assert_eq!(globepay.balances[0].quantity, Decimal::from_str("10.00").unwrap());
        assert_eq!(
            globepay.balances[0].value_in_base,
            Some(Decimal::from_str("10.00").unwrap())
        );

        assert_eq!(globepay.balances[1].commodity, "USD");
        assert_eq!(globepay.balances[1].quantity, Decimal::from_str("100.00").unwrap());
        // 100 USD * 1.37 CAD/USD = 137.00 CAD
        assert_eq!(
            globepay.balances[1].value_in_base,
            Some(Decimal::from_str("137.00").unwrap())
        );

        // Total = 10 + 137 = 147 CAD
        assert_eq!(globepay.total_in_base, Some(Decimal::from_str("147.00").unwrap()));
    }

    #[test]
    fn account_summaries_marks_unconvertible_commodity_with_none() {
        // BTC has no P directive AND each txn is balanced same-commodity, so
        // `ledger-utils::Prices::get_prices_from_transactions` (which needs
        // a 2-posting different-commodity txn) doesn't infer any rate.
        let journal = "\
2026-05-21 BTC airdrop
    Assets:Northwind:Cash         0.003 BTC
    Income:Crypto                   -0.003 BTC

2026-05-21 CAD spending
    Assets:Northwind:Cash       -100.00 CAD
    Expenses:Random                 100.00 CAD
";
        let summaries =
            account_summaries(journal, &[], "CAD", as_of(), &roster()).expect("balance computation");

        let northwind = &summaries[0];
        assert_eq!(northwind.account, "Assets:Northwind:Cash");

        let btc = northwind
            .balances
            .iter()
            .find(|b| b.commodity == "BTC")
            .expect("BTC row");
        assert_eq!(btc.quantity, Decimal::from_str("0.003").unwrap());
        assert_eq!(btc.value_in_base, None);

        let cad = northwind.balances.iter().find(|b| b.commodity == "CAD").unwrap();
        assert_eq!(cad.value_in_base, Some(Decimal::from_str("-100.00").unwrap()));

        // Total reflects only the convertible CAD leg.
        assert_eq!(northwind.total_in_base, Some(Decimal::from_str("-100.00").unwrap()));
    }

    #[test]
    fn account_summaries_splices_declared_metadata() {
        let journal = "\
2026-05-20 Open
    Assets:Northwind:Cash       1000.00 CAD
    Equity:OpeningBalances        -1000.00 CAD
";
        let declared = vec![acct_row(
            "Assets:Northwind:Cash",
            "CAD",
            Some("Northwind Cash"),
        )];
        let summaries = account_summaries(journal, &declared, "CAD", as_of(), &roster()).unwrap();

        let northwind = summaries
            .iter()
            .find(|s| s.account == "Assets:Northwind:Cash")
            .unwrap();
        assert_eq!(northwind.display_name.as_deref(), Some("Northwind Cash"));
    }

    #[test]
    fn account_summaries_includes_declared_account_with_zero_balance() {
        // No postings touch Liabilities:Summit:CreditCard but it's declared —
        // it should still show on the screen so the user can see "yep, zero".
        let journal = "\
2026-05-20 Coffee
    Assets:Northwind:Cash       -5.25 CAD
    Expenses:Coffee                 5.25 CAD
";
        let declared = vec![acct_row(
            "Liabilities:Summit:CreditCard",
            "CAD",
            Some("Summit Rewards"),
        )];
        let summaries = account_summaries(journal, &declared, "CAD", as_of(), &roster()).unwrap();

        let summit = summaries
            .iter()
            .find(|s| s.account == "Liabilities:Summit:CreditCard");
        assert!(summit.is_some(), "declared listable account must appear even with zero balance");
        let summit = summit.unwrap();
        assert!(summit.balances.is_empty());
        assert_eq!(summit.total_in_base, None);
    }

    #[test]
    fn account_summaries_handles_empty_journal() {
        // Fresh-install path: no journal content + no declarations → empty
        // list, not an error.
        let summaries = account_summaries("", &[], "CAD", as_of(), &roster()).unwrap();
        assert!(summaries.is_empty());
    }

    #[test]
    fn account_summaries_keeps_unmatched_clearing_account() {
        // From project_unmatched_account_pattern.md: non-zero Unmatched is
        // the reconciliation-pending signal. Must surface on the list.
        let journal = "\
2026-05-21 Northwind top-up (auto-import; counter-leg unknown)
    Assets:Northwind:Cash       250.00 CAD
    Unmatched                     -250.00 CAD
";
        let summaries = account_summaries(journal, &[], "CAD", as_of(), &roster()).unwrap();
        let unmatched = summaries.iter().find(|s| s.account == "Unmatched");
        assert!(unmatched.is_some(), "Unmatched must remain visible");
        assert_eq!(
            unmatched.unwrap().total_in_base,
            Some(Decimal::from_str("-250.00").unwrap())
        );
    }

    // --- Auto-detected account sets (3.9) -----------------------------------

    /// Mixed-type journal: two Assets, one Liability, plus Expenses/Income
    /// (which must NOT count as balance-bearing).
    fn mixed_journal() -> &'static str {
        "\
2026-05-01 Groceries
    Expenses:Food:Groceries        50.00 CAD
    Assets:Northwind:Cash      -50.00 CAD

2026-05-02 Salary
    Assets:Globepay:CAD              3000.00 CAD
    Income:Salary               -3000.00 CAD

2026-05-03 Card
    Liabilities:Summit:CreditCard   -20.00 CAD
    Expenses:Food:Coffee           20.00 CAD
"
    }

    #[test]
    fn account_type_takes_top_segment() {
        assert_eq!(account_type("Assets:Globepay:CAD"), "Assets");
        assert_eq!(account_type("Liabilities:Summit:CreditCard"), "Liabilities");
        assert_eq!(account_type("Unmatched"), "Unmatched");
        assert_eq!(account_type(""), "");
    }

    #[test]
    fn auto_roster_includes_only_balance_bearing_seen_accounts() {
        let roster = auto_roster(mixed_journal(), &[], &[]);
        // Assets + Liabilities surface; Expenses + Income are dropped.
        assert_eq!(
            roster,
            vec![
                "Assets:Globepay:CAD".to_string(),
                "Assets:Northwind:Cash".to_string(),
                "Liabilities:Summit:CreditCard".to_string(),
            ]
        );
    }

    #[test]
    fn auto_roster_includes_declared_zero_balance_asset() {
        // Declared but never posted to — still an Asset, so it belongs.
        let declared = vec![acct_row("Assets:Summit:Savings", "CAD", None)];
        let roster = auto_roster(mixed_journal(), &declared, &[]);
        assert!(roster.contains(&"Assets:Summit:Savings".to_string()));
    }

    #[test]
    fn auto_roster_excludes_hidden_accounts() {
        let hidden = vec!["Assets:Globepay:CAD".to_string()];
        let roster = auto_roster(mixed_journal(), &[], &hidden);
        assert!(!roster.contains(&"Assets:Globepay:CAD".to_string()));
        assert!(roster.contains(&"Assets:Northwind:Cash".to_string()));
    }

    #[test]
    fn auto_roster_keeps_unmatched() {
        let journal = "\
2026-05-21 WS top-up
    Assets:Northwind:Cash       250.00 CAD
    Unmatched                     -250.00 CAD
";
        let roster = auto_roster(journal, &[], &[]);
        assert!(roster.contains(&"Unmatched".to_string()));
    }

    #[test]
    fn auto_roster_tolerates_malformed_journal() {
        // Garbage in → no seen accounts, declared still surface, no panic.
        let declared = vec![acct_row("Assets:Cash", "CAD", None)];
        let roster = auto_roster("@@@ not a journal @@@", &declared, &[]);
        assert_eq!(roster, vec!["Assets:Cash".to_string()]);
    }

    #[test]
    fn known_accounts_includes_all_types_with_ancestors() {
        let known = known_accounts(mixed_journal(), &[]);
        // Full leaves across every type…
        for leaf in [
            "Assets:Northwind:Cash",
            "Assets:Globepay:CAD",
            "Liabilities:Summit:CreditCard",
            "Expenses:Food:Groceries",
            "Expenses:Food:Coffee",
            "Income:Salary",
        ] {
            assert!(known.contains(&leaf.to_string()), "missing leaf {leaf}");
        }
        // …plus the intermediate hierarchy nodes for typeahead.
        for node in ["Assets", "Assets:Globepay", "Expenses", "Expenses:Food", "Income"] {
            assert!(known.contains(&node.to_string()), "missing node {node}");
        }
        // Sorted + deduped (BTreeSet guarantees both).
        let mut sorted = known.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(known, sorted);
    }

    #[test]
    fn known_accounts_unions_declared() {
        let declared = vec![acct_row("Assets:Brokerage:RRSP", "CAD", None)];
        let known = known_accounts("", &declared);
        assert!(known.contains(&"Assets:Brokerage:RRSP".to_string()));
        assert!(known.contains(&"Assets:Brokerage".to_string()));
        assert!(known.contains(&"Assets".to_string()));
    }

    // --- Per-account tag breakdown (drill-down) -----------------------------

    fn breakdown_posting(commodity: &str, amount: &str, institution: &str) -> QueryPosting {
        QueryPosting {
            account: "Assets:NonRegistered:CAD".into(),
            commodity: commodity.into(),
            amount: Decimal::from_str(amount).unwrap(),
            tags: vec![Tag::KeyValue {
                key: "institution".into(),
                value: institution.into(),
            }],
        }
    }

    fn breakdown_txn(posting: QueryPosting) -> QueryTxn {
        QueryTxn {
            date: "2026-05-21".into(),
            description: "t".into(),
            top_tags: vec![],
            postings: vec![posting],
        }
    }

    #[test]
    fn account_tag_breakdown_groups_and_values_in_base() {
        // Journal supplies only the FX rate; postings come from `txns`.
        let journal = "P 2026-05-20 00:00:00 USD 1.37 CAD\n";
        let txns = vec![
            breakdown_txn(breakdown_posting("CAD", "300.00", "Summit")),
            breakdown_txn(breakdown_posting("USD", "100.00", "Globepay")),
        ];
        let out = account_tag_breakdown(
            journal,
            &txns,
            "Assets:NonRegistered:CAD",
            "institution",
            "CAD",
            as_of(),
        )
        .unwrap();

        assert_eq!(out.len(), 2);
        // Globepay: 100 USD @ 1.37 → 137.00 CAD base value.
        assert_eq!(out[0].value, "Globepay");
        assert_eq!(out[0].balances[0].commodity, "USD");
        assert_eq!(out[0].balances[0].quantity, Decimal::from_str("100.00").unwrap());
        assert_eq!(out[0].total_in_base, Some(Decimal::from_str("137.00").unwrap()));
        // Summit: 300 CAD passes through (== base).
        assert_eq!(out[1].value, "Summit");
        assert_eq!(out[1].total_in_base, Some(Decimal::from_str("300.00").unwrap()));
    }

    #[test]
    fn account_tag_breakdown_marks_unconvertible_commodity_none() {
        // No P directive for AED → its group has no base total.
        let txns = vec![breakdown_txn(breakdown_posting("AED", "500.00", "Meridian"))];
        let out = account_tag_breakdown(
            "",
            &txns,
            "Assets:NonRegistered:CAD",
            "institution",
            "CAD",
            as_of(),
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value, "Meridian");
        assert_eq!(out[0].balances[0].value_in_base, None);
        assert_eq!(out[0].total_in_base, None);
    }
}
