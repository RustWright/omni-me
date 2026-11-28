//! The month's goals (ruling 134): a few lines in his words, set by the monthly review as a
//! `goal.set` proposal. A later set for the month replaces the earlier one. Rationale:
//! `docs/src/planning.md` § Goals.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use crate::db::Database;

/// The action's name, matched where the brief names it.
pub const GOAL_SET: &str = "goal.set";
/// Most goals one month holds: more than a glance is a list, not goals.
pub const MAX_GOALS: usize = 5;
/// Longest one goal may be.
pub const MAX_GOAL_CHARS: usize = 140;

/// A month's goals, as `month_goals_set` carries them. `aggregate_id` is the month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthGoals {
    /// `YYYY-MM`.
    pub month: String,
    pub goals: Vec<String>,
}

/// `YYYY-MM` for the month `date` falls in.
pub fn month_of(date: NaiveDate) -> String {
    format!("{:04}-{:02}", date.year(), date.month())
}

/// The first day of a `YYYY-MM` month.
pub fn parse_month(text: &str) -> Option<NaiveDate> {
    let (y, m) = text.trim().split_once('-')?;
    if y.len() != 4 || m.len() != 2 {
        return None;
    }
    NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, 1)
}

pub fn valid_month(text: &str) -> Result<(), String> {
    parse_month(text)
        .map(|_| ())
        .ok_or_else(|| "is not a month written YYYY-MM".to_string())
}

pub fn valid_goal(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        Err("is empty".to_string())
    } else {
        Ok(())
    }
}

#[derive(SurrealValue)]
struct GoalsRow {
    body: String,
}

/// The goals set for `month` (`YYYY-MM`), if any.
pub async fn for_month(db: &Database, month: &str) -> Result<Option<MonthGoals>, String> {
    let mut resp = db
        .query("SELECT body FROM type::record('month_goals', $month)")
        .bind(("month", month.to_string()))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let rows: Vec<GoalsRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|r| serde_json::from_str(&r.body).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_parse_or_say_why_not() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        assert_eq!(month_of(d), "2026-11");
        assert_eq!(parse_month("2026-12"), NaiveDate::from_ymd_opt(2026, 12, 1));
        assert!(valid_month("2026-13").is_err());
        assert!(valid_month("2026-1").is_err());
        assert!(valid_month("December").is_err());
        assert!(valid_goal("  ").is_err());
        assert!(valid_goal("Thesis ch. 2 drafted by 1 Dec").is_ok());
    }

    #[tokio::test]
    async fn a_later_set_replaces_the_month() {
        use std::sync::Arc;

        use crate::config::ALL_FEATURES;
        use crate::events::{
            EventType, EventWriter, NewEvent, PlanningProjection, ProjectionRunner,
            SurrealEventStore,
        };

        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(PlanningProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(
            store,
            runner,
            ALL_FEATURES.iter().copied().collect(),
            "server",
        );
        let set = |goals: &[&str]| MonthGoals {
            month: "2026-12".into(),
            goals: goals.iter().map(|g| g.to_string()).collect(),
        };
        for g in [
            set(&["Thesis ch. 2", "Run twice a week"]),
            set(&["Thesis ch. 3"]),
        ] {
            writer
                .append_new(NewEvent {
                    id: None,
                    event_type: EventType::MonthGoalsSet.to_string(),
                    aggregate_id: g.month.clone(),
                    timestamp: chrono::Utc::now(),
                    device_id: "server".into(),
                    payload: serde_json::to_value(&g).unwrap(),
                })
                .await
                .unwrap();
        }
        assert_eq!(
            for_month(&db, "2026-12").await.unwrap(),
            Some(set(&["Thesis ch. 3"]))
        );
        assert_eq!(for_month(&db, "2026-11").await.unwrap(), None);
    }
}
