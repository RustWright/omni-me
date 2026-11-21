//! One day's completion status for one routine group — the `2/3` in the card
//! header and the greying-out of a finished routine.
//!
//! Twin of `core::routines::roll_up`, which this crate cannot import; calling it over IPC was
//! rejected by ruling 87 (RULINGS.md). `fixtures/routine_day_agreement.json` feeds both, so
//! inlining this derivation into the page would silently remove the alarm.

use crate::types::{CompletionEntry, RoutineItem};

/// What the card header shows for one day: how many of the routine's items are
/// done, out of how many, and whether that makes the day complete.
///
/// No `skipped` count, deliberately — the screen renders skips per item and
/// never in aggregate, so computing one here would be a number nothing reads.
/// `roll_up` does report it; see [`day_progress`] on what that costs the test.
pub struct DayProgress {
    pub done: usize,
    pub total: usize,
    pub complete: bool,
}

/// Roll one day's completion rows up into the group's header status.
///
/// A skipped item counts as done: ruling 86 (RULINGS.md). `items` excludes removed ones, and
/// `date` mirrors `roll_up`'s contract so one fixture feeds both.
/// Limits: the fixture covers only its cases, nothing forces the screen to call this, and the
/// skip count is asserted on the core side alone.
pub fn day_progress(
    items: &[RoutineItem],
    completions: &[CompletionEntry],
    date: &str,
) -> DayProgress {
    let total = items.len();
    // Iterate items rather than rows: two rows for one item on one day (an
    // undo followed by a skip) is one item done, not two.
    let done = items
        .iter()
        .filter(|item| {
            completions
                .iter()
                .any(|c| c.item_id == item.id && c.date == date)
        })
        .count();
    DayProgress {
        done,
        total,
        // A routine with no items would otherwise report a perfect streak
        // forever — every item of nothing being done.
        complete: total > 0 && done == total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> RoutineItem {
        RoutineItem {
            id: id.to_string(),
            group_id: "group".to_string(),
            name: id.to_string(),
            estimated_duration_min: 0,
            order_num: 0,
            removed: false,
        }
    }

    fn entry(item_id: &str, date: &str, skipped: bool) -> CompletionEntry {
        CompletionEntry {
            id: format!("{item_id}:{date}"),
            item_id: item_id.to_string(),
            group_id: "group".to_string(),
            date: date.to_string(),
            skipped,
            reason: None,
        }
    }

    #[test]
    fn a_skipped_item_still_counts_the_day_complete() {
        let items = [item("stretch"), item("coffee")];
        let rows = [
            entry("stretch", "2026-03-14", false),
            entry("coffee", "2026-03-14", true),
        ];
        let p = day_progress(&items, &rows, "2026-03-14");
        assert_eq!(p.done, 2);
        assert!(p.complete, "a skip is not a miss");
    }

    #[test]
    fn an_empty_routine_is_never_complete() {
        let p = day_progress(&[], &[], "2026-03-14");
        assert_eq!(p.total, 0);
        assert!(!p.complete);
    }

    /// One completion case as the shared fixture states it. Both crates
    /// deserialize this shape independently — there is no shared definition to
    /// derive it from, which is the whole situation being defended against.
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Case {
        name: String,
        why: String,
        item_ids: Vec<String>,
        date: String,
        completions: Vec<Row>,
        expect: Expect,
    }

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Row {
        item_id: String,
        date: String,
        skipped: bool,
    }

    /// Note the missing `skipped`, and the missing `deny_unknown_fields` that
    /// lets it stay missing: the screen computes no aggregate skip count, so
    /// that expectation is core's to check. An unknown field in `Case` above,
    /// by contrast, is a new *input* one side never learned to read — that one
    /// must fail loudly.
    #[derive(serde::Deserialize)]
    struct Expect {
        done: usize,
        total: usize,
        complete: bool,
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    /// Cross-boundary agreement alarm for the day-completion rule.
    ///
    /// This and its twin in `core/src/routines.rs` read one fixture and must agree. If it fires,
    /// decide which side is right first: the fixture is a product decision, not a pin to nudge.
    #[test]
    fn day_progress_agrees_with_the_shared_day_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/routine_day_agreement.json");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let fixture: Fixture = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));
        assert!(
            !fixture.cases.is_empty(),
            "fixture has no cases — this test would pass having checked nothing"
        );

        for case in &fixture.cases {
            let ctx = format!("{} ({})", case.name, case.why);
            let items: Vec<RoutineItem> = case.item_ids.iter().map(|id| item(id)).collect();
            let rows: Vec<CompletionEntry> = case
                .completions
                .iter()
                .map(|r| entry(&r.item_id, &r.date, r.skipped))
                .collect();

            let got = day_progress(&items, &rows, &case.date);
            assert_eq!(got.done, case.expect.done, "done — {ctx}");
            assert_eq!(got.total, case.expect.total, "total — {ctx}");
            assert_eq!(got.complete, case.expect.complete, "complete — {ctx}");
        }
    }
}
