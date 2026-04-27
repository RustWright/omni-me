//! Routine scheduling primitives shared between projections, commands, and UI.
//!
//! The canonical string forms of `Frequency` are the contract between the
//! `RoutineGroupCreated` event payload and the daily-flow scheduler. Anything
//! that persists to disk must round-trip through `Display` / `FromStr`.

use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, NaiveDate};

/// How often a routine group should appear on the daily flow.
///
/// Canonical wire forms: `"daily"`, `"weekly"`, `"biweekly"`, `"monthly"`,
/// `"custom:N"` where N is a positive integer number of days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    Daily,
    Weekly,
    Biweekly,
    Monthly,
    /// Every N days, measured from the group's `created_at` anchor.
    /// Canonical bounds for N enforced by `FromStr` are
    /// `[CUSTOM_FREQUENCY_MIN, CUSTOM_FREQUENCY_MAX]`. Constructing
    /// out-of-bounds values directly compiles but cannot roundtrip.
    Custom(u32),
}

/// Inclusive bounds for `Frequency::Custom(N)`. Lower bound is 2 because
/// `Custom(1)` would shadow `Daily` (two encodings for the same semantic).
/// Upper bound is 31 because routines are for habit formation — anything
/// firing less often than monthly is a calendar task, not a habit. See
/// `project_routine_definition.md` in user memory for the product rationale.
pub const CUSTOM_FREQUENCY_MIN: u32 = 2;
pub const CUSTOM_FREQUENCY_MAX: u32 = 31;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrequencyParseError {
    Empty,
    UnknownVariant(String),
    InvalidCustomInterval(String),
}

impl fmt::Display for FrequencyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "frequency string is empty"),
            Self::UnknownVariant(s) => write!(f, "unknown frequency: {s}"),
            Self::InvalidCustomInterval(s) => {
                write!(
                    f,
                    "invalid custom interval: {s} (expected integer in [{CUSTOM_FREQUENCY_MIN}, {CUSTOM_FREQUENCY_MAX}])"
                )
            }
        }
    }
}

impl std::error::Error for FrequencyParseError {}

impl fmt::Display for Frequency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Frequency::Daily => write!(f, "daily"),
            Frequency::Weekly => write!(f, "weekly"),
            Frequency::Biweekly => write!(f, "biweekly"),
            Frequency::Monthly => write!(f, "monthly"),
            Frequency::Custom(n) => write!(f, "custom:{n}"),
        }
    }
}

impl FromStr for Frequency {
    type Err = FrequencyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "" => Err(FrequencyParseError::Empty),
            "daily" => Ok(Frequency::Daily),
            "weekly" => Ok(Frequency::Weekly),
            "biweekly" => Ok(Frequency::Biweekly),
            "monthly" => Ok(Frequency::Monthly),
            custom if custom.starts_with("custom:") => {
                let tail = custom.strip_prefix("custom:").expect("guard proved prefix");
                let n = tail
                    .parse::<u32>()
                    .map_err(|_| FrequencyParseError::InvalidCustomInterval(custom.into()))?;

                (CUSTOM_FREQUENCY_MIN..=CUSTOM_FREQUENCY_MAX)
                    .contains(&n)
                    .then_some(Frequency::Custom(n))
                    .ok_or(FrequencyParseError::InvalidCustomInterval(custom.into()))
            }
            unknown => Err(FrequencyParseError::UnknownVariant(unknown.into())),
        }
    }
}

impl Frequency {
    /// Should an instance with this frequency, anchored at `anchor`, appear on
    /// `today`? `anchor` is usually the group's `created_at` date.
    pub fn should_run_on(self, anchor: NaiveDate, today: NaiveDate) -> bool {
        if today < anchor {
            return false;
        }
        let days_since = (today - anchor).num_days();
        match self {
            Frequency::Daily => true,
            Frequency::Weekly => days_since % 7 == 0,
            Frequency::Biweekly => days_since % 14 == 0,
            // Clamp the anchor day to the month's last day so a Jan-31 anchor
            // still fires on Feb-28/29, Apr-30, etc. (end-of-month semantics).
            Frequency::Monthly => today.day() == anchor.day().min(last_day_of_month(today)),
            Frequency::Custom(n) => {
                if n == 0 {
                    return false;
                }
                days_since % (n as i64) == 0
            }
        }
    }
}

/// Last calendar day of the month containing `date` (28-31).
fn last_day_of_month(date: NaiveDate) -> u32 {
    [31, 30, 29, 28]
        .into_iter()
        .find(|&day| NaiveDate::from_ymd_opt(date.year(), date.month(), day).is_some())
        .unwrap() // unwrap safe since there is no month with less than 28 days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_roundtrip_through_parse() {
        // Once FromStr is implemented, these must all roundtrip.
        // Custom values must lie within [CUSTOM_FREQUENCY_MIN, CUSTOM_FREQUENCY_MAX]
        // — anything outside is rejected by `FromStr` and cannot roundtrip.
        for freq in [
            Frequency::Daily,
            Frequency::Weekly,
            Frequency::Biweekly,
            Frequency::Monthly,
            Frequency::Custom(CUSTOM_FREQUENCY_MIN),
            Frequency::Custom(3),
            Frequency::Custom(21),
            Frequency::Custom(CUSTOM_FREQUENCY_MAX),
        ] {
            let s = freq.to_string();
            let parsed: Frequency = s.parse().expect("roundtrip parse must succeed");
            assert_eq!(parsed, freq, "roundtrip broke for {s}");
        }
    }

    #[test]
    fn parse_empty_is_error() {
        let err = "".parse::<Frequency>().unwrap_err();
        assert_eq!(err, FrequencyParseError::Empty);
    }

    #[test]
    fn parse_unknown_variant_surfaces_input() {
        let err = "sometimes".parse::<Frequency>().unwrap_err();
        match err {
            FrequencyParseError::UnknownVariant(s) => assert_eq!(s, "sometimes"),
            other => panic!("expected UnknownVariant, got {other:?}"),
        }
    }

    #[test]
    fn parse_custom_zero_is_invalid() {
        let err = "custom:0".parse::<Frequency>().unwrap_err();
        assert!(
            matches!(err, FrequencyParseError::InvalidCustomInterval(_)),
            "custom:0 must be an invalid interval, got {err:?}"
        );
    }

    #[test]
    fn parse_custom_non_numeric_is_invalid() {
        let err = "custom:xyz".parse::<Frequency>().unwrap_err();
        assert!(
            matches!(err, FrequencyParseError::InvalidCustomInterval(_)),
            "custom:xyz must be an invalid interval, got {err:?}"
        );
    }

    #[test]
    fn parse_custom_below_min_is_invalid() {
        // Custom:1 would be redundant with Daily — reject it.
        let err = "custom:1".parse::<Frequency>().unwrap_err();
        assert!(matches!(err, FrequencyParseError::InvalidCustomInterval(_)));
    }

    #[test]
    fn parse_custom_above_max_is_invalid() {
        // Routines are for habit formation; > 31 days isn't habit-shaped.
        for s in ["custom:32", "custom:60", "custom:365", "custom:4000000000"] {
            let err = s.parse::<Frequency>().unwrap_err();
            assert!(
                matches!(err, FrequencyParseError::InvalidCustomInterval(_)),
                "{s} must be invalid, got {err:?}"
            );
        }
    }

    #[test]
    fn parse_custom_at_bounds_is_valid() {
        // Inclusive bounds: both 2 and 31 must parse.
        assert_eq!(
            "custom:2".parse::<Frequency>().unwrap(),
            Frequency::Custom(2)
        );
        assert_eq!(
            "custom:31".parse::<Frequency>().unwrap(),
            Frequency::Custom(31)
        );
    }

    #[test]
    fn parse_is_case_sensitive_and_whitespace_strict() {
        // The wire form is exact: no `.trim()`, no `.to_lowercase()`. Locks the
        // contract against a future "be helpful" refactor that would silently
        // accept variants the event payload writer never produces.
        for s in ["DAILY", "Daily", "Weekly", "MONTHLY", "Custom:3", " daily ", "daily\n"] {
            let err = s.parse::<Frequency>().unwrap_err();
            assert!(
                matches!(err, FrequencyParseError::UnknownVariant(_)),
                "{s:?} must be UnknownVariant, got {err:?}"
            );
        }
        // Leading whitespace defeats the `starts_with("custom:")` guard, so
        // these also fall through to UnknownVariant rather than reaching the
        // numeric parse step.
        for s in [" custom:3", "\tcustom:3"] {
            let err = s.parse::<Frequency>().unwrap_err();
            assert!(
                matches!(err, FrequencyParseError::UnknownVariant(_)),
                "{s:?} must be UnknownVariant, got {err:?}"
            );
        }
    }

    #[test]
    fn should_run_on_daily_is_always_true() {
        let anchor = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 4, 19).unwrap();
        assert!(Frequency::Daily.should_run_on(anchor, today));
    }

    #[test]
    fn should_run_on_weekly_matches_anchor_dow() {
        let anchor = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(); // Wed
        assert!(Frequency::Weekly.should_run_on(anchor, anchor));
        assert!(
            Frequency::Weekly.should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 4, 8).unwrap())
        );
        assert!(
            !Frequency::Weekly.should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 4, 9).unwrap())
        );
    }

    #[test]
    fn should_run_on_biweekly_every_14_days() {
        let anchor = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        assert!(Frequency::Biweekly.should_run_on(anchor, anchor));
        assert!(
            Frequency::Biweekly
                .should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 4, 15).unwrap())
        );
        assert!(
            !Frequency::Biweekly
                .should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 4, 8).unwrap())
        );
    }

    #[test]
    fn should_run_on_monthly_matches_day_of_month() {
        let anchor = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        assert!(Frequency::Monthly.should_run_on(anchor, anchor));
        assert!(
            Frequency::Monthly.should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 5, 15).unwrap())
        );
        assert!(
            !Frequency::Monthly
                .should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 5, 16).unwrap())
        );
    }

    #[test]
    fn should_run_on_custom_every_n_days() {
        let anchor = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let three = Frequency::Custom(3);
        assert!(three.should_run_on(anchor, anchor));
        assert!(three.should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 4, 4).unwrap()));
        assert!(!three.should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 4, 3).unwrap()));
    }

    #[test]
    fn should_run_on_before_anchor_is_false() {
        let anchor = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let past = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        assert!(!Frequency::Daily.should_run_on(anchor, past));
    }

    #[test]
    fn monthly_day_31_anchor_clamps_to_last_day_of_short_months() {
        let anchor = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        // Months without a 31st must fire on their last day.
        for (year, month, expected_last) in [
            (2026, 2, 28),  // non-leap Feb
            (2026, 4, 30),  // April
            (2026, 6, 30),  // June
            (2026, 9, 30),  // September
            (2026, 11, 30), // November
        ] {
            let last = NaiveDate::from_ymd_opt(year, month, expected_last).unwrap();
            assert!(
                Frequency::Monthly.should_run_on(anchor, last),
                "Jan-31 anchor must fire on {year}-{month:02}-{expected_last}"
            );
            // And must NOT fire on the day before the clamped last day.
            let day_before = NaiveDate::from_ymd_opt(year, month, expected_last - 1).unwrap();
            assert!(
                !Frequency::Monthly.should_run_on(anchor, day_before),
                "Jan-31 anchor must not fire on {year}-{month:02}-{:02}",
                expected_last - 1
            );
        }
        // Months that DO have a 31st still fire on the 31st, not earlier.
        assert!(
            Frequency::Monthly
                .should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap())
        );
        assert!(
            !Frequency::Monthly
                .should_run_on(anchor, NaiveDate::from_ymd_opt(2026, 3, 30).unwrap())
        );
    }

    #[test]
    fn monthly_day_29_anchor_handles_leap_year_feb() {
        // 2024 is a leap year — Feb 29 exists, so the day-29 anchor fires on Feb 29.
        let anchor = NaiveDate::from_ymd_opt(2024, 1, 29).unwrap();
        assert!(
            Frequency::Monthly
                .should_run_on(anchor, NaiveDate::from_ymd_opt(2024, 2, 29).unwrap())
        );
        // 2026 is not a leap year — day-29 anchor clamps to Feb 28.
        let anchor_nonleap = NaiveDate::from_ymd_opt(2026, 1, 29).unwrap();
        assert!(
            Frequency::Monthly
                .should_run_on(anchor_nonleap, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap())
        );
        // Day-30 anchor in non-leap Feb also clamps to Feb 28.
        let anchor_30 = NaiveDate::from_ymd_opt(2026, 1, 30).unwrap();
        assert!(
            Frequency::Monthly
                .should_run_on(anchor_30, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap())
        );
    }

    #[test]
    fn last_day_of_month_covers_all_month_lengths() {
        // 31-day months
        for month in [1u32, 3, 5, 7, 8, 10, 12] {
            let date = NaiveDate::from_ymd_opt(2026, month, 1).unwrap();
            assert_eq!(last_day_of_month(date), 31, "month {month}");
        }
        // 30-day months
        for month in [4u32, 6, 9, 11] {
            let date = NaiveDate::from_ymd_opt(2026, month, 1).unwrap();
            assert_eq!(last_day_of_month(date), 30, "month {month}");
        }
        // February — leap and non-leap, plus century-rule edge cases.
        assert_eq!(
            last_day_of_month(NaiveDate::from_ymd_opt(2024, 2, 1).unwrap()),
            29,
            "leap year"
        );
        assert_eq!(
            last_day_of_month(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
            28,
            "non-leap year"
        );
        // 2100 is divisible by 100 but not 400 → not a leap year.
        assert_eq!(
            last_day_of_month(NaiveDate::from_ymd_opt(2100, 2, 1).unwrap()),
            28,
            "century non-leap"
        );
        // 2000 is divisible by 400 → leap year.
        assert_eq!(
            last_day_of_month(NaiveDate::from_ymd_opt(2000, 2, 1).unwrap()),
            29,
            "century leap"
        );
    }
}
