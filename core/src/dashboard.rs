//! R1 financial-health glance dashboard aggregation (Phase 4.5 + 4.6).
//!
//! Produces the four widget payloads that `DashboardView` renders:
//! - **Net worth** — sum of listable accounts' base-currency values, with
//!   liabilities subtracted automatically (negative balances on
//!   `Liabilities:*` come back as negative `value_in_base`, so the sum is
//!   already correct).
//! - **`Unmatched` balance** — single number; the reconciliation-pending
//!   signal per `project_unmatched_account_pattern.md`. Click-through on
//!   the screen filters the transaction list to `account: Unmatched`.
//! - **Monthly income / spending trend** — last N months of (income,
//!   spending) per the user's base currency, bucketed by year-month.
//!   Foreign-commodity legs are excluded (documented limitation; revisit
//!   when more than 5% of activity isn't in `base_currency`).
//! - **Recurring obligations** — confirmed `RecurringPattern` rows pulled
//!   from the projection table. Empty until Phase 5.3/5.4 ship the
//!   detection scanner + confirm UI.
//!
//! Plus one decision-shaped helper:
//! - [`can_i_afford`] — verdict-from-payload for the "can I afford X?" UX.
//!   3.10 makes it liquidity-aware (spendable pool = user-marked liquid
//!   accounts, falling back to net worth) — see the fn's inline decision note.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use ledger_utils::prices::Prices;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::balances::{self, AccountSummary};
use crate::db::queries::{AccountRow, RecurringPatternRow, TxnPostingsRow};
use crate::ledger::{self, LedgerError};

/// One row in the monthly trend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonthlyTrendBucket {
    /// `YYYY-MM` — calendar month label, sortable as-is.
    pub month: String,
    /// Sum of `Income:*` posting magnitudes in base currency (positive).
    /// Postings on `Income:*` accounts carry negative amounts under
    /// double-entry, so we flip the sign for display.
    pub income: Decimal,
    /// Sum of `Expenses:*` posting amounts in base currency (positive).
    pub spending: Decimal,
}

/// One confirmed recurring pattern, distilled to the bits the dashboard
/// widget shows. Mirrors the relevant subset of
/// `RecurringTransactionDetected.pattern`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecurringObligation {
    pub vendor: String,
    pub amount: Decimal,
    pub commodity: String,
    pub cadence_days: u32,
}

/// Full payload for the R1 dashboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub base_currency: String,
    /// Sum of every roster account's `total_in_base` (filtered by the roster
    /// passed to `account_summaries`). `None` when no listable account
    /// has any convertible balance — UI renders an em-dash.
    pub net_worth_in_base: Option<Decimal>,
    /// 3.10: sum of the accounts the user has marked *liquid* (spendable), for
    /// the liquidity-aware "Can I afford X?" verdict. `None` carries a specific
    /// meaning — **no account is marked liquid** (opt-in unused), so
    /// [`can_i_afford`] falls back to net worth. `Some(x)` means the liquid
    /// policy is active with spendable total `x` (possibly `0` if the marked
    /// accounts are empty or unconvertible).
    pub liquid_assets_in_base: Option<Decimal>,
    /// `Unmatched` clearing-account balance in base currency. `None` when
    /// the account has no convertible balance (typically: zero or
    /// unconvertible commodities only). Non-zero values flag pending
    /// reconciliation per [`project-unmatched-account-pattern`].
    pub unmatched_balance: Option<Decimal>,
    pub monthly_buckets: Vec<MonthlyTrendBucket>,
    pub recurring: Vec<RecurringObligation>,
}

/// Returns the verdict for a "Can I afford X?" query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AffordVerdict {
    pub can_afford: bool,
    /// What's left over after the purchase + the obligations the policy
    /// considers. Negative = the policy says you can't afford it.
    pub remaining_in_base: Decimal,
    /// One-line policy explanation for the UI (e.g. "Net worth − next
    /// month's recurring"). Helps the user understand which rule fired.
    pub policy_label: String,
}

/// Build the dashboard summary in one shot. Pure function — takes parsed
/// inputs, returns a struct. The Tauri command wraps the I/O.
// Eight inputs: a pure aggregator fanning the same parsed journal + metadata
// into several sub-computations. Bundling them into a params struct would add
// indirection without clarifying anything — allow the extra argument.
#[allow(clippy::too_many_arguments)]
pub fn dashboard_summary(
    journal_content: &str,
    declared: &[AccountRow],
    recurring: &[RecurringPatternRow],
    base_currency: &str,
    as_of: NaiveDate,
    monthly_txns: &[TxnPostingsRow],
    months_back: u32,
    roster: &[String],
) -> Result<DashboardSummary, LedgerError> {
    let summaries =
        balances::account_summaries(journal_content, declared, base_currency, as_of, roster)?;

    let net_worth_in_base = sum_listable_net_worth(&summaries);
    let liquid_assets_in_base = sum_liquid_assets(&summaries);
    let unmatched_balance = summaries
        .iter()
        .find(|s| s.account == "Unmatched")
        .and_then(|s| s.total_in_base);

    // Same Prices ingestion that `balances` uses — keeps the trend's FX
    // conversion consistent with the per-account aggregation.
    let parsed = ledger::parse(journal_content)?;
    let mut prices = Prices::new();
    prices.insert_from(&parsed);

    let monthly_buckets =
        bucket_postings_by_month(monthly_txns, base_currency, &prices, as_of, months_back);
    let recurring = distill_recurring(recurring);

    Ok(DashboardSummary {
        base_currency: base_currency.to_string(),
        net_worth_in_base,
        liquid_assets_in_base,
        unmatched_balance,
        monthly_buckets,
        recurring,
    })
}

fn sum_listable_net_worth(summaries: &[AccountSummary]) -> Option<Decimal> {
    summaries
        .iter()
        // Exclude Unmatched from net worth — it's the reconciliation-pending
        // clearing account, not real money. Its non-zero state is the
        // signal that net-worth is provisional, not a net-worth component.
        .filter(|s| s.account != "Unmatched")
        .filter_map(|s| s.total_in_base)
        .reduce(|a, b| a + b)
}

/// Sum of the accounts the user marked *liquid* (3.10), for the afford verdict.
///
/// The `None` vs `Some` distinction is load-bearing: `None` means **no account
/// is marked liquid** (opt-in unused) → [`can_i_afford`] falls back to net
/// worth. Once ≥1 account is marked liquid we return `Some(total)` even if that
/// total is `0` (e.g. the marked accounts are empty or unconvertible), so a
/// deliberate "no spendable cash" reads as "can't afford it", not as a fallback
/// to full net worth. `Unmatched` is never liquid (it's a clearing account).
fn sum_liquid_assets(summaries: &[AccountSummary]) -> Option<Decimal> {
    let liquid: Vec<&AccountSummary> = summaries
        .iter()
        .filter(|s| s.is_liquid && s.account != "Unmatched")
        .collect();
    if liquid.is_empty() {
        return None;
    }
    // Some(0) when none of the marked accounts have a convertible balance.
    Some(
        liquid
            .iter()
            .filter_map(|s| s.total_in_base)
            .fold(Decimal::ZERO, |a, b| a + b),
    )
}

fn bucket_postings_by_month(
    rows: &[TxnPostingsRow],
    base_currency: &str,
    prices: &Prices,
    as_of: NaiveDate,
    months_back: u32,
) -> Vec<MonthlyTrendBucket> {
    // Reshape DbValue → serde_json once, then hand off to the pure
    // aggregator. The detour buys testability — tests construct plain
    // serde_json::Values and call the inner fn directly, no DbValue
    // construction needed (surrealdb-types v3 doesn't expose a clean
    // `from_json_value` constructor for FLEXIBLE objects).
    let parsed: Vec<(String, serde_json::Value)> = rows
        .iter()
        .map(|r| (r.date.clone(), r.postings.clone().into_json_value()))
        .collect();
    bucket_parsed_postings(&parsed, base_currency, prices, as_of, months_back)
}

fn bucket_parsed_postings(
    rows: &[(String, serde_json::Value)],
    base_currency: &str,
    prices: &Prices,
    as_of: NaiveDate,
    months_back: u32,
) -> Vec<MonthlyTrendBucket> {
    let earliest = months_back_label(as_of, months_back);
    let mut buckets: BTreeMap<String, (Decimal, Decimal)> = BTreeMap::new();
    for (date_str, postings_json) in rows {
        if date_str.len() < 7 {
            continue;
        }
        let month = &date_str[..7];
        if month.as_bytes() < earliest.as_bytes() {
            continue;
        }
        let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            continue;
        };
        let Some(postings) = postings_json.as_array() else {
            continue;
        };
        let (income, spending) = buckets
            .entry(month.to_string())
            .or_insert((Decimal::ZERO, Decimal::ZERO));
        for p in postings {
            let Some(account) = p.get("account").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(amount_raw) = p.get("amount").and_then(|v| v.as_str()) else {
                continue;
            };
            let Ok(qty) = amount_raw.parse::<Decimal>() else {
                continue;
            };
            let commodity = p
                .get("commodity")
                .and_then(|v| v.as_str())
                .unwrap_or(base_currency);
            let value = if commodity.eq_ignore_ascii_case(base_currency) {
                qty
            } else {
                match prices.convert(qty, commodity, base_currency, date) {
                    Ok(v) => v,
                    Err(_) => continue, // skip unconvertible legs
                }
            };
            if account.starts_with("Income:") {
                // Income postings store negative amounts (the other leg
                // received the money) — flip for a positive display value.
                *income += -value;
            } else if account.starts_with("Expenses:") {
                *spending += value;
            }
        }
    }
    // Fill missing months with zeros so the chart axis is continuous.
    let mut out = Vec::with_capacity(months_back as usize);
    for label in month_range(as_of, months_back) {
        let (income, spending) = buckets
            .get(&label)
            .copied()
            .unwrap_or((Decimal::ZERO, Decimal::ZERO));
        out.push(MonthlyTrendBucket {
            month: label,
            income,
            spending,
        });
    }
    out
}

fn months_back_label(as_of: NaiveDate, months_back: u32) -> String {
    let mut y = as_of.year();
    let mut m = as_of.month() as i32 - (months_back as i32 - 1);
    while m <= 0 {
        m += 12;
        y -= 1;
    }
    format!("{y:04}-{m:02}")
}

fn month_range(as_of: NaiveDate, months_back: u32) -> Vec<String> {
    let mut out = Vec::with_capacity(months_back as usize);
    let mut y = as_of.year();
    let mut m = as_of.month() as i32 - (months_back as i32 - 1);
    while m <= 0 {
        m += 12;
        y -= 1;
    }
    for _ in 0..months_back {
        out.push(format!("{y:04}-{m:02}"));
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    out
}

fn distill_recurring(rows: &[RecurringPatternRow]) -> Vec<RecurringObligation> {
    // Same DbValue → serde_json detour pattern as `bucket_postings_by_month`
    // — keeps the pure logic separately testable.
    let parsed: Vec<(String, serde_json::Value)> = rows
        .iter()
        .map(|r| (r.status.clone(), r.pattern.clone().into_json_value()))
        .collect();
    distill_parsed_recurring(&parsed)
}

fn distill_parsed_recurring(rows: &[(String, serde_json::Value)]) -> Vec<RecurringObligation> {
    rows.iter()
        .filter(|(status, _)| status == "confirmed")
        .filter_map(|(_, pattern)| {
            let vendor = pattern.get("vendor")?.as_str()?.to_string();
            let amount_str = pattern.get("amount")?.as_str()?;
            let amount = amount_str.parse::<Decimal>().ok()?;
            let commodity = pattern
                .get("commodity")
                .and_then(|v| v.as_str())
                .unwrap_or("CAD")
                .to_string();
            let cadence_days = pattern.get("cadence_days")?.as_u64()? as u32;
            Some(RecurringObligation {
                vendor,
                amount,
                commodity,
                cadence_days,
            })
        })
        .collect()
}

/// Compute the verdict for "can I afford `amount`?" against a dashboard
/// summary.
///
/// 3.10 makes this **liquidity-aware**: the spendable pool is the accounts the
/// user has marked liquid (opt-in), not full net worth. Two pools are in play:
///
/// - [`DashboardSummary::liquid_assets_in_base`] — `Some(x)` once ≥1 account is
///   marked liquid; the spendable total (possibly `0`). The primary pool.
/// - [`DashboardSummary::net_worth_in_base`] — the fallback pool when
///   **nothing** is marked liquid (`liquid_assets_in_base == None`), preserving
///   pre-3.10 behavior so the feature degrades gracefully.
///
/// Both pools subtract the next month's recurring obligations before the
/// purchase (the conservative "is there room after my bills?" rule):
/// `remaining = pool − next_month_recurring − amount`, `can_afford =
/// remaining > 0`. `Unmatched` is excluded from both pools (clearing account),
/// so it never inflates a verdict.
pub fn can_i_afford(amount: Decimal, summary: &DashboardSummary) -> AffordVerdict {
    let (pool, policy_label) = match (summary.liquid_assets_in_base, summary.net_worth_in_base) {
        (Some(liquidity), _) => (liquidity, "Liquid assets − next month's recurring".into()),
        (None, Some(net_worth)) => (net_worth, "Net worth − next month's recurring".into()),
        (None, None) => {
            return AffordVerdict {
                can_afford: false,
                remaining_in_base: Decimal::ZERO,
                policy_label: "Net worth unavailable".into(),
            };
        }
    };

    let remaining_in_base = pool - next_month_recurring_total(summary) - amount;
    let can_afford = remaining_in_base > Decimal::ZERO;
    AffordVerdict {
        can_afford,
        remaining_in_base,
        policy_label,
    }
}

/// Total monthly burn of confirmed recurring obligations, in base currency.
/// Assumes each recurring amount is already denominated in the dashboard's
/// base currency (which Phase 5.3 will enforce at detection time — until
/// then, foreign-commodity recurring patterns are dropped from this sum).
pub fn next_month_recurring_total(summary: &DashboardSummary) -> Decimal {
    let days_in_month = Decimal::from(30u32);
    summary
        .recurring
        .iter()
        .filter(|r| r.commodity.eq_ignore_ascii_case(&summary.base_currency))
        .map(|r| {
            // Normalize to a per-month equivalent: weekly → 4.something,
            // monthly → 1, biweekly → ~2, etc. Cadence in days → multiplier.
            if r.cadence_days == 0 {
                Decimal::ZERO
            } else {
                r.amount * days_in_month / Decimal::from(r.cadence_days)
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Roster the dashboard fixtures were written against (Northwind / Globepay / Summit /
    /// Unmatched). Net-worth + unmatched assertions depend on these surfacing.
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

    fn postings_json(items: &[(&str, &str, &str)]) -> serde_json::Value {
        serde_json::Value::Array(
            items
                .iter()
                .map(|(account, commodity, amount)| {
                    serde_json::json!({
                        "account": account,
                        "commodity": commodity,
                        "amount": amount,
                    })
                })
                .collect(),
        )
    }

    fn txn(date: &str, postings: &[(&str, &str, &str)]) -> (String, serde_json::Value) {
        (date.to_string(), postings_json(postings))
    }

    fn recurring_row(
        status: &str,
        vendor: &str,
        amount: &str,
        cadence_days: u64,
    ) -> (String, serde_json::Value) {
        let pattern = serde_json::json!({
            "vendor": vendor,
            "amount": amount,
            "commodity": "CAD",
            "cadence_days": cadence_days,
        });
        (status.to_string(), pattern)
    }

    #[test]
    fn months_back_label_handles_year_rollover() {
        assert_eq!(months_back_label(day(2026, 3, 15), 6), "2025-10");
        assert_eq!(months_back_label(day(2026, 1, 5), 3), "2025-11");
        assert_eq!(months_back_label(day(2026, 5, 23), 12), "2025-06");
    }

    #[test]
    fn month_range_returns_continuous_oldest_to_newest() {
        let r = month_range(day(2026, 3, 15), 4);
        assert_eq!(r, vec!["2025-12", "2026-01", "2026-02", "2026-03"]);
    }

    #[test]
    fn dashboard_summary_computes_net_worth_excluding_unmatched() {
        let journal = "\
2026-05-01 Salary
    Assets:Northwind:Cash      3000.00 CAD
    Income:Salary                -3000.00 CAD

2026-05-15 Auto-import (Northwind top-up, counter-leg unknown)
    Assets:Globepay:CAD                250.00 CAD
    Unmatched                     -250.00 CAD
";
        let summary = dashboard_summary(
            journal,
            &[],
            &[],
            "CAD",
            day(2026, 5, 23),
            &[],
            6,
            &roster(),
        )
        .unwrap();

        // Listable accounts: Assets:Northwind:Cash (+3000) + Assets:Globepay:CAD (+250)
        // Unmatched (-250) is explicitly excluded from net worth.
        assert_eq!(summary.net_worth_in_base, Some(d("3250.00")));
        assert_eq!(summary.unmatched_balance, Some(d("-250.00")));
    }

    #[test]
    fn dashboard_summary_returns_no_unmatched_when_absent() {
        let journal = "\
2026-05-01 Coffee
    Assets:Northwind:Cash       -5.25 CAD
    Expenses:Coffee                 5.25 CAD
";
        let s = dashboard_summary(
            journal,
            &[],
            &[],
            "CAD",
            day(2026, 5, 23),
            &[],
            6,
            &roster(),
        )
        .unwrap();
        assert_eq!(s.unmatched_balance, None);
    }

    #[test]
    fn bucket_postings_groups_by_month_and_flips_income_sign() {
        let txns = vec![
            txn(
                "2026-04-15",
                &[
                    ("Assets:Northwind:Cash", "CAD", "2500.00"),
                    ("Income:Salary", "CAD", "-2500.00"),
                ],
            ),
            txn(
                "2026-04-18",
                &[
                    ("Assets:Northwind:Cash", "CAD", "-87.42"),
                    ("Expenses:Groceries", "CAD", "87.42"),
                ],
            ),
            txn(
                "2026-05-01",
                &[
                    ("Assets:Northwind:Cash", "CAD", "-5.25"),
                    ("Expenses:Coffee", "CAD", "5.25"),
                ],
            ),
        ];
        let prices = Prices::new();
        let buckets = bucket_parsed_postings(&txns, "CAD", &prices, day(2026, 5, 23), 3);
        // 3 months back from May 23: March, April, May
        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[0].month, "2026-03");
        assert_eq!(buckets[0].income, Decimal::ZERO);
        assert_eq!(buckets[0].spending, Decimal::ZERO);
        assert_eq!(buckets[1].month, "2026-04");
        assert_eq!(buckets[1].income, d("2500.00")); // sign flipped
        assert_eq!(buckets[1].spending, d("87.42"));
        assert_eq!(buckets[2].month, "2026-05");
        assert_eq!(buckets[2].income, Decimal::ZERO);
        assert_eq!(buckets[2].spending, d("5.25"));
    }

    #[test]
    fn bucket_postings_drops_unconvertible_foreign_commodity() {
        let txns = vec![txn(
            "2026-05-10",
            &[
                ("Assets:Globepay:CAD", "USD", "-50.00"),
                ("Expenses:Travel", "USD", "50.00"),
            ],
        )];
        let prices = Prices::new(); // no rates loaded
        let buckets = bucket_parsed_postings(&txns, "CAD", &prices, day(2026, 5, 23), 1);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].month, "2026-05");
        // USD posting with no rate → dropped silently from the trend.
        assert_eq!(buckets[0].spending, Decimal::ZERO);
    }

    #[test]
    fn distill_recurring_keeps_only_confirmed() {
        let rows = vec![
            recurring_row("confirmed", "Netflix", "16.99", 30),
            recurring_row("detected", "Spotify", "10.99", 30),
            recurring_row("dismissed", "Landlord", "1500.00", 30),
            recurring_row("confirmed", "Telus", "55.00", 30),
        ];
        let out = distill_parsed_recurring(&rows);
        assert_eq!(out.len(), 2);
        let vendors: Vec<&str> = out.iter().map(|o| o.vendor.as_str()).collect();
        assert!(vendors.contains(&"Netflix"));
        assert!(vendors.contains(&"Telus"));
    }

    #[test]
    fn next_month_recurring_total_scales_by_cadence() {
        let summary = DashboardSummary {
            base_currency: "CAD".into(),
            net_worth_in_base: None,
            liquid_assets_in_base: None,
            unmatched_balance: None,
            monthly_buckets: vec![],
            recurring: vec![
                RecurringObligation {
                    vendor: "Netflix".into(),
                    amount: d("16.99"),
                    commodity: "CAD".into(),
                    cadence_days: 30, // monthly
                },
                RecurringObligation {
                    vendor: "Coffee shop fund".into(),
                    amount: d("10.00"),
                    commodity: "CAD".into(),
                    cadence_days: 7, // weekly
                },
            ],
        };
        let total = next_month_recurring_total(&summary);
        // Netflix: 16.99 * 30/30 = 16.99
        // Coffee:  10.00 * 30/7  ≈ 42.857...
        // Total   ≈ 59.847...
        assert!(total > d("59"));
        assert!(total < d("60"));
    }

    #[test]
    fn next_month_recurring_total_drops_foreign_commodity() {
        let summary = DashboardSummary {
            base_currency: "CAD".into(),
            net_worth_in_base: None,
            liquid_assets_in_base: None,
            unmatched_balance: None,
            monthly_buckets: vec![],
            recurring: vec![RecurringObligation {
                vendor: "AWS".into(),
                amount: d("12.00"),
                commodity: "USD".into(),
                cadence_days: 30,
            }],
        };
        assert_eq!(next_month_recurring_total(&summary), Decimal::ZERO);
    }

    // --- can_i_afford (conservative after-recurring policy) -------------

    fn fixture_summary(
        net_worth: Option<Decimal>,
        recurring: Vec<RecurringObligation>,
    ) -> DashboardSummary {
        // No liquid account marked → exercises the fallback path.
        DashboardSummary {
            base_currency: "CAD".into(),
            net_worth_in_base: net_worth,
            liquid_assets_in_base: None,
            unmatched_balance: None,
            monthly_buckets: vec![],
            recurring,
        }
    }

    /// Build a bare `AccountSummary` for the `sum_liquid_assets` tests —
    /// only the fields that fn reads (`account`, `total_in_base`, `is_liquid`).
    fn summ(account: &str, total: Option<Decimal>, is_liquid: bool) -> AccountSummary {
        AccountSummary {
            account: account.into(),
            display_name: None,
            last_reconciled_through: None,
            last_statement_balance: None,
            balances: vec![],
            total_in_base: total,
            is_liquid,
        }
    }

    #[test]
    fn sum_liquid_assets_none_when_nothing_marked() {
        // Opt-in: with no account flagged liquid, the result is None so the
        // verdict falls back to net worth (not Some(0), which would read as
        // "you have no spendable money").
        let summaries = vec![
            summ("Assets:Globepay:CAD", Some(d("1000.00")), false),
            summ("Assets:Northwind:TFSA", Some(d("9000.00")), false),
        ];
        assert_eq!(sum_liquid_assets(&summaries), None);
    }

    #[test]
    fn sum_liquid_assets_sums_only_liquid() {
        let summaries = vec![
            summ("Assets:Globepay:CAD", Some(d("1000.00")), true),
            summ("Assets:Northwind:Cash", Some(d("250.00")), true),
            summ("Assets:Northwind:TFSA", Some(d("9000.00")), false), // illiquid: excluded
            summ("Unmatched", Some(d("500.00")), true),        // clearing account: never liquid
        ];
        assert_eq!(sum_liquid_assets(&summaries), Some(d("1250.00")));
    }

    #[test]
    fn sum_liquid_assets_some_zero_when_marked_but_unconvertible() {
        // A marked-liquid account with no convertible balance yields Some(0),
        // not None — a deliberate "no spendable cash", which must read as
        // can't-afford, not as a fallback to full net worth.
        let summaries = vec![summ("Assets:Crypto:BTC", None, true)];
        assert_eq!(sum_liquid_assets(&summaries), Some(Decimal::ZERO));
    }

    fn netflix_monthly() -> RecurringObligation {
        RecurringObligation {
            vendor: "Netflix".into(),
            amount: d("16.99"),
            commodity: "CAD".into(),
            cadence_days: 30,
        }
    }

    #[test]
    fn can_i_afford_true_when_balance_clears_recurring_and_amount() {
        let summary = fixture_summary(Some(d("5000.00")), vec![netflix_monthly()]);
        let verdict = can_i_afford(d("100.00"), &summary);
        assert!(verdict.can_afford);
        // 5000 - 16.99 - 100 = 4883.01
        assert_eq!(verdict.remaining_in_base, d("4883.01"));
        assert_eq!(verdict.policy_label, "Net worth − next month's recurring");
    }

    #[test]
    fn can_i_afford_false_when_amount_exceeds_balance_minus_recurring() {
        let summary = fixture_summary(Some(d("100.00")), vec![netflix_monthly()]);
        let verdict = can_i_afford(d("200.00"), &summary);
        assert!(!verdict.can_afford);
        // 100 - 16.99 - 200 = -116.99 (negative signals can't afford)
        assert_eq!(verdict.remaining_in_base, d("-116.99"));
    }

    #[test]
    fn can_i_afford_false_when_balance_lands_exactly_zero() {
        // Conservative policy: exactly $0 left = can't afford. User
        // confirmed they never want net worth to hit zero on a purchase.
        let summary = fixture_summary(Some(d("100.00")), vec![]);
        let verdict = can_i_afford(d("100.00"), &summary);
        assert!(
            !verdict.can_afford,
            "$0 remaining must read as can't afford"
        );
        assert_eq!(verdict.remaining_in_base, Decimal::ZERO);
    }

    #[test]
    fn can_i_afford_false_when_net_worth_unavailable() {
        let summary = fixture_summary(None, vec![]);
        let verdict = can_i_afford(d("50.00"), &summary);
        assert!(!verdict.can_afford);
        assert_eq!(verdict.remaining_in_base, Decimal::ZERO);
        assert!(
            verdict.policy_label.contains("Net worth unavailable"),
            "label should explain why we can't compute a verdict"
        );
    }

    #[test]
    fn can_i_afford_treats_negative_amount_as_refund() {
        // Refund / income — verdict should improve, never worsen.
        let summary = fixture_summary(Some(d("100.00")), vec![]);
        let baseline = can_i_afford(Decimal::ZERO, &summary);
        let with_refund = can_i_afford(d("-50.00"), &summary);
        assert!(with_refund.remaining_in_base > baseline.remaining_in_base);
        assert!(with_refund.can_afford);
    }
}
