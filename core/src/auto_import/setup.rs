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
/// An empty `sources` vec is a no-op — startup still succeeds (graceful
/// zero-config).
pub async fn spawn_sources(
    sources: Vec<Arc<dyn AutoImportSource>>,
    interval: Duration,
    registry: &SourceRegistry,
) {
    for source in sources {
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
        spawn_sources(Vec::new(), DEFAULT_INTERVAL, &registry).await;
        assert_eq!(registry.snapshot().await.len(), 0);
    }

    #[tokio::test]
    async fn n_sources_register_and_spawn() {
        let registry = SourceRegistry::new();
        let sources: Vec<Arc<dyn AutoImportSource>> = vec![
            Arc::new(NullSource::new("alpha")),
            Arc::new(NullSource::new("beta")),
        ];
        spawn_sources(sources, DEFAULT_INTERVAL, &registry).await;
        assert_eq!(registry.snapshot().await.len(), 2);
        // Tear down so the spawned tasks don't outlive the test (the registry
        // owns the handles now, so removal aborts them).
        assert!(registry.remove("alpha").await);
        assert!(registry.remove("beta").await);
        assert_eq!(registry.snapshot().await.len(), 0);
    }
}
