use chrono::{NaiveDate, Utc};
use chrono_tz::Tz;

/// A date in the user's local timezone.
///
/// All constructors require a `&Tz`, preventing accidental use of
/// `Utc::now()` for user-facing date display. Event timestamps should
/// continue using `chrono::Utc` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserDate(NaiveDate);

impl UserDate {
    /// Today's date in the user's local timezone.
    pub fn today(tz: &Tz) -> Self {
        Self(Utc::now().with_timezone(tz).date_naive())
    }

    /// Yesterday's date in the user's local timezone.
    pub fn yesterday(tz: &Tz) -> Self {
        let today = Self::today(tz);
        Self(today.0 - chrono::Duration::days(1))
    }

    /// A date N days before today in the user's local timezone.
    pub fn days_ago(tz: &Tz, n: i64) -> Self {
        let today = Self::today(tz);
        Self(today.0 - chrono::Duration::days(n))
    }

    /// Format as YYYY-MM-DD (the format used for note.date and routine date fields).
    pub fn to_date_string(&self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }

    /// The underlying NaiveDate, for chrono operations like day-of-week formatting.
    pub fn naive_date(&self) -> NaiveDate {
        self.0
    }

    /// Format with a custom format string.
    pub fn format(&self, fmt: &str) -> String {
        self.0.format(fmt).to_string()
    }
}

impl std::fmt::Display for UserDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_date_string())
    }
}
