//! One day's completion status for one routine group — the `2/3` in the card
//! header and the greying-out of a finished routine.
//!
//! Lives outside `pages/routines.rs` because the rule it encodes is not the
//! screen's own. `core::routines::roll_up` encodes the same rule for the
//! assistant, and the two crates share no Rust code — the frontend is its own
//! cargo workspace and everything crossing is JSON over Tauri IPC. Wiring the
//! screen to `roll_up` was considered and rejected (user, 2026-09-08): an async
//! round-trip is not worth it for a number already on screen. So the guarantee
//! is carried by a test instead — `fixtures/routine_day_agreement.json` at the
//! repo root feeds the same cases to this function and to `roll_up`, and both
//! sides must land on the same answer. Pulling this derivation back inline
//! would silently remove that alarm.

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
/// ⚠️ **A skipped item counts as done** (user, 2026-09-08). A skip is a
/// deliberate act with a reason attached, not a failure to act, so any row —
/// skipped or not — is an item that happened.
///
/// `items` are the routine's *current* items; the caller filters `removed`
/// ones out, exactly as `roll_up`'s caller does. `date` is taken as a
/// parameter even though the loader only ever holds one date's rows: it makes
/// this function's input contract identical to `roll_up`'s, so the shared
/// fixture can feed both sides the same case without massaging it per side.
///
/// The agreement this buys has limits, and they are worth naming:
/// - the fixture pins agreement **on the cases it covers**, not on all inputs;
/// - nothing forces the screen to keep *calling* this function — inline a fresh
///   derivation in the component and no test fires;
/// - `roll_up` also reports how many of `done` were skips, which this does not,
///   so that field is asserted on the core side alone.
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
    /// The other implementation is `core::routines::roll_up`, in a crate this
    /// one cannot import. Change the rule on either side and nothing fails to
    /// build; the divergence surfaces as the assistant answering "did I do my
    /// routine" differently from what the user is looking at. This test and its
    /// twin in `core/src/routines.rs` read one fixture and must agree.
    ///
    /// If it fires, decide which side is right FIRST. The fixture is a product
    /// decision written down, not a pin to be nudged until the build is green.
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
