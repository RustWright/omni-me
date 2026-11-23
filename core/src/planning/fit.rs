//! Free time and fit: the day's window, travel, what is left, and whether a plan fits it.
//!
//! Plain code the planning model calls as tools; the model never does this arithmetic (ruling
//! 112). Rationale: `docs/src/planning.md` § Free time.

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use super::places::Places;
use crate::calendar::CalendarEvent;
use crate::calendar::day::{Answers, Day};

pub type Interval = (DateTime<Utc>, DateTime<Utc>);

/// The part of a day a plan may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// From his first activity plus the morning routine, to the bedtime target less the evening
/// routine (ruling 114). The bedtime is the first one after the morning ends, so a target past
/// midnight lands on the next date. `None` when nothing is left between them.
pub fn day_window(
    first_activity: DateTime<Utc>,
    morning_min: u32,
    bedtime: NaiveTime,
    evening_min: u32,
    tz: Tz,
) -> Option<DayWindow> {
    let start = first_activity + Duration::minutes(i64::from(morning_min));
    let local = start.with_timezone(&tz);
    let mut bed = tz
        .from_local_datetime(&local.date_naive().and_time(bedtime))
        .earliest()?
        .with_timezone(&Utc);
    if bed <= start {
        bed = tz
            .from_local_datetime(&(local.date_naive() + Duration::days(1)).and_time(bedtime))
            .earliest()?
            .with_timezone(&Utc);
    }
    let end = bed - Duration::minutes(i64::from(evening_min));
    (end > start).then_some(DayWindow { start, end })
}

/// A trip between two places, busy like an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TravelLeg {
    pub from: String,
    pub to: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreeTime {
    pub window: DayWindow,
    pub travel: Vec<TravelLeg>,
    /// Gaps inside the window between events and travel, in order.
    pub free: Vec<Interval>,
    /// Routine minutes that fall inside the window with no set time (meal prep), taken off the
    /// total rather than placed.
    pub routine_min: u32,
}

impl FreeTime {
    /// What a plan can fill: the gaps, less the unplaced routines, never below zero.
    pub fn free_min(&self) -> i64 {
        let gaps: i64 = self.free.iter().map(|(s, e)| (*e - *s).num_minutes()).sum();
        (gaps - i64::from(self.routine_min)).max(0)
    }
}

/// The timed events he is going to: a clash he answered keeps only his choice, and an
/// unanswered one keeps both, though travel follows the earlier (ruling 116).
fn attended<'a>(day: &Day<'a>) -> (Vec<&'a CalendarEvent>, Vec<&'a str>) {
    let mut dropped: Vec<&'a str> = Vec::new();
    let mut follow_skip: Vec<&'a str> = Vec::new();
    for clash in &day.clashes {
        let (earlier, later): (&'a CalendarEvent, &'a CalendarEvent) = (clash.earlier, clash.later);
        match clash.going_to.as_deref() {
            Some(g) if g == earlier.uid => dropped.push(later.uid.as_str()),
            Some(g) if g == later.uid => dropped.push(earlier.uid.as_str()),
            _ => follow_skip.push(later.uid.as_str()),
        }
    }
    let events = day
        .timed
        .iter()
        .copied()
        .filter(|e| !dropped.contains(&e.uid.as_str()))
        .collect();
    (events, follow_skip)
}

/// The free time in `window`, given the day's events, places and his answers.
///
/// The day starts and ends at home. Before each event at a known place other than the current
/// one, the trip there is busy; an event whose place is unknown leaves him where he was. If the
/// day ends away, the trip home is placed at the end of the window: he stays out until then,
/// which is when he chooses to go home. A day an all-day event takes has no free time.
pub fn free_time(
    day: &Day<'_>,
    window: DayWindow,
    places: &Places,
    answers: &Answers,
    routine_min: u32,
    tz: Tz,
) -> FreeTime {
    let mut busy: Vec<Interval> = Vec::new();
    let mut travel = Vec::new();
    if !day.takes_day.is_empty() {
        return FreeTime {
            window,
            travel,
            free: Vec::new(),
            routine_min,
        };
    }
    let (events, follow_skip) = attended(day);
    let home = places.home().map(str::to_string);
    let mut here = home.clone();
    for event in &events {
        let (start, end) = event.span(tz);
        busy.push((start, end));
        if follow_skip.contains(&event.uid.as_str()) {
            continue;
        }
        let Some(place) = places.place_of(event, answers) else {
            continue;
        };
        if let Some(from) = here.as_deref()
            && let Some(min) = places.travel(from, &place)
            && min > 0
        {
            let leg_start = start - Duration::minutes(i64::from(min));
            travel.push(TravelLeg {
                from: from.to_string(),
                to: place.clone(),
                start: leg_start,
                end: start,
            });
            busy.push((leg_start, start));
        }
        here = Some(place);
    }
    if let (Some(at), Some(home)) = (here.as_deref(), home.as_deref())
        && let Some(min) = places.travel(at, home)
        && min > 0
    {
        let leg_start = window.end - Duration::minutes(i64::from(min));
        travel.push(TravelLeg {
            from: at.to_string(),
            to: home.to_string(),
            start: leg_start,
            end: window.end,
        });
        busy.push((leg_start, window.end));
    }
    FreeTime {
        window,
        travel,
        free: gaps(window, busy),
        routine_min,
    }
}

/// The parts of `window` no busy interval covers.
fn gaps(window: DayWindow, mut busy: Vec<Interval>) -> Vec<Interval> {
    busy.sort();
    let mut out = Vec::new();
    let mut cursor = window.start;
    for (s, e) in busy {
        if e <= cursor || s >= window.end {
            continue;
        }
        if s > cursor {
            out.push((cursor, s.min(window.end)));
        }
        cursor = cursor.max(e);
        if cursor >= window.end {
            break;
        }
    }
    if cursor < window.end {
        out.push((cursor, window.end));
    }
    out
}

/// A timed block in a proposed plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub title: String,
}

/// Why the validator refuses a block. Each reads as a sentence the model can act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Refusal {
    Empty,
    BeforeNow,
    OutsideWindow,
    OnEvent { event: String },
    OnTravel { to: String },
    OverlapsBlock { other: String },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Empty => write!(f, "ends before it starts"),
            Refusal::BeforeNow => write!(f, "starts in the past"),
            Refusal::OutsideWindow => write!(f, "falls outside the day's window"),
            Refusal::OnEvent { event } => write!(f, "overlaps the calendar event \"{event}\""),
            Refusal::OnTravel { to } => write!(f, "overlaps travel to {to}"),
            Refusal::OverlapsBlock { other } => write!(f, "overlaps the block \"{other}\""),
        }
    }
}

/// Every refusal for every block, by block index (ruling 112). Empty means the plan is valid.
pub fn validate(
    blocks: &[Block],
    day: &Day<'_>,
    free: &FreeTime,
    now: DateTime<Utc>,
    tz: Tz,
) -> Vec<(usize, Refusal)> {
    let overlaps = |s: DateTime<Utc>, e: DateTime<Utc>, (s2, e2): Interval| s < e2 && s2 < e;
    let mut out = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        if b.end <= b.start {
            out.push((i, Refusal::Empty));
            continue;
        }
        if b.start < now {
            out.push((i, Refusal::BeforeNow));
        }
        if b.start < free.window.start || b.end > free.window.end {
            out.push((i, Refusal::OutsideWindow));
        }
        if !day.takes_day.is_empty() {
            out.push((
                i,
                Refusal::OnEvent {
                    event: day.takes_day[0].title.clone(),
                },
            ));
        }
        for e in &day.timed {
            if overlaps(b.start, b.end, e.span(tz)) {
                out.push((
                    i,
                    Refusal::OnEvent {
                        event: e.title.clone(),
                    },
                ));
            }
        }
        for leg in &free.travel {
            if overlaps(b.start, b.end, (leg.start, leg.end)) {
                out.push((i, Refusal::OnTravel { to: leg.to.clone() }));
            }
        }
        for (j, other) in blocks.iter().enumerate() {
            if j != i
                && other.end > other.start
                && overlaps(b.start, b.end, (other.start, other.end))
            {
                out.push((
                    i,
                    Refusal::OverlapsBlock {
                        other: other.title.clone(),
                    },
                ));
            }
        }
    }
    out
}

/// Whether a plan fits: what it asks for against what the day has. Never blocks Accept (117).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fit {
    pub planned_min: i64,
    pub free_min: i64,
}

impl Fit {
    /// Minutes over the free time; zero or less fits.
    pub fn over_min(&self) -> i64 {
        self.planned_min - self.free_min
    }
}

/// Blocks plus the untimed list's estimates, against the free time.
pub fn fit(blocks: &[Block], untimed_min: u32, free: &FreeTime) -> Fit {
    let blocked: i64 = blocks
        .iter()
        .map(|b| (b.end - b.start).num_minutes().max(0))
        .sum();
    Fit {
        planned_min: blocked + i64::from(untimed_min),
        free_min: free.free_min(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::places::{Place, pair};
    use super::*;
    use crate::calendar::day::read_day;
    use crate::calendar::test_support::{all_day, timed};

    const TZ: Tz = chrono_tz::America::Toronto;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn places() -> Places {
        let mut travel = HashMap::new();
        travel.insert(pair("Home", "Campus"), 45);
        Places {
            places: vec![
                Place {
                    name: "Home".into(),
                    matches: vec![],
                    home: true,
                },
                Place {
                    name: "Campus".into(),
                    matches: vec!["E7".into()],
                    home: false,
                },
            ],
            travel,
        }
    }

    fn hm(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn the_window_runs_from_the_morning_to_bedtime_less_the_evening() {
        // Up at 10:00 EST (15:00Z), a 120-minute morning, bed at 23:30 less 50 minutes.
        let w = day_window(at("2026-11-23T15:00:00Z"), 120, hm(23, 30), 50, TZ).unwrap();
        assert_eq!(w.start, at("2026-11-23T17:00:00Z"));
        assert_eq!(w.end, at("2026-11-24T03:40:00Z"));
    }

    #[test]
    fn a_bedtime_past_midnight_lands_on_the_next_date() {
        let w = day_window(at("2026-11-23T15:00:00Z"), 120, hm(0, 30), 0, TZ).unwrap();
        assert_eq!(w.end, at("2026-11-24T05:30:00Z"));
    }

    #[test]
    fn a_window_with_nothing_left_is_none() {
        // Up at 23:00 EST, a 20-minute morning, bed at 23:30 less a 60-minute evening.
        assert_eq!(
            day_window(at("2026-11-24T04:00:00Z"), 20, hm(23, 30), 60, TZ),
            None
        );
    }

    fn lecture(uid: &str, start: &str, end: &str) -> CalendarEvent {
        let mut e = timed(uid, "Lecture", start, end);
        e.location = Some("E7 3342".into());
        e
    }

    #[test]
    fn travel_to_campus_and_home_is_busy() {
        // Window 12:00–22:00 EST; one lecture 14:00–15:00 on campus.
        let events = vec![lecture("l", "2026-11-23T19:00:00Z", "2026-11-23T20:00:00Z")];
        let day = read_day(
            &events,
            &Answers::default(),
            "2026-11-23".parse().unwrap(),
            TZ,
        );
        let window = DayWindow {
            start: at("2026-11-23T17:00:00Z"),
            end: at("2026-11-24T03:00:00Z"),
        };
        let free = free_time(&day, window, &places(), &Answers::default(), 0, TZ);
        assert_eq!(free.travel.len(), 2);
        assert_eq!(free.travel[0].start, at("2026-11-23T18:15:00Z"));
        assert_eq!(free.travel[1].start, at("2026-11-24T02:15:00Z"));
        assert_eq!(
            free.free,
            vec![
                (at("2026-11-23T17:00:00Z"), at("2026-11-23T18:15:00Z")),
                (at("2026-11-23T20:00:00Z"), at("2026-11-24T02:15:00Z")),
            ]
        );
        assert_eq!(free.free_min(), 75 + 375);
    }

    #[test]
    fn a_day_taken_whole_has_no_free_time() {
        let events = vec![all_day("c", "Conference", "2026-11-23", "2026-11-24")];
        let mut answers = Answers::default();
        answers.takes_day.insert("c".into(), true);
        let day = read_day(&events, &answers, "2026-11-23".parse().unwrap(), TZ);
        let window = DayWindow {
            start: at("2026-11-23T17:00:00Z"),
            end: at("2026-11-24T03:00:00Z"),
        };
        let free = free_time(&day, window, &places(), &answers, 0, TZ);
        assert_eq!(free.free_min(), 0);
    }

    #[test]
    fn the_validator_names_every_problem() {
        let events = vec![lecture("l", "2026-11-23T19:00:00Z", "2026-11-23T20:00:00Z")];
        let day = read_day(
            &events,
            &Answers::default(),
            "2026-11-23".parse().unwrap(),
            TZ,
        );
        let window = DayWindow {
            start: at("2026-11-23T17:00:00Z"),
            end: at("2026-11-24T03:00:00Z"),
        };
        let free = free_time(&day, window, &places(), &Answers::default(), 0, TZ);
        let block = |s: &str, e: &str, t: &str| Block {
            start: at(s),
            end: at(e),
            title: t.into(),
        };
        let blocks = vec![
            block("2026-11-23T17:00:00Z", "2026-11-23T18:00:00Z", "Essay"),
            block("2026-11-23T18:00:00Z", "2026-11-23T19:30:00Z", "Reading"),
            block("2026-11-23T16:00:00Z", "2026-11-23T16:30:00Z", "Early"),
        ];
        let now = at("2026-11-23T16:30:00Z");
        let refusals = validate(&blocks, &day, &free, now, TZ);
        assert!(!refusals.iter().any(|(i, _)| *i == 0), "{refusals:?}");
        assert!(refusals.contains(&(
            1,
            Refusal::OnTravel {
                to: "Campus".into()
            }
        )));
        assert!(refusals.contains(&(
            1,
            Refusal::OnEvent {
                event: "Lecture".into()
            }
        )));
        assert!(refusals.contains(&(2, Refusal::BeforeNow)));
        assert!(refusals.contains(&(2, Refusal::OutsideWindow)));

        let f = fit(&blocks[..1], 30, &free);
        assert_eq!(f.planned_min, 90);
        assert!(f.over_min() < 0);
    }
}
