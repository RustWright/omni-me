//! Pure reorder logic for drag-and-drop on routine group cards.
//!
//! Keeps the DOM handlers in `pages/routines.rs` thin: they just identify
//! `(dragged_id, target_id)` and call into this module to compute the new
//! order. The transform is pure, trivially testable, and shared between the
//! UI update and the payload sent to `reorder_routine_groups`.

use crate::types::RoutineGroup;

/// Compute the new group ordering after `dragged_id` is dropped onto
/// `target_id`. Returns a fresh Vec with the moved element relocated.
///
/// To minimize no-ops the function makes drag-up drops before target;
/// and drag-down drops after target. Self-drops / missing ids are a no-op.
/// The function must preserve every group exactly once.
pub fn reorder_groups_after_drop(
    mut groups: Vec<RoutineGroup>,
    dragged_id: &str,
    target_id: &str,
) -> Vec<RoutineGroup> {
    let dragged_idx = groups.iter().position(|g| g.id == dragged_id);
    let target_idx = groups.iter().position(|g| g.id == target_id);
    match (dragged_idx, target_idx) {
        (None, _) | (_, None) => groups,
        (Some(dragged_idx), Some(target_idx)) if dragged_idx == target_idx => groups,
        (Some(dragged_idx), Some(target_idx)) => {
            let dragged = groups.remove(dragged_idx);
            groups.insert(target_idx, dragged);
            groups
        }
    }
}

/// Serialize a group list as the `orderings` payload for the
/// `reorder_routine_groups` Tauri command. Each entry's `order` becomes its
/// new index in the list (0-based).
pub fn to_orderings_payload(groups: &[RoutineGroup]) -> serde_json::Value {
    let entries: Vec<serde_json::Value> = groups
        .iter()
        .enumerate()
        .map(|(idx, g)| {
            serde_json::json!({
                "group_id": g.id,
                "order": idx as u32,
            })
        })
        .collect();
    serde_json::Value::Array(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(id: &str) -> RoutineGroup {
        RoutineGroup {
            id: id.to_string(),
            name: id.to_string(),
            frequency: "daily".to_string(),
            order_num: 0,
            removed: false,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn ids(gs: &[RoutineGroup]) -> Vec<&str> {
        gs.iter().map(|g| g.id.as_str()).collect()
    }

    #[test]
    fn missing_dragged_returns_unchanged() {
        let groups = vec![g("a"), g("b"), g("c")];
        let out = reorder_groups_after_drop(groups.clone(), "ghost", "b");
        assert_eq!(ids(&out), vec!["a", "b", "c"]);
    }

    #[test]
    fn missing_target_returns_unchanged() {
        let groups = vec![g("a"), g("b"), g("c")];
        let out = reorder_groups_after_drop(groups.clone(), "a", "ghost");
        assert_eq!(ids(&out), vec!["a", "b", "c"]);
    }

    #[test]
    fn self_drop_returns_unchanged() {
        let groups = vec![g("a"), g("b"), g("c")];
        let out = reorder_groups_after_drop(groups.clone(), "b", "b");
        assert_eq!(ids(&out), vec!["a", "b", "c"]);
    }

    #[test]
    fn preserves_every_group_exactly_once() {
        let groups = vec![g("a"), g("b"), g("c"), g("d")];
        let out = reorder_groups_after_drop(groups.clone(), "a", "c");
        let mut sorted_ids: Vec<&str> = ids(&out);
        sorted_ids.sort();
        assert_eq!(sorted_ids, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn items_dragged_up_dropped_before() {
        let groups = vec![g("a"), g("b"), g("c"), g("d")];
        let out = reorder_groups_after_drop(groups.clone(), "c", "b");
        let result: Vec<&str> = ids(&out);
        assert_eq!(result, vec!["a", "c", "b", "d"]);
    }

    #[test]
    fn items_dragged_down_dropped_after() {
        let groups = vec![g("a"), g("b"), g("c"), g("d")];
        let out = reorder_groups_after_drop(groups.clone(), "a", "c");
        let result: Vec<&str> = ids(&out);
        assert_eq!(result, vec!["b", "c", "a", "d"]);
    }

    #[test]
    fn items_dragged_down_to_bottom_of_list_successfully() {
        let groups = vec![g("a"), g("b"), g("c"), g("d")];
        let out = reorder_groups_after_drop(groups.clone(), "a", "d");
        let result: Vec<&str> = ids(&out);
        assert_eq!(result, vec!["b", "c", "d", "a"]);
    }

    #[test]
    fn items_dragged_up_to_top_of_list_successfully() {
        let groups = vec![g("a"), g("b"), g("c"), g("d"), g("e")];
        let out = reorder_groups_after_drop(groups.clone(), "e", "a");
        let result: Vec<&str> = ids(&out);
        assert_eq!(result, vec!["e", "a", "b", "c", "d"]);
    }

    #[test]
    fn payload_indices_match_list_position() {
        let groups = vec![g("x"), g("y"), g("z")];
        let payload = to_orderings_payload(&groups);
        assert_eq!(
            payload,
            serde_json::json!([
                { "group_id": "x", "order": 0 },
                { "group_id": "y", "order": 1 },
                { "group_id": "z", "order": 2 },
            ])
        );
    }
}
