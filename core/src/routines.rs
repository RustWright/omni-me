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
    Custom(u32),
}

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
                    "invalid custom interval: {s} (expected positive integer)"
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

                n.gt(&0u32)
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
            Frequency::Monthly => today.day() == anchor.day(),
            Frequency::Custom(n) => {
                if n == 0 {
                    return false;
                }
                days_since % (n as i64) == 0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_roundtrip_through_parse() {
        // Once FromStr is implemented, these must all roundtrip.
        for freq in [
            Frequency::Daily,
            Frequency::Weekly,
            Frequency::Biweekly,
            Frequency::Monthly,
            Frequency::Custom(3),
            Frequency::Custom(100),
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
}
