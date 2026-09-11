//! Which features own which projections — the projection half of the feature map.
//!
//! A projection registers when **any** feature that owns it is enabled, because
//! ownership is many-to-many in both directions: `notes` serves three features,
//! and `finances` owns two projections. The full map, including the tab,
//! scheduler, command and settings columns that live in the client, is published
//! in `docs/src/features.md`.
//!
//! ⚠️ A projection that no feature claims and that is not in [`NEVER_GATED`] will
//! never register at all. `every_projection_has_an_owner` is what stops that
//! being a silent mistake — keep it passing rather than deleting the entry.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::config::{ALL_FEATURES, Feature, ResolvedConfig};
use crate::journal_file::JournalFile;

use super::{
    AssistantProjection, AutoImportProjection, BeliefsProjection, BudgetProjection,
    ConfigProjection, NotesProjection, Projection, RecordTypeProjection, RoutinesProjection,
};

/// Projections that are never feature-gated.
///
/// `config` materializes the `app_config` table that the gating itself reads, so
/// gating it would be circular: the first launch after a toggle would have no
/// config to decide with.
///
/// `record_types` is ungated for the neighbouring reason: it decides how a journal
/// entry is *read*, so gating it behind the journal feature would mean turning
/// journal off and back on again reinterpreted the whole back catalogue against
/// whatever fallback shape happened to apply.
pub const NEVER_GATED: &[&str] = &[ConfigProjection::NAME, RecordTypeProjection::NAME];

/// Every projection registered in production, for [`every_projection_has_an_owner`].
pub const ALL_PROJECTIONS: &[&str] = &[
    ConfigProjection::NAME,
    RecordTypeProjection::NAME,
    NotesProjection::NAME,
    RoutinesProjection::NAME,
    BudgetProjection::NAME,
    AutoImportProjection::NAME,
    JournalFile::NAME,
    AssistantProjection::NAME,
    BeliefsProjection::NAME,
];

/// The projections a feature owns.
///
/// Two entries carry a dependency that is not obvious from the names:
/// - `notes` appears under `Llm` because `note_llm_processed` is projected there,
///   into whichever of the two tables the aggregate belongs to.
/// - `budget` appears under `AutoImport` because a committed batch emits
///   `transaction_recorded`, so without it imported money has nowhere to land.
///   `journal_file` is deliberately *not* under `AutoImport`: it reads `budget`'s
///   `transactions` table, so it is only ever useful alongside the finances tab
///   that reads its output.
/// - `Llm` owns three: `notes`, because `note_llm_processed` lands in its tables;
///   `assistant`, the conversation itself; and `beliefs`, which exists only
///   because the assistant concluded something — turning the assistant off
///   therefore stops beliefs being maintained, which is the intended reading of
///   "the feature is inert".
pub fn owned_projections(feature: Feature) -> &'static [&'static str] {
    match feature {
        Feature::Journal | Feature::Notes => &[NotesProjection::NAME],
        Feature::Llm => &[
            NotesProjection::NAME,
            AssistantProjection::NAME,
            BeliefsProjection::NAME,
        ],
        Feature::Routines => &[RoutinesProjection::NAME],
        Feature::Finances => &[BudgetProjection::NAME, JournalFile::NAME],
        Feature::AutoImport => &[BudgetProjection::NAME, AutoImportProjection::NAME],
    }
}

/// The projection names that should be registered under `config`.
///
/// Read once at startup, before the projection list is built — which is why a
/// toggle applies at the next launch and [`crate::config::ConfigKey::applies_immediately`]
/// reports `false` for every feature key.
pub fn enabled_projection_names(config: &ResolvedConfig) -> BTreeSet<&'static str> {
    let mut names: BTreeSet<&'static str> = NEVER_GATED.iter().copied().collect();
    for feature in ALL_FEATURES {
        if config.enabled(*feature) {
            names.extend(owned_projections(*feature).iter().copied());
        }
    }
    names
}

/// Every production projection, in registration order.
///
/// `BudgetProjection` **must** precede `JournalFile`: the latter re-reads the
/// former's `transactions` table when re-rendering an updated transaction.
///
/// Constructed here rather than at the call site so there is exactly one list.
/// A second copy in the client's startup could disagree with [`ALL_PROJECTIONS`]
/// and the disagreement would be silent — a projection missing from the client's
/// copy simply never runs.
fn all_projections(journal_path: PathBuf) -> Vec<Box<dyn Projection>> {
    vec![
        Box::new(ConfigProjection),
        // `RecordTypeProjection` **must** precede `NotesProjection`: the latter
        // reads the declaration to decide whether an entry is complete, and
        // `ProjectionRunner` applies every projection to one event before moving
        // to the next. Ahead of it, a declaration applies to the events that
        // follow it in the log and to no earlier one — behind it, a shape change
        // would land one event late on every replay.
        Box::new(RecordTypeProjection),
        Box::new(NotesProjection),
        Box::new(RoutinesProjection),
        Box::new(BudgetProjection),
        Box::new(AutoImportProjection),
        Box::new(JournalFile::new(journal_path)),
        // Last, and order-independent: neither reads anything another projection
        // writes, and nothing reads their tables but the client and the agent.
        Box::new(AssistantProjection),
        Box::new(BeliefsProjection),
    ]
}

/// The projections to register for `config`.
///
/// The whole feature gate on the projection side: anything whose feature is off is
/// never constructed into the runner, so its tables stop being maintained and its
/// watermark row freezes. `ProjectionRunner::catch_up` filters by the registered
/// set precisely so that frozen row cannot force a replay on every later launch.
pub fn build_projections(
    config: &ResolvedConfig,
    journal_path: PathBuf,
) -> Vec<Box<dyn Projection>> {
    let enabled = enabled_projection_names(config);
    all_projections(journal_path)
        .into_iter()
        .filter(|p| enabled.contains(p.name()))
        .collect()
}

/// Projections a host with no filesystem of its own must never register.
///
/// `JournalFile` writes an hledger file through `tokio::fs`. It is owned only by
/// `Finances`, so a finances-off config already omits it — but ⚠️ that is an
/// accident of configuration, not a guarantee. A **cold-start** host has no
/// config at all (the shared layer is itself a projection, so it is empty until
/// events have been folded) and every feature falls back to its default, which
/// is `true`. The config filter would let `JournalFile` through on exactly the
/// boot where nobody asked for it.
pub const FILESYSTEM_PROJECTIONS: &[&str] = &[JournalFile::NAME];

/// The projections to register for a headless host — the agent.
///
/// Same feature gating as [`build_projections`], minus anything in
/// [`FILESYSTEM_PROJECTIONS`].
pub fn build_projections_headless(config: &ResolvedConfig) -> Vec<Box<dyn Projection>> {
    let enabled = enabled_projection_names(config);
    // The path is never used: every projection that would read it is filtered
    // out below.
    all_projections(PathBuf::new())
        .into_iter()
        .filter(|p| enabled.contains(p.name()) && !FILESYSTEM_PROJECTIONS.contains(&p.name()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent must never register `JournalFile`, **whatever** the config says.
    ///
    /// The point of the test is the finances-on case: that is the one the
    /// ordinary feature filter would let through, and it is reachable on any
    /// cold start, because an empty config resolves every feature to its default
    /// of `true`.
    #[test]
    fn a_headless_host_never_registers_a_filesystem_projection() {
        let every_config = [
            ResolvedConfig::new(ConfigMap::new(), ConfigMap::new()),
            without(&[Feature::Finances]),
            without(ALL_FEATURES),
        ];
        for config in every_config {
            let registered = build_projections_headless(&config);
            let names: Vec<&str> = registered.iter().map(|p| p.name()).collect();
            for excluded in FILESYSTEM_PROJECTIONS {
                assert!(
                    !names.contains(excluded),
                    "{excluded} reached a headless host: {names:?}"
                );
            }
        }
    }

    /// Everything else a feature owns still registers — the exclusion must be
    /// surgical, not a blanket "headless means nothing runs".
    #[test]
    fn a_headless_host_still_registers_the_rest() {
        let config = ResolvedConfig::new(ConfigMap::new(), ConfigMap::new());
        let registered = build_projections_headless(&config);
        let names: Vec<&str> = registered.iter().map(|p| p.name()).collect();
        assert!(names.contains(&NotesProjection::NAME), "{names:?}");
        assert!(names.contains(&ConfigProjection::NAME), "{names:?}");
        assert_eq!(
            names.len(),
            ALL_PROJECTIONS.len() - FILESYSTEM_PROJECTIONS.len(),
            "an all-features-on headless host should get everything but the \
             filesystem projections: {names:?}"
        );
    }

    use crate::config::{ConfigMap, ConfigValue};
    use crate::events::Projection;

    fn without(features: &[Feature]) -> ResolvedConfig {
        let mut global = ConfigMap::new();
        for f in features {
            global.insert(f.key(), ConfigValue::Bool(false));
        }
        ResolvedConfig::new(global, ConfigMap::new())
    }

    /// `ALL_PROJECTIONS` is hand-written, so it can drift from what is actually
    /// registered. This binds it to the real `name()` of every production
    /// projection — the same identity `projection_versions` keys on.
    #[test]
    fn all_projections_matches_the_real_names() {
        // Constructing a `JournalFile` touches no disk — the path is only opened
        // on the first append.
        let journal_file = JournalFile::new("/nonexistent/never-touched.journal");
        let live: Vec<&str> = vec![
            ConfigProjection.name(),
            RecordTypeProjection.name(),
            NotesProjection.name(),
            RoutinesProjection.name(),
            BudgetProjection.name(),
            AutoImportProjection.name(),
            journal_file.name(),
            AssistantProjection.name(),
            BeliefsProjection.name(),
        ];
        let listed: BTreeSet<&str> = ALL_PROJECTIONS.iter().copied().collect();
        let actual: BTreeSet<&str> = live.iter().copied().collect();
        assert_eq!(listed, actual, "ALL_PROJECTIONS has drifted from name()");
    }

    /// A projection nobody claims never registers, and nothing else would say so.
    #[test]
    fn every_projection_has_an_owner() {
        let mut claimed: BTreeSet<&str> = NEVER_GATED.iter().copied().collect();
        for feature in ALL_FEATURES {
            claimed.extend(owned_projections(*feature).iter().copied());
        }
        for name in ALL_PROJECTIONS {
            assert!(
                claimed.contains(name),
                "projection {name} is claimed by no feature and is not in NEVER_GATED, \
                 so it would never register"
            );
        }
    }

    /// The default — nothing set anywhere — must register exactly what the build
    /// before this machinery registered.
    #[test]
    fn everything_registers_by_default() {
        let names = enabled_projection_names(&ResolvedConfig::default());
        let all: BTreeSet<&str> = ALL_PROJECTIONS.iter().copied().collect();
        assert_eq!(names, all);
    }

    #[test]
    fn the_never_gated_projections_survive_every_feature_being_off() {
        let names = enabled_projection_names(&without(ALL_FEATURES));
        assert_eq!(
            names,
            BTreeSet::from([ConfigProjection::NAME, RecordTypeProjection::NAME]),
            "config must register even with every feature off — it is what the \
             gating reads — and record_types with it, or turning journal off and \
             on again would reinterpret the back catalogue"
        );
    }

    /// `notes` serves journal, notes and llm, so it survives any one of them
    /// being off and only drops when all three are.
    #[test]
    fn notes_needs_all_three_of_its_owners_off() {
        for one in [Feature::Journal, Feature::Notes, Feature::Llm] {
            assert!(
                enabled_projection_names(&without(&[one])).contains(NotesProjection::NAME),
                "notes dropped when only {one} was off"
            );
        }
        assert!(
            !enabled_projection_names(&without(&[Feature::Journal, Feature::Notes, Feature::Llm]))
                .contains(NotesProjection::NAME)
        );
    }

    /// Auto-import's output lands in `budget`, so it holds that projection up on
    /// its own — but not `journal_file`, which only serves the finances tab.
    #[test]
    fn auto_import_holds_budget_up_without_finances() {
        let names = enabled_projection_names(&without(&[Feature::Finances]));
        assert!(names.contains(BudgetProjection::NAME));
        assert!(!names.contains(JournalFile::NAME));
    }

    #[test]
    fn finances_off_and_auto_import_off_drops_both() {
        let names = enabled_projection_names(&without(&[Feature::Finances, Feature::AutoImport]));
        assert!(!names.contains(BudgetProjection::NAME));
        assert!(!names.contains(JournalFile::NAME));
        assert!(!names.contains(AutoImportProjection::NAME));
    }

    /// A device override of `false` must gate just as a global `false` does —
    /// the whole point of the device layer is turning a feature off on one phone.
    #[test]
    fn a_device_override_gates_too() {
        let mut device = ConfigMap::new();
        device.insert(Feature::Routines.key(), ConfigValue::Bool(false));
        let config = ResolvedConfig::new(ConfigMap::new(), device);
        assert!(!enabled_projection_names(&config).contains(RoutinesProjection::NAME));
    }

    fn names_of(projections: &[Box<dyn Projection>]) -> Vec<&str> {
        projections.iter().map(|p| p.name()).collect()
    }

    /// `build_projections` is the client's whole registration step, so the order
    /// it returns is the order the runner folds in. `JournalFile` re-reads
    /// `BudgetProjection`'s `transactions` table, so budget must come first.
    #[test]
    fn budget_is_registered_before_the_journal_file_reads_it() {
        let all = build_projections(&ResolvedConfig::default(), PathBuf::from("/tmp/x.journal"));
        let names = names_of(&all);
        let budget = names.iter().position(|n| *n == BudgetProjection::NAME);
        let file = names.iter().position(|n| *n == JournalFile::NAME);
        assert!(
            budget < file,
            "journal_file must fold after budget, got {names:?}"
        );
    }

    /// The **whole seam**, end to end: a real `ConfigSet` event lands in the store,
    /// the projection folds it into `app_config`, the next launch reads that table
    /// back with `load_persisted`, and the registration list narrows accordingly.
    ///
    /// The pure-function tests above never touch a database, and the client's
    /// startup path was only ever exercised by hand — so an event-payload or
    /// column-name change between the writer and `load_persisted` would have gone
    /// unnoticed by every other test here. This is the one that would catch it.
    #[tokio::test]
    async fn a_stored_config_event_narrows_the_next_launch_s_registration() {
        use crate::events::{EventStore, NewEvent, ProjectionRunner, SurrealEventStore};

        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::connect(dir.path().join("t.db").to_str().unwrap())
            .await
            .unwrap();
        let journal = dir.path().join("budget.journal");

        // Launch 1: nothing configured, so everything registers.
        let config = ResolvedConfig::new(
            crate::events::load_persisted(&db).await.unwrap(),
            ConfigMap::new(),
        );
        let first = build_projections(&config, journal.clone());
        assert_eq!(
            names_of(&first).len(),
            ALL_PROJECTIONS.len(),
            "a fresh install must register everything"
        );

        // The user switches finances off. This is the real write path: an event
        // through the store, folded by the real projection.
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(ConfigProjection)]);
        runner.init_all().await.unwrap();
        // The canonical factory, not a hand-built envelope — otherwise this test
        // could keep passing against a payload shape the real writer no longer
        // produces, which is most of what it exists to catch.
        let stored = SurrealEventStore::new(db.clone())
            .append(
                NewEvent::config_set(
                    "d1",
                    Feature::Finances.key(),
                    Some(ConfigValue::Bool(false)),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        runner.apply_events(&[stored]).await.unwrap();

        // Launch 2: startup reads the materialized table, not the log.
        let config = ResolvedConfig::new(
            crate::events::load_persisted(&db).await.unwrap(),
            ConfigMap::new(),
        );
        assert!(
            !config.enabled(Feature::Finances),
            "load_persisted did not read back the value ConfigProjection wrote"
        );

        let registered = build_projections(&config, journal.clone());
        let names = names_of(&registered);
        assert!(!names.contains(&JournalFile::NAME), "{names:?}");
        assert!(
            names.contains(&BudgetProjection::NAME),
            "auto-import is still on, so budget must survive: {names:?}"
        );
        assert!(
            names.contains(&ConfigProjection::NAME),
            "config must always register, or the switch becomes unreachable: {names:?}"
        );

        // And the device layer still wins over what the log says.
        let mut device = ConfigMap::new();
        device.insert(Feature::Finances.key(), ConfigValue::Bool(true));
        let overridden =
            ResolvedConfig::new(crate::events::load_persisted(&db).await.unwrap(), device);
        let re_registered = build_projections(&overridden, journal);
        assert!(
            names_of(&re_registered).contains(&JournalFile::NAME),
            "a device override of `true` must re-register the feature"
        );
    }
}
