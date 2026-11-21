//! Tasks: native tasks and projects owned here, and hub tasks read from project indexes.
//!
//! Rulings 90-93 and 121-131 (RULINGS.md). The events live in `events::types`, the tables in
//! `events::tasks_projection`; this module holds the pure rules both share.

pub mod deadlines;
pub mod hub;
pub mod queries;
pub mod repeats;

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};

/// A native task's state. Hub tasks keep the index's own status strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    Done,
    Dropped,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Todo => "todo",
            TaskStatus::Done => "done",
            TaskStatus::Dropped => "dropped",
        }
    }
}

/// A field a `TaskModified` may clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskField {
    Project,
    Estimate,
    Due,
    Scheduled,
    Note,
    Repeat,
}

/// A due date, with a time when the deadline has one (LEARN's 23:59, a 19:00 hand-in).
///
/// Written as `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`, in the user's zone. Due is only ever a real
/// external deadline: ruling 130 (RULINGS.md).
pub fn parse_due(s: &str) -> Result<(NaiveDate, Option<NaiveTime>), String> {
    let s = s.trim();
    let (date, time) = match s.split_once([' ', 'T']) {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| format!("`{s}` is not a date: write YYYY-MM-DD, optionally with HH:MM"))?;
    let time = match time {
        Some(t) => Some(
            NaiveTime::parse_from_str(t, "%H:%M")
                .map_err(|_| format!("`{t}` is not a time: write HH:MM, 24-hour"))?,
        ),
        None => None,
    };
    Ok((date, time))
}

/// The validator form of [`parse_due`], for action arguments.
pub fn valid_due(s: &str) -> Result<(), String> {
    parse_due(s).map(|_| ())
}

/// How a repeating obligation's next occurrence is dated: ruling 131 (RULINGS.md).
///
/// Each occurrence is its own task, dated from the previous occurrence's due date, never from
/// when it was done, so one slipping never moves the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatRule {
    /// Every N days (1-366); biweekly is `every:14d`.
    EveryDays(u32),
    /// The same weekday each week.
    Weekly(Weekday),
    /// This day of each month, clamped to the month's last day.
    MonthlyOn(u32),
    /// The last day of each month (rent).
    MonthlyLast,
    /// This month and day each year; 29 February falls on the 28th outside leap years.
    Yearly { month: u32, day: u32 },
}

impl RepeatRule {
    /// Parse `every:Nd`, `weekly:mon`, `monthly:15`, `monthly:last` or `yearly:12-25`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let bad = || {
            format!(
                "`{s}` is not a repeat rule: use every:Nd, weekly:<day>, monthly:<1-31>, \
                 monthly:last or yearly:MM-DD"
            )
        };
        let (kind, arg) = s.trim().split_once(':').ok_or_else(bad)?;
        match kind {
            "every" => {
                let n: u32 = arg
                    .strip_suffix('d')
                    .ok_or_else(bad)?
                    .parse()
                    .map_err(|_| bad())?;
                if (1..=366).contains(&n) {
                    Ok(RepeatRule::EveryDays(n))
                } else {
                    Err(bad())
                }
            }
            "weekly" => arg
                .parse::<Weekday>()
                .map(RepeatRule::Weekly)
                .map_err(|_| bad()),
            "monthly" if arg == "last" => Ok(RepeatRule::MonthlyLast),
            "monthly" => match arg.parse::<u32>() {
                Ok(d) if (1..=31).contains(&d) => Ok(RepeatRule::MonthlyOn(d)),
                _ => Err(bad()),
            },
            "yearly" => {
                let (m, d) = arg.split_once('-').ok_or_else(bad)?;
                let (month, day): (u32, u32) =
                    (m.parse().map_err(|_| bad())?, d.parse().map_err(|_| bad())?);
                // 2024 is a leap year, so this admits 02-29 and refuses 02-30.
                NaiveDate::from_ymd_opt(2024, month, day).ok_or_else(bad)?;
                Ok(RepeatRule::Yearly { month, day })
            }
            _ => Err(bad()),
        }
    }

    /// The occurrence after one due on `prev`.
    pub fn next_after(self, prev: NaiveDate) -> NaiveDate {
        match self {
            RepeatRule::EveryDays(n) => prev + Duration::days(i64::from(n)),
            RepeatRule::Weekly(day) => {
                let ahead =
                    (day.num_days_from_monday() + 7 - prev.weekday().num_days_from_monday()) % 7;
                prev + Duration::days(if ahead == 0 { 7 } else { i64::from(ahead) })
            }
            RepeatRule::MonthlyOn(d) => on_day_clamped(first_of_next_month(prev), d),
            RepeatRule::MonthlyLast => last_of_month(first_of_next_month(prev)),
            RepeatRule::Yearly { month, day } => {
                let this_year = on_month_day(prev.year(), month, day);
                if this_year > prev {
                    this_year
                } else {
                    on_month_day(prev.year() + 1, month, day)
                }
            }
        }
    }
}

/// The validator form of [`RepeatRule::parse`], for action arguments.
pub fn valid_repeat(s: &str) -> Result<(), String> {
    RepeatRule::parse(s).map(|_| ())
}

fn first_of_next_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("every month has a first") + Months::new(1)
}

fn last_of_month(first: NaiveDate) -> NaiveDate {
    first + Months::new(1) - Duration::days(1)
}

fn on_day_clamped(first: NaiveDate, day: u32) -> NaiveDate {
    let last = last_of_month(first);
    first
        .with_day(day.min(last.day()))
        .expect("clamped to the month's length")
}

fn on_month_day(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day)
        .or_else(|| NaiveDate::from_ymd_opt(year, month, day - 1))
        .expect("only 29 February is missing in some years")
}

/// A native task untouched this long is offered for drop in the weekly review: ruling 92
/// (RULINGS.md), which makes it a setting.
pub const NATIVE_STALE_DAYS: i64 = 28;

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn a_due_date_takes_an_optional_time() {
        assert_eq!(parse_due("2026-12-04").unwrap(), (day("2026-12-04"), None));
        let (d, t) = parse_due("2026-12-04 19:00").unwrap();
        assert_eq!(d, day("2026-12-04"));
        assert_eq!(t, NaiveTime::from_hms_opt(19, 0, 0));
        assert!(parse_due("2026-12-04T23:59").is_ok());
        assert!(parse_due("4 Dec").is_err());
        assert!(parse_due("2026-12-04 7pm").is_err());
    }

    #[test]
    fn repeat_rules_parse_and_refuse() {
        assert_eq!(
            RepeatRule::parse("every:14d"),
            Ok(RepeatRule::EveryDays(14))
        );
        assert_eq!(
            RepeatRule::parse("weekly:sun"),
            Ok(RepeatRule::Weekly(Weekday::Sun))
        );
        assert_eq!(
            RepeatRule::parse("monthly:last"),
            Ok(RepeatRule::MonthlyLast)
        );
        assert_eq!(
            RepeatRule::parse("monthly:15"),
            Ok(RepeatRule::MonthlyOn(15))
        );
        assert_eq!(
            RepeatRule::parse("yearly:12-25"),
            Ok(RepeatRule::Yearly { month: 12, day: 25 })
        );
        for bad in [
            "every:0d",
            "every:14",
            "monthly:0",
            "monthly:32",
            "yearly:02-30",
            "daily",
        ] {
            assert!(RepeatRule::parse(bad).is_err(), "{bad} parsed");
        }
    }

    /// Rent is due the last day of each month, whatever its length.
    #[test]
    fn monthly_last_walks_every_month_end() {
        let r = RepeatRule::MonthlyLast;
        assert_eq!(r.next_after(day("2026-11-30")), day("2026-12-31"));
        assert_eq!(r.next_after(day("2026-12-31")), day("2027-01-31"));
        assert_eq!(r.next_after(day("2027-01-31")), day("2027-02-28"));
        assert_eq!(r.next_after(day("2028-01-31")), day("2028-02-29"));
    }

    /// A day past a short month's end lands on its last day, and the next month returns to it.
    #[test]
    fn monthly_on_clamps_without_drifting() {
        let r = RepeatRule::MonthlyOn(31);
        assert_eq!(r.next_after(day("2027-01-31")), day("2027-02-28"));
        assert_eq!(r.next_after(day("2027-02-28")), day("2027-03-31"));
    }

    #[test]
    fn weekly_and_every_n_days() {
        let sunday = RepeatRule::Weekly(Weekday::Sun);
        assert_eq!(sunday.next_after(day("2026-11-22")), day("2026-11-29"));
        assert_eq!(sunday.next_after(day("2026-11-24")), day("2026-11-29"));
        assert_eq!(
            RepeatRule::EveryDays(14).next_after(day("2026-11-22")),
            day("2026-12-06")
        );
    }

    #[test]
    fn yearly_crosses_the_year_and_survives_a_leap_day() {
        let xmas = RepeatRule::Yearly { month: 12, day: 25 };
        assert_eq!(xmas.next_after(day("2026-12-25")), day("2027-12-25"));
        assert_eq!(xmas.next_after(day("2026-06-01")), day("2026-12-25"));
        let leap = RepeatRule::Yearly { month: 2, day: 29 };
        assert_eq!(leap.next_after(day("2028-02-29")), day("2029-02-28"));
    }
}
