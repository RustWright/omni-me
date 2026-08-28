//! Generic auto-import source spawner.
//!
//! After the open-core split this module knows nothing about any specific
//! upstream. The composition root (the private overlay binary, or any other
//! caller) builds the `Vec<Arc<dyn AutoImportSource>>` and hands it here to be
//! registered + spawned. Keeping this fully generic is what lets the public
//! engine ship with zero bank-specific code — a caller that supplies no sources
//! gets a working server with auto-import simply idle.
//!
//! Interval defaults to 30 minutes (`DEFAULT_INTERVAL`) — a reasonable balance
//! for per-day-tx-volume use. Callers pass the effective *global* interval; a
//! source can override it for itself via `AutoImportSource::poll_interval`
//! (config-declared sources carry their own `schedule_secs`).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use crate::auto_import_scheduler::{AutoImportSource, SourceRegistry};

/// Default poll interval when the caller doesn't override it.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Register + spawn each source on the `registry`, which then *owns* each
/// scheduler task's handle — that ownership is what lets a source be torn down
/// live (the in-app remove / edit flow) via [`SourceRegistry::remove`]. Each
/// source's effective interval is its own `poll_interval()` if it declares one,
/// else `interval`.
///
/// `paused` is the persisted off-switch set (#367): a source whose name is in it
/// is **registered but not spawned** — it appears in the status snapshot flagged
/// `paused`, ready to be resumed, but never spawns a scheduler task, so it does
/// not even tick once at boot. That "not even once" matters for a bank source:
/// re-arming a source the user switched off — if only for a single login at
/// startup — is exactly the runaway hammering the pause exists to prevent. Pass
/// an empty set to spawn everything.
///
/// An empty `sources` vec is a no-op — startup still succeeds (graceful
/// zero-config).
pub async fn spawn_sources(
    sources: Vec<Arc<dyn AutoImportSource>>,
    interval: Duration,
    registry: &SourceRegistry,
    paused: &BTreeSet<String>,
) {
    for source in sources {
        if paused.contains(source.name()) {
            tracing::info!(
                source = source.name(),
                "auto-import source is paused — registering without spawning"
            );
            // Register status-only (task = None), then flag it paused so the
            // snapshot shows the off state and `resume` can later re-arm it. The
            // interval still reflects the source's own override for the UI.
            let iv = source.poll_interval().unwrap_or(interval);
            let name = source.name().to_string();
            registry.register(source, iv).await;
            registry.pause(&name).await;
            continue;
        }
        tracing::info!(source = source.name(), "spawning auto-import");
        registry.spawn_one(source, interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auto_import_scheduler::null::NullSource;

    #[tokio::test]
    async fn empty_sources_spawns_nothing() {
        let registry = SourceRegistry::new();
        spawn_sources(Vec::new(), DEFAULT_INTERVAL, &registry, &BTreeSet::new()).await;
        assert_eq!(registry.snapshot().await.len(), 0);
    }

    #[tokio::test]
    async fn n_sources_register_and_spawn() {
        let registry = SourceRegistry::new();
        let sources: Vec<Arc<dyn AutoImportSource>> = vec![
            Arc::new(NullSource::new("alpha")),
            Arc::new(NullSource::new("beta")),
        ];
        spawn_sources(sources, DEFAULT_INTERVAL, &registry, &BTreeSet::new()).await;
        assert_eq!(registry.snapshot().await.len(), 2);
        // Tear down so the spawned tasks don't outlive the test (the registry
        // owns the handles now, so removal aborts them).
        assert!(registry.remove("alpha").await);
        assert!(registry.remove("beta").await);
        assert_eq!(registry.snapshot().await.len(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn persisted_paused_source_registers_but_never_ticks() {
        // A source named in the persisted paused set is registered (visible +
        // resumable) but its scheduler never runs — not even one boot tick.
        let registry = SourceRegistry::new();
        let paused_src = Arc::new(NullSource::new("bank"));
        let live_src = Arc::new(NullSource::new("other"));
        let mut paused = BTreeSet::new();
        paused.insert("bank".to_string());

        spawn_sources(
            vec![paused_src.clone(), live_src.clone()],
            Duration::from_secs(60),
            &registry,
            &paused,
        )
        .await;

        // Give virtual time for many would-be ticks.
        tokio::time::sleep(Duration::from_secs(600)).await;

        let snap = registry.snapshot().await;
        assert_eq!(snap.len(), 2, "both sources are registered");
        let bank = snap.iter().find(|s| s.name == "bank").unwrap();
        assert!(bank.paused, "the persisted-paused source is flagged paused");
        assert_eq!(
            paused_src.call_count(),
            0,
            "a paused source must not tick even once at boot"
        );
        assert!(
            live_src.call_count() >= 1,
            "the un-paused source still runs normally"
        );
        registry.remove("bank").await;
        registry.remove("other").await;
    }
}
