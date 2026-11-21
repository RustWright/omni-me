//! Repeating obligations: each occurrence is its own task. Ruling 131 (RULINGS.md).
//!
//! The next occurrence is dated from the previous one's date by its rule, never from when it was
//! done, and appears once the latest is closed or its date has come. A series stops when its
//! latest occurrence no longer carries a rule.

use std::collections::BTreeMap;

use chrono::{NaiveDate, Utc};
use surrealdb::types::SurrealValue;

use super::RepeatRule;
use crate::db::Database;
use crate::events::{EventError, EventType, EventWriter, NewEvent, TaskCreatedPayload};

/// One occurrence as the job needs it.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub struct Occurrence {
    pub task_id: String,
    pub series_id: Option<String>,
    pub repeat_rule: Option<String>,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub scheduled: Option<String>,
    pub status: String,
    pub title: String,
    pub project_id: Option<String>,
    pub estimate_min: Option<i64>,
    pub note: Option<String>,
}

fn day(s: &Option<String>) -> Option<NaiveDate> {
    s.as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

impl Occurrence {
    /// The date the rule steps from: the due date, else the scheduled day.
    fn anchor(&self) -> Option<NaiveDate> {
        day(&self.due_date).or_else(|| day(&self.scheduled))
    }

    fn series(&self) -> &str {
        self.series_id.as_deref().unwrap_or(&self.task_id)
    }
}

/// The occurrences to create now, one per series at most.
///
/// Ids are derived from the series and the date, so a replay or a second run writes the same
/// task rather than a duplicate.
pub fn next_occurrences(all: &[Occurrence], today: NaiveDate) -> Vec<TaskCreatedPayload> {
    let mut latest: BTreeMap<&str, &Occurrence> = BTreeMap::new();
    for o in all {
        let Some(anchor) = o.anchor() else { continue };
        let slot = latest.entry(o.series()).or_insert(o);
        if slot.anchor().is_none_or(|a| anchor > a) {
            *slot = o;
        }
    }
    latest
        .into_iter()
        .filter_map(|(series, last)| {
            let rule = RepeatRule::parse(last.repeat_rule.as_deref()?).ok()?;
            let anchor = last.anchor()?;
            if last.status == "todo" && anchor > today {
                return None;
            }
            let next = rule.next_after(anchor);
            let next_text = next.format("%Y-%m-%d").to_string();
            let due = last.due_date.as_ref().map(|_| match &last.due_time {
                Some(t) => format!("{next_text} {t}"),
                None => next_text.clone(),
            });
            let scheduled = last.due_date.is_none().then_some(next);
            Some(TaskCreatedPayload {
                task_id: format!("{series}-{next_text}"),
                title: last.title.clone(),
                project_id: last.project_id.clone(),
                estimate_min: last.estimate_min.and_then(|m| u32::try_from(m).ok()),
                due,
                scheduled,
                repeat: last.repeat_rule.clone(),
                series_id: Some(series.to_string()),
                hub_id: None,
                deadline_uid: None,
                note: last.note.clone(),
            })
        })
        .collect()
}

/// Read every repeating task, create what is due, and return how many were created.
pub async fn run(db: &Database, writer: &EventWriter, today: NaiveDate) -> Result<usize, String> {
    let mut resp = db
        .query(
            "SELECT task_id, series_id, repeat_rule, due_date, due_time, scheduled, status, title,
                    project_id, estimate_min, note
             FROM tasks WHERE repeat_rule != NONE OR series_id != NONE",
        )
        .await
        .map_err(|e| e.to_string())?;
    let all: Vec<Occurrence> = resp.take(0).map_err(|e| e.to_string())?;
    let next = next_occurrences(&all, today);
    for payload in &next {
        let event = NewEvent {
            id: None,
            event_type: EventType::TaskCreated.to_string(),
            aggregate_id: payload.task_id.clone(),
            timestamp: Utc::now(),
            device_id: writer.device_id().to_string(),
            payload: serde_json::to_value(payload).map_err(|e| e.to_string())?,
        };
        writer
            .append_new(event)
            .await
            .map_err(|e: crate::events::WriteError| e.to_string())?;
    }
    Ok(next.len())
}

/// Hourly is plenty for rules counted in days.
pub const TICK: std::time::Duration = std::time::Duration::from_secs(3600);

/// The user's today, in the zone the check-in records. UTC when none is set.
pub async fn today(db: &Database) -> Result<NaiveDate, EventError> {
    let config = crate::events::load_shared(db).await?;
    let tz = crate::assistant::scheduled::user_timezone(&config);
    Ok(Utc::now().with_timezone(&tz).date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn rent(id: &str, series: Option<&str>, due: &str, status: &str) -> Occurrence {
        Occurrence {
            task_id: id.into(),
            series_id: series.map(str::to_string),
            repeat_rule: Some("monthly:last".into()),
            due_date: Some(due.into()),
            due_time: None,
            scheduled: None,
            status: status.into(),
            title: "Pay rent".into(),
            project_id: None,
            estimate_min: Some(5),
            note: None,
        }
    }

    #[test]
    fn the_next_appears_on_the_due_day_not_before() {
        let all = [rent("R1", None, "2026-11-30", "todo")];
        assert!(next_occurrences(&all, d("2026-11-29")).is_empty());
        let next = next_occurrences(&all, d("2026-11-30"));
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].task_id, "R1-2026-12-31");
        assert_eq!(next[0].due.as_deref(), Some("2026-12-31"));
        assert_eq!(next[0].series_id.as_deref(), Some("R1"));
    }

    /// Paying early brings the next month forward in view, never in date.
    #[test]
    fn closing_early_shows_the_next_with_its_own_date() {
        let all = [rent("R1", None, "2026-11-30", "done")];
        let next = next_occurrences(&all, d("2026-11-10"));
        assert_eq!(next[0].due.as_deref(), Some("2026-12-31"));
    }

    /// One slipping never moves the next: December is dated from November's due date even
    /// though November is still open and late.
    #[test]
    fn a_late_occurrence_does_not_move_the_next() {
        let all = [
            rent("R1", None, "2026-11-30", "todo"),
            rent("R1-2026-12-31", Some("R1"), "2026-12-31", "todo"),
        ];
        assert!(next_occurrences(&all, d("2026-12-15")).is_empty());
        let next = next_occurrences(&all, d("2026-12-31"));
        assert_eq!(next[0].task_id, "R1-2027-01-31");
    }

    #[test]
    fn clearing_the_rule_on_the_latest_ends_the_series() {
        let mut last = rent("R1-2026-12-31", Some("R1"), "2026-12-31", "done");
        last.repeat_rule = None;
        let all = [rent("R1", None, "2026-11-30", "done"), last];
        assert!(next_occurrences(&all, d("2027-01-15")).is_empty());
    }

    /// Biweekly bookkeeping has no deadline, so it steps its scheduled day.
    #[test]
    fn a_rule_without_a_due_date_steps_the_scheduled_day() {
        let mut b = rent("B1", None, "", "todo");
        b.due_date = None;
        b.scheduled = Some("2026-11-22".into());
        b.repeat_rule = Some("every:14d".into());
        let next = next_occurrences(&[b], d("2026-11-22"));
        assert_eq!(next[0].scheduled, Some(d("2026-12-06")));
        assert_eq!(next[0].due, None);
    }

    #[test]
    fn a_due_time_carries_over() {
        let mut t = rent("T1", None, "2026-11-30", "todo");
        t.due_time = Some("19:00".into());
        let next = next_occurrences(&[t], d("2026-11-30"));
        assert_eq!(next[0].due.as_deref(), Some("2026-12-31 19:00"));
    }
}
