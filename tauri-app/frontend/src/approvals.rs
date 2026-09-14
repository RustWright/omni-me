//! What is waiting on the user, as app-wide context.
//!
//! Two renderers share it: the **nav badges**, so a queue is visible from
//! wherever the user is, and the **assistant's reminder row**, for a stretch
//! spent entirely in that window. ⛔ One fetch, one set of numbers — a badge and
//! a sentence disagreeing about how much is waiting is worse than either alone.
//!
//! ⚠️ **Refreshed, unlike [`crate::features`].** The feature set is a boot
//! snapshot on purpose; a count is the opposite — it is stale the moment a
//! proposal lands or the user decides one, so this subscribes to
//! [`crate::sync_refresh`], which fires for an inbound pull *and* for a local
//! write that changed data another view is showing.

use dioxus::prelude::*;

use crate::Tab;
use crate::bridge;
use crate::components::nav::ALL_TABS;
use crate::sync_refresh::use_sync_epoch;
use crate::types::{Feature, PendingApprovals};

/// The wire name core uses for its own inbox, which is not a feature key.
const ASSISTANT_INBOX: &str = "assistant_inbox";

/// Pending approvals by surface. Empty until the first read lands, so a badge
/// appears when there is news rather than flickering off a default.
#[derive(Clone, Copy)]
pub struct PendingApprovalSet(pub Signal<Vec<PendingApprovals>>);

/// Provide the set at the app root and keep it current.
pub fn use_pending_approvals_provider() -> Signal<Vec<PendingApprovals>> {
    let mut set = use_signal(Vec::<PendingApprovals>::new);
    use_context_provider(|| PendingApprovalSet(set));

    let epoch = use_sync_epoch();
    use_effect(move || {
        let _ = epoch.read();
        spawn(async move {
            match bridge::invoke_pending_approvals().await {
                Ok(fresh) => set.set(fresh),
                // ⚠️ Reported, not swallowed. A failed read leaves the previous
                // counts on screen, which is the right fallback — but a badge
                // that silently stops updating is indistinguishable from a queue
                // that stopped growing, and that is the failure worth naming.
                Err(e) => web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(&format!(
                    "pending_approvals read failed, badges may be stale: {e}"
                ))),
            }
        });
    });

    set
}

/// The current set, for components deciding what to draw. Empty when no provider
/// is mounted — a component rendered in isolation draws no badges.
pub fn use_pending_approvals() -> Vec<PendingApprovals> {
    match try_use_context::<PendingApprovalSet>() {
        Some(PendingApprovalSet(set)) => set.read().clone(),
        None => Vec::new(),
    }
}

/// The tab a surface is reviewed on, or `None` when this build has no such tab.
///
/// ⛔ **`None` is a real answer, not an error.** A newer build can name a surface
/// this one does not have; the honest response is to draw no badge, because a
/// badge with nowhere to lead is worse than a missing one.
pub fn tab_of(reviewed_at: &str) -> Option<Tab> {
    let feature = match reviewed_at {
        ASSISTANT_INBOX => Feature::Llm,
        key => Feature::from_key(key)?,
    };
    ALL_TABS
        .iter()
        .copied()
        .find(|tab| tab.feature() == Some(feature))
}

/// How much is waiting on one tab. Zero means no badge.
pub fn count_for(approvals: &[PendingApprovals], tab: Tab) -> usize {
    approvals
        .iter()
        .filter(|entry| tab_of(&entry.reviewed_at) == Some(tab))
        .map(|entry| entry.count)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_key_routes_to_the_tab_that_owns_it() {
        assert_eq!(tab_of("feature.documents"), Some(Tab::Archive));
        assert_eq!(tab_of("feature.finances"), Some(Tab::Finances));
    }

    /// The assistant inbox is the one surface whose name is not a feature key.
    #[test]
    fn the_assistant_inbox_routes_to_the_assistant_tab() {
        assert_eq!(tab_of(ASSISTANT_INBOX), Some(Tab::Assistant));
    }

    /// ⛔ A surface this build does not know must draw nothing rather than
    /// panic or land on an arbitrary tab.
    #[test]
    fn an_unknown_surface_routes_nowhere() {
        assert_eq!(tab_of("feature.time_travel"), None);
        assert_eq!(tab_of(""), None);
    }

    /// Two surfaces can share a tab; the badge is their total, not the first one
    /// found. Core merges by surface, and Finances is the case where two queues
    /// already land on one screen.
    #[test]
    fn counts_for_one_tab_are_summed() {
        let approvals = vec![
            PendingApprovals {
                reviewed_at: "feature.finances".into(),
                count: 3,
            },
            PendingApprovals {
                reviewed_at: "feature.documents".into(),
                count: 2,
            },
            PendingApprovals {
                reviewed_at: "feature.finances".into(),
                count: 4,
            },
        ];

        assert_eq!(count_for(&approvals, Tab::Finances), 7);
        assert_eq!(count_for(&approvals, Tab::Archive), 2);
        assert_eq!(count_for(&approvals, Tab::Journal), 0);
    }

    /// An unknown surface contributes to no tab's total either — otherwise the
    /// count on some tab would quietly include something it cannot show.
    #[test]
    fn an_unknown_surface_adds_to_no_tab() {
        let approvals = vec![PendingApprovals {
            reviewed_at: "feature.time_travel".into(),
            count: 9,
        }];

        for tab in ALL_TABS {
            assert_eq!(count_for(&approvals, *tab), 0, "{tab:?}");
        }
    }
}
