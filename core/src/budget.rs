//! Pure helpers for the budget feature (Phase 5).
//!
//! The W4 budget setup screen (5.1) stores each budget target's cadence
//! as a free-form string in `BudgetSetPayload.period` — `"weekly"`,
//! `"biweekly"`, `"monthly"`, and (per the schema comment) future
//! `"custom:N"`. This module owns the canonical parse from that string
//! to a day count, which 5.2's actual-vs-planned view will use to
//! normalize a target against a spending window of arbitrary length.

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
}
