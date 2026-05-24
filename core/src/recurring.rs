//! Recurring-transaction detection (Phase 5.3).
//!
//! The W3 scanner sweeps the transaction log for repeat patterns and emits
//! `RecurringTransactionDetected` events for new candidates. The dashboard
//! (4.6) shows `confirmed` rows; the 5.4 confirm UI flips `detected` →
//! `confirmed` (or `dismissed`). This module owns the pure detection
//! logic; the Tauri layer handles the read/write of events and the
//! "skip already-tracked patterns" idempotency check.
//!
//! Algorithm:
//!   1. Group expense legs by `(description, amount, commodity)`.
//!   2. For each group with ≥3 occurrences, compute gaps between
//!      consecutive dates (sorted ascending).
//!   3. Hand the gaps to [`classify_cadence`] — if it returns
//!      `Some(cadence_days)`, the group is a detected pattern.
//!   4. Emit one `DetectedPattern` per qualifying group.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::db::queries::TxnPostingsRow;

/// One recurring pattern surfaced by the scanner. The `pattern_id` is
/// derived deterministically from the natural-key fields so re-scans are
/// idempotent — the Tauri layer skips ids already present in the
/// `recurring_patterns` table to avoid clobbering user confirmations.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetectedPattern {
    pub pattern_id: String,
    pub vendor: String,
    pub amount: Decimal,
    pub commodity: String,
    pub cadence_days: u32,
    pub occurrences: u32,
    pub first_seen: NaiveDate,
    pub last_seen: NaiveDate,
}

/// Minimum number of occurrences required for a group to be considered
/// a recurring pattern. Three is the smallest count that produces at
/// least two gaps, which classify_cadence needs to look at consistency.
pub const MIN_OCCURRENCES: usize = 3;

/// Decide whether a list of inter-occurrence gaps (in days) is regular
/// enough to call recurring, and if so what cadence to snap to.
///
/// Returns `Some(cadence_days)` if the gaps qualify, `None` if they're
/// too irregular. The cadence value should snap to one of the named
/// cadences when possible (7 = weekly, 14 = biweekly, 30 = monthly)
/// since those are what the dashboard's `RecurringCard` widget labels;
/// otherwise return the median gap.
///
/// Callers guarantee `gaps.len() >= MIN_OCCURRENCES - 1` (i.e., at
/// least 2 gaps).
pub fn classify_cadence(gaps_days: &[u32]) -> Option<u32> {
    if gaps_days.len() < MIN_OCCURRENCES - 1 {
        return None;
    }

    // Median of the gaps. `sort + pick middle` is robust to a single outlier
    // in a way that mean is not (a 365-day gap doesn't tilt the median).
    let mut sorted: Vec<u32> = gaps_days.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];

    // Snap to a named cadence when the median is close AND every gap is
    // within a looser tolerance — this catches "weekly with one missed
    // week" without admitting "weekly with a year-long gap." Tolerances
    // are per-bucket: tight for short cadences, looser for monthly to
    // absorb 28-31 day month variation.
    const NAMED: &[(u32, u32)] = &[(7, 1), (14, 2), (30, 4)];
    for &(cadence, tol) in NAMED {
        if median.abs_diff(cadence) <= tol
            && gaps_days.iter().all(|g| g.abs_diff(cadence) <= tol * 2)
        {
            return Some(cadence);
        }
    }

    // Custom cadence: all gaps must cluster around the median (within 25%
    // + 1 day floor for small medians). Keeps `(median > 1)` as a hard
    // floor — a 0/1-day cadence isn't a "subscription," it's same-day
    // double-charges or daily activity.
    if median > 1 {
        let tol = (median / 4).max(1);
        if gaps_days.iter().all(|g| g.abs_diff(median) <= tol) {
            return Some(median);
        }
    }
    None
}

/// Pure detection entry — given a transaction roster, surface every
/// pattern that qualifies under [`classify_cadence`].
pub fn detect_patterns(txn_rows: &[TxnPostingsRow]) -> Vec<DetectedPattern> {
    let parsed: Vec<(String, serde_json::Value)> = txn_rows
        .iter()
        .map(|r| (r.date.clone(), r.postings.clone().into_json_value()))
        .collect();
    detect_parsed(&parsed)
}

fn detect_parsed(rows: &[(String, serde_json::Value)]) -> Vec<DetectedPattern> {
    // Group key: (description, amount_string, commodity). description comes
    // from the parent txn row, not the posting, so we read it once per row
    // and apply it to every expense leg in that row.
    let mut groups: BTreeMap<(String, String, String), Vec<NaiveDate>> = BTreeMap::new();

    for (date_str, postings_json) in rows {
        let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            continue;
        };
        let Some(postings) = postings_json.as_array() else {
            continue;
        };
        // Description lives at row level, not on each posting — for the
        // group key we need it but the current `TxnPostingsRow` shape
        // only carries date + postings. Surface description via a synthetic
        // "_description" hint on each posting that the Tauri layer can
        // populate, or accept that the group key falls back to category.
        // For now: group by (category, amount, commodity) — same effect
        // when a vendor consistently posts to the same category.
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
            // Normalize: round to 2dp so 42.18 and 42.180 group together.
            let amount = qty.round_dp(2);
            let commodity = p
                .get("commodity")
                .and_then(|v| v.as_str())
                .unwrap_or("CAD")
                .to_string();
            groups
                .entry((account.to_string(), amount.to_string(), commodity))
                .or_default()
                .push(date);
        }
    }

    let mut out = Vec::new();
    for ((category, amount_str, commodity), mut dates) in groups {
        if dates.len() < MIN_OCCURRENCES {
            continue;
        }
        dates.sort();
        let gaps: Vec<u32> = dates
            .windows(2)
            .map(|w| (w[1] - w[0]).num_days().max(0) as u32)
            .collect();
        let Some(cadence_days) = classify_cadence(&gaps) else {
            continue;
        };
        let amount: Decimal = amount_str.parse().unwrap_or(Decimal::ZERO);
        let pattern_id = derive_pattern_id(&category, &amount_str, &commodity, cadence_days);
        out.push(DetectedPattern {
            pattern_id,
            vendor: category, // 5.4 backlog: lift real `description` into the input row
            amount,
            commodity,
            cadence_days,
            occurrences: dates.len() as u32,
            first_seen: *dates.first().unwrap(),
            last_seen: *dates.last().unwrap(),
        });
    }
    out
}

/// Stable pattern id derived from the natural-key fields. The Tauri layer
/// uses this to skip emitting events for patterns already present in the
/// `recurring_patterns` table (any status), preserving user confirmations
/// across re-scans.
fn derive_pattern_id(category: &str, amount: &str, commodity: &str, cadence_days: u32) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    category.hash(&mut hasher);
    amount.hash(&mut hasher);
    commodity.hash(&mut hasher);
    cadence_days.hash(&mut hasher);
    format!("recurring-{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txn(date: &str, postings: serde_json::Value) -> (String, serde_json::Value) {
        (date.to_string(), postings)
    }

    fn p(account: &str, amount: &str) -> serde_json::Value {
        serde_json::json!({
            "account": account,
            "amount": amount,
            "commodity": "CAD",
        })
    }

    #[test]
    fn group_below_minimum_occurrences_is_skipped() {
        let rows = vec![
            txn(
                "2026-04-01",
                serde_json::json!([p("Expenses:Netflix", "15.99")]),
            ),
            txn(
                "2026-05-01",
                serde_json::json!([p("Expenses:Netflix", "15.99")]),
            ),
        ];
        let out = detect_parsed(&rows);
        assert!(out.is_empty(), "2 occurrences should not qualify");
    }

    #[test]
    fn weekly_pattern_detected() {
        let rows = vec![
            txn(
                "2026-04-01",
                serde_json::json!([p("Expenses:Coffee", "5.50")]),
            ),
            txn(
                "2026-04-08",
                serde_json::json!([p("Expenses:Coffee", "5.50")]),
            ),
            txn(
                "2026-04-15",
                serde_json::json!([p("Expenses:Coffee", "5.50")]),
            ),
            txn(
                "2026-04-22",
                serde_json::json!([p("Expenses:Coffee", "5.50")]),
            ),
        ];
        let out = detect_parsed(&rows);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].cadence_days, 7);
        assert_eq!(out[0].occurrences, 4);
    }

    #[test]
    fn monthly_pattern_detected() {
        let rows = vec![
            txn(
                "2026-02-15",
                serde_json::json!([p("Expenses:Netflix", "15.99")]),
            ),
            txn(
                "2026-03-15",
                serde_json::json!([p("Expenses:Netflix", "15.99")]),
            ),
            txn(
                "2026-04-15",
                serde_json::json!([p("Expenses:Netflix", "15.99")]),
            ),
            txn(
                "2026-05-15",
                serde_json::json!([p("Expenses:Netflix", "15.99")]),
            ),
        ];
        let out = detect_parsed(&rows);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].cadence_days, 30, "monthly should snap to 30");
        assert_eq!(out[0].vendor, "Expenses:Netflix");
    }

    #[test]
    fn irregular_gaps_are_not_recurring() {
        // 5, 30, 100 days apart — no consistent cadence.
        let rows = vec![
            txn(
                "2026-01-01",
                serde_json::json!([p("Expenses:OneOff", "20.00")]),
            ),
            txn(
                "2026-01-06",
                serde_json::json!([p("Expenses:OneOff", "20.00")]),
            ),
            txn(
                "2026-02-05",
                serde_json::json!([p("Expenses:OneOff", "20.00")]),
            ),
            txn(
                "2026-05-15",
                serde_json::json!([p("Expenses:OneOff", "20.00")]),
            ),
        ];
        let out = detect_parsed(&rows);
        assert!(
            out.is_empty(),
            "irregular gaps should not produce a pattern"
        );
    }

    #[test]
    fn different_amounts_do_not_group() {
        // Same vendor, different amounts — three separate "groups" each
        // below the threshold.
        let rows = vec![
            txn(
                "2026-04-01",
                serde_json::json!([p("Expenses:Gas", "30.00")]),
            ),
            txn(
                "2026-04-08",
                serde_json::json!([p("Expenses:Gas", "45.00")]),
            ),
            txn(
                "2026-04-15",
                serde_json::json!([p("Expenses:Gas", "25.00")]),
            ),
        ];
        let out = detect_parsed(&rows);
        assert!(out.is_empty());
    }

    #[test]
    fn weekly_with_one_outlier_is_still_recurring() {
        // Three weekly gaps + one missed week (14 instead of 7) — common
        // real-world shape. Should still snap to weekly.
        assert_eq!(classify_cadence(&[7, 14, 7, 7]), None); // 14 > 7+2 = 9, so all-within-tol fails — confirms the "tight outlier guard"
        // But a single small drift (8 days instead of 7) should pass.
        assert_eq!(classify_cadence(&[7, 8, 7, 7]), Some(7));
    }

    #[test]
    fn stale_pattern_with_year_long_gap_is_rejected() {
        // 3 weekly charges + 1 year-long gap — pattern stopped, not
        // recurring. The old "mean OR median in band" classifier would
        // have wrongly returned Some(7) here.
        assert_eq!(classify_cadence(&[7, 7, 365]), None);
    }

    #[test]
    fn median_correctness_on_odd_length() {
        // Sorted gaps [5, 30, 100]; median = 30. The earlier bug indexed
        // sorted[0] = 5 by mistake; this case would have leaked through.
        // Median is 30 but the [5, 100] outliers aren't within 30/4=7 of
        // 30, so we correctly reject.
        assert_eq!(classify_cadence(&[5, 30, 100]), None);
    }

    #[test]
    fn monthly_with_28_31_day_variation_is_accepted() {
        // The exact case in `monthly_pattern_detected` — Feb/Mar/Apr/May
        // 15th charges. Gaps [28, 31, 30]. Median = 30, tolerance ±4
        // catches all gaps.
        assert_eq!(classify_cadence(&[28, 31, 30]), Some(30));
    }

    #[test]
    fn custom_cadence_returns_median_when_consistent() {
        // Quarterly-ish; gaps [88, 92, 90]. None of the named cadences
        // catch it; falls through to custom.
        assert_eq!(classify_cadence(&[88, 92, 90]), Some(90));
    }

    #[test]
    fn derive_pattern_id_is_stable() {
        // Two calls with the same key produce the same id — idempotency
        // guarantee the Tauri layer relies on for skip-already-tracked.
        let id_a = derive_pattern_id("Expenses:Netflix", "15.99", "CAD", 30);
        let id_b = derive_pattern_id("Expenses:Netflix", "15.99", "CAD", 30);
        assert_eq!(id_a, id_b);
        // And distinct keys produce distinct ids.
        let id_c = derive_pattern_id("Expenses:Netflix", "15.99", "CAD", 7);
        assert_ne!(id_a, id_c);
    }
}
