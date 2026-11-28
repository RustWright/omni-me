//! The calendar, read in and never written (ruling 96).
//!
//! A source returns every occurrence in a window; [`sync::sync_once`] diffs that snapshot against
//! what is recorded and appends only the change. Rationale: `docs/src/planning.md` § Calendar.

pub mod day;
pub mod sync;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// Days before today a sync still covers, so an edit to yesterday's event is not missed.
pub const WINDOW_BACK_DAYS: i64 = 1;
/// Days ahead a sync covers: a weekly plan and the month's view both fit inside it.
pub const WINDOW_AHEAD_DAYS: i64 = 56;

/// One occurrence. A recurring series arrives expanded, one value per instance.
///
/// There is no "show me as" field: every timed event is busy, ruling 125 (RULINGS.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Stable for this occurrence within its source. The diff key.
    pub uid: String,
    /// Which of the source's calendars it came from, for display only.
    pub calendar: String,
    pub title: String,
    #[serde(default)]
    pub location: Option<String>,
    pub when: EventTime,
    #[serde(default)]
    pub series: Option<Series>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventTime {
    Timed {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    /// `end` is exclusive, as in iCalendar: a one-day event ends the next day.
    AllDay { start: NaiveDate, end: NaiveDate },
}

/// The recurring series an occurrence belongs to. Answers about places (126) and all-day events
/// (111) are kept per series, so they are asked once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Series {
    pub id: String,
    /// Repeats yearly: birthdays and anniversaries, which are labels and never asked about.
    #[serde(default)]
    pub yearly: bool,
}

impl CalendarEvent {
    /// What a per-series answer is keyed by: the series, or the event itself for a one-off.
    pub fn series_key(&self) -> &str {
        self.series.as_ref().map_or(&self.uid, |s| &s.id)
    }

    pub fn is_all_day(&self) -> bool {
        matches!(self.when, EventTime::AllDay { .. })
    }

    /// The instants it covers. An all-day event covers whole local days in `tz`.
    pub fn span(&self, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
        match &self.when {
            EventTime::Timed { start, end } => (*start, *end),
            EventTime::AllDay { start, end } => (day_start(*start, tz), day_start(*end, tz)),
        }
    }
}

/// Local midnight of `day` in `tz`, as an instant. A midnight skipped by a clock change takes
/// the first instant that exists that day.
pub fn day_start(day: NaiveDate, tz: Tz) -> DateTime<Utc> {
    let naive = day.and_time(NaiveTime::MIN);
    match tz.from_local_datetime(&naive) {
        chrono::LocalResult::Single(t) => t.with_timezone(&Utc),
        chrono::LocalResult::Ambiguous(first, _) => first.with_timezone(&Utc),
        chrono::LocalResult::None => tz
            .from_local_datetime(&(naive + Duration::hours(1)))
            .earliest()
            .map_or_else(|| naive.and_utc(), |t| t.with_timezone(&Utc)),
    }
}

/// The instants a sync covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl Window {
    /// The window a sync at `now` covers, in whole local days.
    pub fn around(now: DateTime<Utc>, tz: Tz) -> Self {
        let today = now.with_timezone(&tz).date_naive();
        Window {
            from: day_start(today - Duration::days(WINDOW_BACK_DAYS), tz),
            to: day_start(today + Duration::days(WINDOW_AHEAD_DAYS), tz),
        }
    }

    pub fn overlaps(&self, event: &CalendarEvent, tz: Tz) -> bool {
        let (start, end) = event.span(tz);
        start < self.to && end > self.from
    }
}

/// A calendar connector. The public engine ships none; a composition root supplies them
/// (ruling 133). It reads only, and owns its own credentials.
#[async_trait]
pub trait CalendarSource: Send + Sync {
    /// Recorded on every change this source produces. Never rename one: its recorded events
    /// would stop matching and all of them would be re-added under the new name.
    fn name(&self) -> &str;

    /// Every occurrence overlapping `window`, from every calendar the source reads. A failure
    /// returns an error, never an empty list: an empty list would remove every recorded event.
    async fn fetch(&self, window: &Window) -> Result<Vec<CalendarEvent>, String>;
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub fn timed(uid: &str, title: &str, start: &str, end: &str) -> CalendarEvent {
        CalendarEvent {
            uid: uid.into(),
            calendar: "main".into(),
            title: title.into(),
            location: None,
            when: EventTime::Timed {
                start: start.parse().unwrap(),
                end: end.parse().unwrap(),
            },
            series: None,
        }
    }

    pub fn all_day(uid: &str, title: &str, start: &str, end: &str) -> CalendarEvent {
        CalendarEvent {
            uid: uid.into(),
            calendar: "main".into(),
            title: title.into(),
            location: None,
            when: EventTime::AllDay {
                start: start.parse().unwrap(),
                end: end.parse().unwrap(),
            },
            series: None,
        }
    }

    pub fn in_series(mut event: CalendarEvent, id: &str, yearly: bool) -> CalendarEvent {
        event.series = Some(Series {
            id: id.into(),
            yearly,
        });
        event
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    const TZ: Tz = chrono_tz::America::Toronto;

    #[test]
    fn all_day_span_is_local_days() {
        let e = all_day("a", "Gwen", "2026-10-01", "2026-10-02");
        let (start, end) = e.span(TZ);
        assert_eq!(start.to_rfc3339(), "2026-10-01T04:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-10-02T04:00:00+00:00");
    }

    #[test]
    fn window_is_whole_local_days() {
        let now: DateTime<Utc> = "2026-11-22T15:00:00Z".parse().unwrap();
        let w = Window::around(now, TZ);
        assert_eq!(w.from.to_rfc3339(), "2026-11-21T05:00:00+00:00");
        assert_eq!(w.to, day_start("2027-01-17".parse().unwrap(), TZ));
    }

    #[test]
    fn overlap_excludes_touching_edges() {
        let w = Window {
            from: "2026-11-22T00:00:00Z".parse().unwrap(),
            to: "2026-11-23T00:00:00Z".parse().unwrap(),
        };
        let before = timed("b", "x", "2026-11-21T23:00:00Z", "2026-11-22T00:00:00Z");
        let inside = timed("i", "x", "2026-11-21T23:00:00Z", "2026-11-22T00:30:00Z");
        assert!(!w.overlaps(&before, Tz::UTC));
        assert!(w.overlaps(&inside, Tz::UTC));
    }

    #[test]
    fn series_key_falls_back_to_the_event() {
        let one_off = timed("u1", "x", "2026-11-22T10:00:00Z", "2026-11-22T11:00:00Z");
        assert_eq!(one_off.series_key(), "u1");
        assert_eq!(in_series(one_off, "s1", false).series_key(), "s1");
    }
}
