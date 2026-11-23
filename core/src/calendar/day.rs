//! What one local day holds: labels, busy time, clashes, and what to ask about.
//!
//! Rulings 111 (all-day events are labels), 116 (overlaps and duplicates), 125 (every timed
//! event is busy) and 126 (a series without a location is asked about once).

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;

use super::{CalendarEvent, day_start};

/// His recorded answers. Keys are [`CalendarEvent::series_key`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answers {
    /// Whether an all-day event or series takes the day (111).
    pub takes_day: HashMap<String, bool>,
    /// The place a series happens at (126).
    pub place: HashMap<String, String>,
    /// Which event he is going to, per day and clashing pair (116). Holds for that day only.
    pub clash: HashMap<(NaiveDate, String), String>,
}

/// Two timed events on the same day that overlap and are not exact duplicates.
#[derive(Debug, Clone, PartialEq)]
pub struct Clash<'a> {
    /// The one that starts first; travel assumes it until he answers (116).
    pub earlier: &'a CalendarEvent,
    pub later: &'a CalendarEvent,
    /// The uid he said he is going to, if he has answered for this day.
    pub going_to: Option<String>,
}

impl Clash<'_> {
    /// Keys the answer: the two uids, sorted, so either order finds it.
    pub fn pair_key(&self) -> String {
        pair_key(&self.earlier.uid, &self.later.uid)
    }
}

pub fn pair_key(a: &str, b: &str) -> String {
    if a <= b {
        format!("{a}|{b}")
    } else {
        format!("{b}|{a}")
    }
}

/// A question owed to him, asked once per series.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask<'a> {
    /// An all-day event that may take the day (111). Yearly series are labels and never asked.
    TakesDay(&'a CalendarEvent),
    /// A recurring timed series with no location (126). One-offs without one get no buffer.
    Place(&'a CalendarEvent),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Day<'a> {
    /// All-day events that take no time.
    pub labels: Vec<&'a CalendarEvent>,
    /// All-day events he said take the day; the whole day is then busy.
    pub takes_day: Vec<&'a CalendarEvent>,
    /// Timed events touching the day, exact duplicates merged, in start order.
    pub timed: Vec<&'a CalendarEvent>,
    /// Busy intervals clipped to the day, overlaps merged so each minute counts once.
    pub busy: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    pub clashes: Vec<Clash<'a>>,
    pub asks: Vec<Ask<'a>>,
}

impl Day<'_> {
    pub fn busy_minutes(&self) -> i64 {
        self.busy.iter().map(|(s, e)| (*e - *s).num_minutes()).sum()
    }
}

/// Reads `events` for local `day` in `tz`.
pub fn read_day<'a>(
    events: &'a [CalendarEvent],
    answers: &Answers,
    day: NaiveDate,
    tz: Tz,
) -> Day<'a> {
    let from = day_start(day, tz);
    let to = day_start(day + Duration::days(1), tz);
    let mut out = Day::default();
    let mut asked: HashSet<&str> = HashSet::new();

    let mut touching: Vec<&CalendarEvent> = events
        .iter()
        .filter(|e| {
            let (s, end) = e.span(tz);
            s < to && end > from
        })
        .collect();
    touching.sort_by_key(|e| e.span(tz).0);

    let mut seen: HashSet<(&str, DateTime<Utc>, DateTime<Utc>)> = HashSet::new();
    for event in touching {
        let key = event.series_key();
        if event.is_all_day() {
            match answers.takes_day.get(key) {
                Some(true) => out.takes_day.push(event),
                Some(false) => out.labels.push(event),
                None => {
                    out.labels.push(event);
                    let yearly = event.series.as_ref().is_some_and(|s| s.yearly);
                    if !yearly && asked.insert(key) {
                        out.asks.push(Ask::TakesDay(event));
                    }
                }
            }
            continue;
        }
        let (s, e) = event.span(tz);
        if !seen.insert((event.title.trim(), s, e)) {
            continue;
        }
        let recurring = event.series.as_ref().is_some_and(|s| !s.yearly);
        if recurring
            && event
                .location
                .as_deref()
                .is_none_or(|l| l.trim().is_empty())
            && !answers.place.contains_key(key)
            && asked.insert(key)
        {
            out.asks.push(Ask::Place(event));
        }
        out.timed.push(event);
    }

    for (i, &a) in out.timed.iter().enumerate() {
        let a_end = a.span(tz).1;
        for &b in &out.timed[i + 1..] {
            if b.span(tz).0 < a_end {
                let pair = pair_key(&a.uid, &b.uid);
                out.clashes.push(Clash {
                    earlier: a,
                    later: b,
                    going_to: answers.clash.get(&(day, pair)).cloned(),
                });
            }
        }
    }

    if out.takes_day.is_empty() {
        let spans = out.timed.iter().map(|e| {
            let (s, end) = e.span(tz);
            (s.max(from), end.min(to))
        });
        out.busy = merge(spans);
    } else {
        out.busy = vec![(from, to)];
    }
    out
}

/// Sorted intervals, overlapping or touching ones joined.
fn merge(
    spans: impl Iterator<Item = (DateTime<Utc>, DateTime<Utc>)>,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut spans: Vec<_> = spans.filter(|(s, e)| s < e).collect();
    spans.sort();
    let mut out: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (s, e) in spans {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    const DAY: &str = "2026-11-23";

    fn day() -> NaiveDate {
        DAY.parse().unwrap()
    }

    #[test]
    fn birthdays_are_labels_and_never_asked() {
        let events = vec![in_series(
            all_day("b1", "Gwen's birthday", DAY, "2026-11-24"),
            "gwen",
            true,
        )];
        let d = read_day(&events, &Answers::default(), day(), Tz::UTC);
        assert_eq!(d.labels.len(), 1);
        assert!(d.busy.is_empty());
        assert!(d.asks.is_empty());
    }

    #[test]
    fn a_one_off_all_day_event_is_asked_once_and_the_answer_holds() {
        let events = vec![all_day("c1", "Conference", DAY, "2026-11-25")];
        let d = read_day(&events, &Answers::default(), day(), Tz::UTC);
        assert_eq!(d.asks, vec![Ask::TakesDay(&events[0])]);
        assert!(d.busy.is_empty(), "unanswered, it still takes no time");

        let mut answers = Answers::default();
        answers.takes_day.insert("c1".into(), true);
        let d = read_day(&events, &answers, day(), Tz::UTC);
        assert!(d.asks.is_empty());
        assert_eq!(d.busy_minutes(), 24 * 60);
    }

    #[test]
    fn overlaps_count_once_and_duplicates_merge_silently() {
        let events = vec![
            timed(
                "a",
                "Lecture",
                "2026-11-23T10:00:00Z",
                "2026-11-23T11:00:00Z",
            ),
            timed(
                "a2",
                "Lecture",
                "2026-11-23T10:00:00Z",
                "2026-11-23T11:00:00Z",
            ),
            timed(
                "b",
                "Meeting",
                "2026-11-23T10:30:00Z",
                "2026-11-23T12:00:00Z",
            ),
        ];
        let d = read_day(&events, &Answers::default(), day(), Tz::UTC);
        assert_eq!(d.timed.len(), 2, "the exact duplicate merged");
        assert_eq!(d.busy_minutes(), 120);
        assert_eq!(d.clashes.len(), 1);
        assert_eq!(d.clashes[0].earlier.uid, "a");
        assert_eq!(d.clashes[0].going_to, None);
    }

    #[test]
    fn a_clash_answer_holds_for_its_day_only() {
        let events = vec![
            timed(
                "a",
                "Lecture",
                "2026-11-23T10:00:00Z",
                "2026-11-23T11:00:00Z",
            ),
            timed(
                "b",
                "Meeting",
                "2026-11-23T10:30:00Z",
                "2026-11-23T12:00:00Z",
            ),
        ];
        let mut answers = Answers::default();
        answers
            .clash
            .insert((day(), pair_key("b", "a")), "b".into());
        let d = read_day(&events, &answers, day(), Tz::UTC);
        assert_eq!(d.clashes[0].going_to.as_deref(), Some("b"));
        answers.clash.clear();
        answers.clash.insert(
            ("2026-11-24".parse().unwrap(), pair_key("a", "b")),
            "b".into(),
        );
        let d = read_day(&events, &answers, day(), Tz::UTC);
        assert_eq!(d.clashes[0].going_to, None);
    }

    #[test]
    fn busy_is_clipped_to_the_day() {
        let events = vec![timed(
            "late",
            "Flight",
            "2026-11-23T22:00:00Z",
            "2026-11-24T03:00:00Z",
        )];
        let d = read_day(&events, &Answers::default(), day(), Tz::UTC);
        assert_eq!(d.busy_minutes(), 120);
    }

    #[test]
    fn a_series_without_a_location_is_asked_once() {
        let mon = in_series(
            timed(
                "s-1",
                "Club",
                "2026-11-23T18:00:00Z",
                "2026-11-23T19:00:00Z",
            ),
            "club",
            false,
        );
        let mut placed = in_series(
            timed(
                "p-1",
                "Seminar",
                "2026-11-23T13:00:00Z",
                "2026-11-23T14:00:00Z",
            ),
            "seminar",
            false,
        );
        placed.location = Some("B2 3342".into());
        let one_off = timed(
            "o",
            "Coffee",
            "2026-11-23T15:00:00Z",
            "2026-11-23T15:30:00Z",
        );
        let events = vec![mon, placed, one_off];
        let d = read_day(&events, &Answers::default(), day(), Tz::UTC);
        assert_eq!(d.asks, vec![Ask::Place(&events[0])]);

        let mut answers = Answers::default();
        answers.place.insert("club".into(), "Campus".into());
        let d = read_day(&events, &answers, day(), Tz::UTC);
        assert!(d.asks.is_empty());
    }
}
