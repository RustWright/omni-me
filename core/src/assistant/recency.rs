//! A belief is proposed only after the run has looked at the last month.
//!
//! Looking is the requirement, not citing: what the recent records say is the
//! model's call. Why: `docs/src/assistant.md` § A belief looks at the last month.

use chrono::{Duration, NaiveDate};
use serde_json::Value;

use super::session::TurnRecord;

/// How far back "recent" reaches, counted from the day the run happens.
pub const RECENT_DAYS: i64 = 30;

/// Why a `belief.record` proposal has to wait, or `None` once an earlier call
/// in the run listed records with a date range reaching the last
/// [`RECENT_DAYS`] days.
pub fn belief_needs_a_recent_look(earlier: &[TurnRecord], today: NaiveDate) -> Option<String> {
    let since = today - Duration::days(RECENT_DAYS);
    let looked = earlier.iter().any(|turn| {
        turn.verb.as_deref() == Some("list")
            && !turn.refused
            && reaches(&turn.arguments["filters"], since, today)
    });
    (!looked).then(|| {
        format!(
            "Not proposed yet. Before recording a belief, check that it still holds now: \
             `list` the records it rests on with a date filter from {since} to {today}. If they \
             support it, propose it again. If they contradict it, say in the statement what \
             changed and since when. If they say nothing about it, propose it again unchanged."
        )
    })
}

/// True when some filter in `filters` is a date range overlapping `since..=today`.
/// A range with only one end is open on the other.
fn reaches(filters: &Value, since: NaiveDate, today: NaiveDate) -> bool {
    let Some(filters) = filters.as_object() else {
        return false;
    };
    filters.values().any(|range| {
        let from = date_at(range, "from");
        let to = date_at(range, "to");
        (from.is_some() || to.is_some())
            && from.is_none_or(|f| f <= today)
            && to.is_none_or(|t| t >= since)
    })
}

fn date_at(range: &Value, key: &str) -> Option<NaiveDate> {
    let text = range.get(key)?.as_str()?;
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::chat::Usage;
    use serde_json::json;
    use std::time::Duration as StdDuration;

    fn list(filters: Value) -> TurnRecord {
        TurnRecord {
            verb: Some("list".into()),
            arguments: json!({ "type": "journal", "filters": filters }),
            repeated: false,
            refused: false,
            usage: Usage::default(),
            latency: StdDuration::ZERO,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 4).unwrap()
    }

    /// The rejected sleep belief: twelve reads by relevance, the newest July 15.
    #[test]
    fn a_run_that_never_listed_the_last_month_must_look_first() {
        let why = belief_needs_a_recent_look(&[], today()).unwrap();
        assert!(why.contains("from 2026-09-04 to 2026-10-04"), "{why}");
    }

    #[test]
    fn a_range_ending_before_the_window_does_not_count() {
        let old = list(json!({ "date": { "from": "2026-06-01", "to": "2026-07-15" } }));
        assert!(belief_needs_a_recent_look(&[old], today()).is_some());
    }

    #[test]
    fn a_range_reaching_into_the_window_counts_even_open_ended() {
        let closed = list(json!({ "date": { "from": "2026-08-20", "to": "2026-09-10" } }));
        let open = list(json!({ "date": { "from": "2026-09-20" } }));
        assert!(belief_needs_a_recent_look(&[closed], today()).is_none());
        assert!(belief_needs_a_recent_look(&[open], today()).is_none());
    }

    /// Only a date range is a look at a period; a tag or a flag is not.
    #[test]
    fn a_filter_that_is_not_a_range_does_not_count() {
        let tagged = list(json!({ "tags": "sleep", "closed": true }));
        assert!(belief_needs_a_recent_look(&[tagged], today()).is_some());
    }
}
