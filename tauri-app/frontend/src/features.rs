//! The feature set this launch is running with, as app-wide context.
//!
//! Read **once**, at boot, and never refreshed — deliberately. A feature's
//! projections and schedulers were decided when the backend started, so a live
//! value would produce a tab whose data nothing is maintaining. Core says the
//! same thing from the other side: `ConfigKey::applies_immediately` is `false`
//! for every feature key, and the settings row already tells the user the change
//! lands at the next launch.

use dioxus::prelude::*;

use crate::types::{Feature, Features};

/// The boot feature set. `None` until the config read resolves.
///
/// The unresolved state is distinct from "everything on" so the nav can wait
/// rather than draw a tab and then remove it — see [`use_features_provider`].
#[derive(Clone, Copy)]
pub struct FeatureSet(pub Signal<Option<Features>>);

/// Provide the feature set to the tree, returning the signal for the root's
/// config future to fill.
///
/// The fetch itself is *not* here: the root already reads the same
/// `Vec<ConfigEntry>` for theme and accent, and a second `get_config` at boot
/// would put a redundant round trip on the pre-paint path. Whoever holds this
/// signal must set it exactly once — including on failure, or `features_ready`
/// never becomes true and the splash never lifts.
pub fn use_features_provider() -> Signal<Option<Features>> {
    let set = use_signal(|| None::<Features>);
    use_context_provider(|| FeatureSet(set));
    set
}

/// The feature set, for components deciding what to draw.
///
/// Reports everything on while the read is outstanding, so a component that
/// renders before it resolves shows too much rather than too little. The nav
/// avoids that window entirely by holding boot until [`features_ready`].
pub fn use_features() -> Features {
    match try_use_context::<FeatureSet>() {
        Some(FeatureSet(set)) => set.read().clone().unwrap_or_default(),
        // No provider: a component rendered outside the app root, which in
        // practice means a test or a preview harness.
        None => Features::default(),
    }
}

/// Whether the feature read has landed. The boot splash waits on this.
pub fn features_ready() -> bool {
    match try_use_context::<FeatureSet>() {
        Some(FeatureSet(set)) => set.read().is_some(),
        None => true,
    }
}

/// Gate a subtree on a feature — `rsx!` nothing when it is off.
///
/// For the sub-surfaces inside a page that a *different* feature owns, where
/// hiding the whole tab would be wrong: the capture and batch-review flows live
/// inside Finances but belong to LLM and auto-import.
pub fn feature_on(feature: Feature) -> bool {
    use_features().on(feature)
}
