//! Coursework deadlines from the LEARN calendar feed. Rulings 122 and 123 (RULINGS.md).
//!
//! Read-only, like the calendar: a deadline attaches to a task, and the feed stays the source of
//! its date. Hand-parsed because only unfolding and five fields are needed; a crate would move
//! the public and private lockfiles together for that.
//! Kinds are told apart only by text; whatever the rules miss is reported, never dropped.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// One deadline as the feed states it, in the user's zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deadline {
    /// The feed's UID; what a task attaches to.
    pub uid: String,
    /// "ME 744", from LOCATION's "ME 744 - Fall 2026".
    pub course: String,
    pub term: String,
    /// The summary without its " - Due" suffix.
    pub title: String,
    /// `YYYY-MM-DD HH:MM` local, the form `tasks::parse_due` reads.
    pub due: String,
}

/// A read of the feed: the deadlines in the window and what could not be classified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeadlineFeed {
    pub source: String,
    pub deadlines: Vec<Deadline>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
    pub content_hash: String,
}

#[derive(Debug, Default)]
struct RawEvent {
    uid: String,
    summary: String,
    location: String,
    description: String,
    start: String,
}

/// Unfold continuation lines (RFC 5545 3.1) and split into events.
fn events(text: &str) -> Vec<RawEvent> {
    let mut unfolded: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match (line.strip_prefix([' ', '\t']), unfolded.last_mut()) {
            (Some(rest), Some(prev)) => prev.push_str(rest),
            _ => unfolded.push(line.to_string()),
        }
    }
    let mut out = Vec::new();
    let mut current: Option<RawEvent> = None;
    for line in unfolded {
        match line.as_str() {
            "BEGIN:VEVENT" => current = Some(RawEvent::default()),
            "END:VEVENT" => out.extend(current.take()),
            _ => {
                let (Some(ev), Some((key, value))) = (current.as_mut(), line.split_once(':'))
                else {
                    continue;
                };
                let name = key.split(';').next().unwrap_or(key);
                let value = unescape(value);
                match name {
                    "UID" => ev.uid = value,
                    "SUMMARY" => ev.summary = value,
                    "LOCATION" => ev.location = value,
                    "DESCRIPTION" => ev.description = value,
                    "DTSTART" => ev.start = value,
                    _ => {}
                }
            }
        }
    }
    out
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// A UTC timestamp (`20261124T045900Z`) in the user's zone, or a floating date-time as given.
/// A bare date is end of day: a deadline on a day is due by its end.
fn local_due(start: &str, tz: Tz) -> Option<NaiveDateTime> {
    if let Some(utc) = start.strip_suffix('Z') {
        let t = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some(
            DateTime::<Utc>::from_naive_utc_and_offset(t, Utc)
                .with_timezone(&tz)
                .naive_local(),
        );
    }
    if let Ok(t) = NaiveDateTime::parse_from_str(start, "%Y%m%dT%H%M%S") {
        return Some(t);
    }
    NaiveDate::parse_from_str(start, "%Y%m%d")
        .ok()?
        .and_hms_opt(23, 59, 0)
}

enum Kind {
    Deadline,
    Material,
    OwnTask,
    Other,
}

fn kind(ev: &RawEvent) -> Kind {
    if ev.location.starts_with("Status:") {
        Kind::OwnTask
    } else if ev.summary.trim_end().ends_with("- Due")
        || ev.description.starts_with("Dropbox:")
        || ev.description.starts_with("Quizzes:")
    {
        Kind::Deadline
    } else if ev.summary.trim_end().ends_with("- Available") {
        Kind::Material
    } else {
        Kind::Other
    }
}

/// Read a feed. Deadlines due before `from` (local) are left out: past terms are in the feed.
pub fn parse_feed(text: &str, tz: Tz, from: NaiveDate) -> DeadlineFeed {
    let mut deadlines = Vec::new();
    let mut problems = Vec::new();
    for ev in events(text) {
        let Some(due) = local_due(&ev.start, tz) else {
            problems.push(format!("`{}`: unreadable start `{}`", ev.summary, ev.start));
            continue;
        };
        if due.date() < from {
            continue;
        }
        match kind(&ev) {
            Kind::Deadline => {}
            Kind::Material | Kind::OwnTask => continue,
            Kind::Other => {
                problems.push(format!(
                    "not classified, left out: `{}` ({}) on {}",
                    ev.summary,
                    ev.location,
                    due.format("%Y-%m-%d")
                ));
                continue;
            }
        }
        let (course, term) = match ev.location.rsplit_once(" - ") {
            Some((c, t)) => (c.trim().to_string(), t.trim().to_string()),
            None => {
                problems.push(format!("`{}`: no course in `{}`", ev.summary, ev.location));
                (ev.location.trim().to_string(), String::new())
            }
        };
        let title = ev
            .summary
            .trim_end()
            .strip_suffix("- Due")
            .unwrap_or(&ev.summary)
            .trim()
            .to_string();
        deadlines.push(Deadline {
            uid: ev.uid,
            course,
            term,
            title,
            due: due.format("%Y-%m-%d %H:%M").to_string(),
        });
    }
    deadlines.sort_by(|a, b| a.due.cmp(&b.due).then(a.uid.cmp(&b.uid)));
    let content_hash =
        super::hub::content_hash(&serde_json::to_string(&deadlines).unwrap_or_default());
    DeadlineFeed {
        source: "learn".to_string(),
        deadlines,
        problems,
        content_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic, in the shape of the real export: UTC stamps one second before local
    /// midnight, a folded description, a material, one of his own LEARN tasks, an event.
    const FEED: &str = "BEGIN:VCALENDAR\r
VERSION:2.0\r
BEGIN:VEVENT\r
UID:quiz-2@learn\r
DTSTART:20261124T045900Z\r
SUMMARY:Quiz 2 - Due\r
LOCATION:ME 744 - Fall 2026\r
DESCRIPTION:Quizzes:\\nQuiz 2 - https://learn.example/d2l/\r
 quizzes/2\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:a5@learn\r
DTSTART:20261216T010000Z\r
SUMMARY:Assignment 5\\, final - Due\r
LOCATION:ME 600 - Fall 2026\r
DESCRIPTION:Dropbox:\\nAssignment 5\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:intro@learn\r
DTSTART:20261201T160000Z\r
SUMMARY:Week 12 notes - Available\r
LOCATION:ME 600 - Fall 2026\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:own@learn\r
DTSTART;VALUE=DATE:20261202\r
SUMMARY:Read chapter 4\r
LOCATION:Status: Completed (Task)\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:orientation@learn\r
DTSTART:20261203T180000Z\r
SUMMARY:Graduate Orientation\r
LOCATION:Grad Ready - Fall 2026\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:old@learn\r
DTSTART:20250921T035959Z\r
SUMMARY:Assignment 1 - Due\r
LOCATION:ME 600 - Fall 2025\r
END:VEVENT\r
END:VCALENDAR\r
";

    fn feed() -> DeadlineFeed {
        parse_feed(
            FEED,
            chrono_tz::America::Toronto,
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        )
    }

    #[test]
    fn deadlines_are_read_in_local_time_and_the_rest_left_out() {
        let f = feed();
        let got: Vec<(&str, &str, &str)> = f
            .deadlines
            .iter()
            .map(|d| (d.course.as_str(), d.title.as_str(), d.due.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("ME 744", "Quiz 2", "2026-11-23 23:59"),
                ("ME 600", "Assignment 5, final", "2026-12-15 20:00"),
            ]
        );
        assert_eq!(f.deadlines[0].term, "Fall 2026");
    }

    #[test]
    fn an_unclassified_event_is_reported_not_dropped() {
        let f = feed();
        assert_eq!(f.problems.len(), 1, "{:?}", f.problems);
        assert!(f.problems[0].contains("Graduate Orientation"));
    }

    #[test]
    fn the_hash_follows_the_deadlines_not_the_file() {
        let a = feed().content_hash;
        let restamped = FEED.replace("VERSION:2.0", "VERSION:2.0\r\nX-STAMP:later");
        let b = parse_feed(
            &restamped,
            chrono_tz::America::Toronto,
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        )
        .content_hash;
        assert_eq!(a, b);
        let moved = FEED.replace("20261124T045900Z", "20261125T045900Z");
        let c = parse_feed(
            &moved,
            chrono_tz::America::Toronto,
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        )
        .content_hash;
        assert_ne!(a, c);
    }
}
