//! Pure helpers for the budget feature (Phase 5).
//!
//! The W4 budget setup screen (5.1) stores each budget target's cadence
//! as a free-form string in `BudgetSetPayload.period` — `"weekly"`,
//! `"biweekly"`, `"monthly"`, and (per the schema comment) future
//! `"custom:N"`. This module owns the canonical parse from that string
//! to a day count, which 5.2's actual-vs-planned view uses to normalize a
//! target against a spending window of arbitrary length.

use super::routines::last_day_of_month;
use crate::db::queries::TxnPostingsRow;
use chrono::{Datelike, Days, NaiveDate};
use ledger_utils::prices::Prices;
use rust_decimal::Decimal;
use serde::Serialize;

/// Normalize a budget period string to its equivalent in days. Returns
/// `None` for unknown / malformed input.
///
/// Supported forms:
///   - `"weekly"` → `Some(7)`
///   - `"biweekly"` → `Some(14)`
///   - `"monthly"` → `Some(30)` (calendar-month approximation; matches the
///     30-day normalizer already in `dashboard::next_month_recurring_total`)
///   - `"custom:N"` where N is a positive integer → `Some(N)`
///   - anything else → `None`
pub fn period_to_days(period: &str) -> Option<u32> {
    match period {
        "weekly" => Some(7),
        "biweekly" => Some(14),
        "monthly" => Some(30),
        custom if custom.starts_with("custom:") => {
            let interval = custom.strip_prefix("custom:")?.parse::<u32>().ok()?;
            (interval > 0).then_some(interval)
        }
        _ => None,
    }
}

/// One budget's actual-vs-planned state over the current period window.
/// Wire shape carries decimals as strings so the serde JSON boundary
/// doesn't need to commit to f64 round-trips — same convention as the
/// dashboard view types.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BudgetProgress {
    pub category: String,
    pub period: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub target: Decimal,
    pub actual: Decimal,
    /// `actual / target` clamped at 0; uncapped on the high end so the UI
    /// can show "over by N%" naturally.
    pub percent_used: f64,
    /// True when actual exceeds target. Equivalent to `percent_used > 100`
    /// but precomputed so the UI doesn't have to redo the comparison.
    pub over_budget: bool,
}

/// Resolve the current period window for a budget cadence as of `as_of`.
/// Returns `None` for unparseable period strings (delegates the vocabulary
/// to [`period_to_days`]).
///
/// The choice of window shape is a policy call — see the function body for
/// the convention this codebase commits to. Window endpoints are inclusive
/// (`[start, end]`), matching how the rest of the codebase uses date ranges
/// (e.g., `TxnFilter.date_from`/`date_to` and `list_transactions_since`).
pub fn current_period_window(period: &str, as_of: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let interval = period_to_days(period)? as u64 - 1;
    match period {
        "weekly" => {
            let sunday =
                as_of.checked_sub_days(Days::new(as_of.weekday().num_days_from_sunday() as u64))?;
            Some((sunday, sunday.checked_add_days(Days::new(6u64))?))
        }
        "monthly" => Some((
            as_of.with_day(1)?,
            as_of.with_day(last_day_of_month(as_of))?,
        )),
        _ => Some((as_of.checked_sub_days(Days::new(interval))?, as_of)), // Bi-weekly and Custom:N should be the only options available at this point because of interval check at the start
    }
}

/// One posting flattened into a budget aggregator's input shape. Decoupled
/// from `TransactionRow` / `TxnPostingsRow` so the pure compute layer
/// doesn't need to know about DbValue or surrealdb internals — callers
/// flatten transactions into this shape at the boundary.
#[derive(Debug, Clone)]
pub struct DatedPosting {
    pub date: NaiveDate,
    pub category: String,
    pub amount: Decimal,
}

/// Compute per-budget progress for the current period window. Budgets whose
/// `period` doesn't parse are skipped (caller is responsible for surfacing
/// them — typically via a "this budget is misconfigured" UI row).
///
/// `postings` should be pre-filtered to expense legs only (other-side legs
/// like `Assets:*` shouldn't be summed against a category budget) and to
/// the broadest window the caller is willing to scan — this fn is happy to
/// receive more than it needs and filters internally on the per-budget
/// window.
/// Tauri-boundary entry: reshape `TxnPostingsRow`s into `DatedPosting`s,
/// filtered to expense-account legs and converted to `base_currency` via
/// the supplied `Prices` table. Same DbValue → serde_json detour pattern
/// as `dashboard::bucket_postings_by_month` — keeps the inner pure-fn
/// testable without constructing DbValues in tests.
pub fn collect_expense_postings(
    rows: &[TxnPostingsRow],
    base_currency: &str,
    prices: &Prices,
) -> Vec<DatedPosting> {
    let parsed: Vec<(String, serde_json::Value)> = rows
        .iter()
        .map(|r| (r.date.clone(), r.postings.clone().into_json_value()))
        .collect();
    collect_expense_parsed(&parsed, base_currency, prices)
}

fn collect_expense_parsed(
    rows: &[(String, serde_json::Value)],
    base_currency: &str,
    prices: &Prices,
) -> Vec<DatedPosting> {
    let mut out = Vec::new();
    for (date_str, postings_json) in rows {
        let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            continue;
        };
        let Some(postings) = postings_json.as_array() else {
            continue;
        };
        for p in postings {
            let Some(account) = p.get("account").and_then(|v| v.as_str()) else {
                continue;
            };
            if !account.starts_with("Expenses:") {
                continue;
            }
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
            let amount = if commodity.eq_ignore_ascii_case(base_currency) {
                qty
            } else {
                match prices.convert(qty, commodity, base_currency, date) {
                    Ok(v) => v,
                    Err(_) => continue, // unconvertible leg — drop silently
                }
            };
            out.push(DatedPosting {
                date,
                category: account.to_string(),
                amount,
            });
        }
    }
    out
}

/// High-level entry that orchestrates the full progress computation: parses
/// the journal for `Prices`, flattens postings, and computes per-budget
/// progress. The Tauri layer just supplies raw inputs — keeps the
/// `ledger-utils` dep contained to core.
pub fn budget_progress_summary(
    journal_content: &str,
    budgets: &[(String, Decimal, String)],
    txn_rows: &[TxnPostingsRow],
    base_currency: &str,
    as_of: NaiveDate,
) -> Result<Vec<BudgetProgress>, crate::ledger::LedgerError> {
    let parsed = crate::ledger::parse(journal_content)?;
    let mut prices = Prices::new();
    prices.insert_from(&parsed);
    let postings = collect_expense_postings(txn_rows, base_currency, &prices);
    Ok(compute_budget_progress(budgets, &postings, as_of))
}

/// Outcome of a `balance_check` — sum of cleared postings on the
/// requested account compared against a user-supplied statement closing
/// balance. `discrepancy = cleared_total - statement_balance`. `ok` is
/// true when discrepancy is exactly zero.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BalanceCheckResult {
    pub account: String,
    pub commodity: String,
    pub cleared_total: Decimal,
    pub statement_balance: Decimal,
    pub discrepancy: Decimal,
    pub ok: bool,
}

/// Compare a sum of cleared postings to a user-supplied statement
/// closing balance.
///
/// Pure fn over numeric inputs — the caller (Tauri layer) is responsible
/// for: (a) querying cleared transactions from the projection; (b)
/// summing the postings on the target account in the target commodity;
/// and (c) handing the resulting `cleared_total` here for comparison.
pub fn balance_check(
    account: &str,
    commodity: &str,
    cleared_total: Decimal,
    statement_balance: Decimal,
) -> BalanceCheckResult {
    let discrepancy = cleared_total - statement_balance;
    BalanceCheckResult {
        account: account.to_string(),
        commodity: commodity.to_string(),
        cleared_total,
        statement_balance,
        discrepancy,
        ok: discrepancy.is_zero(),
    }
}

/// Sum postings on `account` in `commodity` from a list of pre-filtered
/// cleared `TxnPostingsRow`s. Skips legs in other commodities (caller
/// handles FX conversion if needed; for the balance-check use case the
/// statement is in one currency so single-commodity is the right scope).
pub fn sum_cleared_postings(
    rows: &[TxnPostingsRow],
    account: &str,
    commodity: &str,
) -> Decimal {
    let mut total = Decimal::ZERO;
    for row in rows {
        let postings = row.postings.clone().into_json_value();
        let Some(arr) = postings.as_array() else {
            continue;
        };
        for p in arr {
            let Some(acc) = p.get("account").and_then(|v| v.as_str()) else {
                continue;
            };
            if acc != account {
                continue;
            }
            let posting_commodity = p
                .get("commodity")
                .and_then(|v| v.as_str())
                .unwrap_or("CAD");
            if !posting_commodity.eq_ignore_ascii_case(commodity) {
                continue;
            }
            let Some(amount_raw) = p.get("amount").and_then(|v| v.as_str()) else {
                continue;
            };
            let Ok(qty) = amount_raw.parse::<Decimal>() else {
                continue;
            };
            total += qty;
        }
    }
    total
}

pub fn compute_budget_progress(
    budgets: &[(String, Decimal, String)],
    postings: &[DatedPosting],
    as_of: NaiveDate,
) -> Vec<BudgetProgress> {
    budgets
        .iter()
        .filter_map(|(category, target, period)| {
            let (start, end) = current_period_window(period, as_of)?;
            let actual: Decimal = postings
                .iter()
                .filter(|p| p.date >= start && p.date <= end && &p.category == category)
                .map(|p| p.amount)
                .sum();
            let percent_used = if target.is_zero() {
                0.0
            } else {
                // f64 conversion is for display only — exact decimals are
                // already locked in `target` + `actual`.
                let a: f64 = actual.try_into().unwrap_or(0.0);
                let t: f64 = (*target).try_into().unwrap_or(1.0);
                (a / t * 100.0).max(0.0)
            };
            Some(BudgetProgress {
                category: category.clone(),
                period: period.clone(),
                period_start: start,
                period_end: end,
                target: *target,
                actual,
                percent_used,
                over_budget: actual > *target,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekly_is_seven_days() {
        assert_eq!(period_to_days("weekly"), Some(7));
    }

    #[test]
    fn biweekly_is_fourteen_days() {
        assert_eq!(period_to_days("biweekly"), Some(14));
    }

    #[test]
    fn monthly_is_thirty_days() {
        assert_eq!(period_to_days("monthly"), Some(30));
    }

    #[test]
    fn custom_n_parses_positive_integer() {
        assert_eq!(period_to_days("custom:10"), Some(10));
        assert_eq!(period_to_days("custom:1"), Some(1));
        assert_eq!(period_to_days("custom:365"), Some(365));
    }

    #[test]
    fn custom_zero_is_rejected() {
        assert_eq!(period_to_days("custom:0"), None);
    }

    #[test]
    fn custom_non_numeric_is_rejected() {
        assert_eq!(period_to_days("custom:abc"), None);
        assert_eq!(period_to_days("custom:"), None);
    }

    #[test]
    fn unknown_period_is_rejected() {
        assert_eq!(period_to_days(""), None);
        assert_eq!(period_to_days("yearly"), None);
        assert_eq!(period_to_days("daily"), None);
    }

    // --- current_period_window contract (policy-agnostic) ---

    #[test]
    fn window_includes_as_of() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        for period in ["weekly", "biweekly", "monthly", "custom:10"] {
            let (start, end) =
                current_period_window(period, as_of).expect("known period must resolve");
            assert!(
                as_of >= start && as_of <= end,
                "{period} window {start}..={end} must contain as_of {as_of}",
            );
        }
    }

    #[test]
    fn window_returns_none_for_unknown_period() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        assert!(current_period_window("yearly", as_of).is_none());
        assert!(current_period_window("custom:0", as_of).is_none());
    }

    #[test]
    fn window_for_custom_n_spans_n_days() {
        // Custom cadences have no calendar anchor — the window length should
        // match the requested day count regardless of policy choice.
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        let (start, end) = current_period_window("custom:10", as_of).expect("custom:10 resolves");
        let span = (end - start).num_days() + 1; // inclusive both ends
        assert_eq!(span, 10);
    }

    // --- compute_budget_progress aggregation ---

    fn dp(date: NaiveDate, category: &str, amount: i64) -> DatedPosting {
        DatedPosting {
            date,
            category: category.to_string(),
            amount: Decimal::from(amount),
        }
    }

    #[test]
    fn budget_progress_sums_in_window_postings() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        let budgets = vec![(
            "Expenses:Groceries".to_string(),
            Decimal::from(600),
            "monthly".to_string(),
        )];
        // All postings near as_of so any reasonable monthly window includes them.
        let postings = vec![
            dp(as_of, "Expenses:Groceries", 50),
            dp(as_of - chrono::Duration::days(3), "Expenses:Groceries", 75),
            dp(as_of - chrono::Duration::days(7), "Expenses:Groceries", 100),
        ];
        let out = compute_budget_progress(&budgets, &postings, as_of);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].actual, Decimal::from(225));
        assert_eq!(out[0].target, Decimal::from(600));
        assert!(!out[0].over_budget);
        assert!((out[0].percent_used - 37.5).abs() < 0.01);
    }

    #[test]
    fn budget_progress_flags_over_budget() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        let budgets = vec![(
            "Expenses:DiningOut".to_string(),
            Decimal::from(100),
            "monthly".to_string(),
        )];
        let postings = vec![dp(as_of, "Expenses:DiningOut", 150)];
        let out = compute_budget_progress(&budgets, &postings, as_of);
        assert!(out[0].over_budget);
        assert!(out[0].percent_used > 100.0);
    }

    #[test]
    fn budget_progress_ignores_other_categories() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        let budgets = vec![(
            "Expenses:Groceries".to_string(),
            Decimal::from(600),
            "monthly".to_string(),
        )];
        let postings = vec![
            dp(as_of, "Expenses:Groceries", 50),
            dp(as_of, "Expenses:DiningOut", 999),
            dp(as_of, "Expenses:Transit", 999),
        ];
        let out = compute_budget_progress(&budgets, &postings, as_of);
        assert_eq!(out[0].actual, Decimal::from(50));
    }

    #[test]
    fn budget_progress_skips_unparseable_period() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        let budgets = vec![(
            "Expenses:Mystery".to_string(),
            Decimal::from(100),
            "yearly".to_string(),
        )];
        let out = compute_budget_progress(&budgets, &[], as_of);
        assert!(out.is_empty(), "unparseable period should be skipped");
    }

    // --- collect_expense_parsed ---

    fn posting_json(account: &str, amount: &str, commodity: &str) -> serde_json::Value {
        serde_json::json!({
            "account": account,
            "amount": amount,
            "commodity": commodity,
        })
    }

    #[test]
    fn collect_expense_parsed_drops_non_expense_legs() {
        let rows = vec![(
            "2026-05-15".to_string(),
            serde_json::json!([
                posting_json("Expenses:Groceries", "50", "CAD"),
                posting_json("Assets:Northwind:Cash", "-50", "CAD"),
            ]),
        )];
        let out = collect_expense_parsed(&rows, "CAD", &Prices::new());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].category, "Expenses:Groceries");
        assert_eq!(out[0].amount, Decimal::from(50));
    }

    #[test]
    fn collect_expense_parsed_passes_through_base_currency_legs() {
        let rows = vec![(
            "2026-05-15".to_string(),
            serde_json::json!([posting_json("Expenses:DiningOut", "42.50", "CAD")]),
        )];
        let out = collect_expense_parsed(&rows, "CAD", &Prices::new());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].amount.to_string(), "42.50");
    }

    #[test]
    fn collect_expense_parsed_drops_unconvertible_foreign_leg() {
        // USD posting against an empty Prices table — no rate → drop silently.
        let rows = vec![(
            "2026-05-15".to_string(),
            serde_json::json!([posting_json("Expenses:Travel", "100", "USD")]),
        )];
        let out = collect_expense_parsed(&rows, "CAD", &Prices::new());
        assert!(out.is_empty());
    }

    #[test]
    fn collect_expense_parsed_skips_malformed_rows() {
        let rows = vec![
            ("not-a-date".to_string(), serde_json::json!([])),
            (
                "2026-05-15".to_string(),
                serde_json::json!("not an array"),
            ),
        ];
        let out = collect_expense_parsed(&rows, "CAD", &Prices::new());
        assert!(out.is_empty());
    }

    // --- Balance check (5.8) ---

    #[test]
    fn balance_check_zero_discrepancy_is_ok() {
        let r = balance_check("Assets:Summit:Chequing", "CAD", Decimal::from(1500), Decimal::from(1500));
        assert!(r.ok);
        assert_eq!(r.discrepancy, Decimal::ZERO);
    }

    #[test]
    fn balance_check_positive_discrepancy_means_cleared_exceeds_statement() {
        let r = balance_check("Assets:Summit:Chequing", "CAD", Decimal::from(1525), Decimal::from(1500));
        assert!(!r.ok);
        assert_eq!(r.discrepancy, Decimal::from(25));
    }

    #[test]
    fn balance_check_negative_discrepancy_means_cleared_short_of_statement() {
        let r = balance_check("Assets:Summit:Chequing", "CAD", Decimal::from(1480), Decimal::from(1500));
        assert!(!r.ok);
        assert_eq!(r.discrepancy, Decimal::from(-20));
    }

    #[test]
    fn budget_progress_handles_zero_target() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        let budgets = vec![(
            "Expenses:Groceries".to_string(),
            Decimal::ZERO,
            "monthly".to_string(),
        )];
        let postings = vec![dp(as_of, "Expenses:Groceries", 10)];
        let out = compute_budget_progress(&budgets, &postings, as_of);
        // A zero-target budget shouldn't divide-by-zero; percent_used clamps
        // at 0 (caller's UI can choose to render "no target set").
        assert_eq!(out[0].percent_used, 0.0);
        assert!(out[0].over_budget); // 10 > 0
    }
}
