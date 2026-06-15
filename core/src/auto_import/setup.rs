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
//! for per-day-tx-volume use. Callers pass the effective interval explicitly so
//! an env-override / per-deployment policy lives at the composition root, not here.

use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use crate::auto_import_scheduler::{spawn_with_registry, AutoImportSource, SourceRegistry};

/// Default poll interval when the caller doesn't override it.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Register each source in `registry`, then spawn its perpetual scheduler task.
///
/// Each source is registered *before* its task starts, so a manual-tick lookup
/// can find it by name and status reads see every configured source even before
/// its first tick completes.
///
/// Returns the `JoinHandle`s so the caller can abort them at shutdown. An empty
/// `sources` vec returns an empty vec — startup succeeds (graceful zero-config).
pub async fn spawn_sources(
    sources: Vec<Arc<dyn AutoImportSource>>,
    interval: Duration,
    registry: &SourceRegistry,
) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::with_capacity(sources.len());
    for source in sources {
        tracing::info!(
            source = source.name(),
            interval_secs = interval.as_secs(),
            "spawning auto-import"
        );
        registry.register(source.clone(), interval).await;
        handles.push(spawn_with_registry(registry.clone(), source, interval));
    }
    handles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auto_import_scheduler::null::NullSource;

    #[tokio::test]
    async fn empty_sources_spawns_nothing() {
        let registry = SourceRegistry::new();
        let handles = spawn_sources(Vec::new(), DEFAULT_INTERVAL, &registry).await;
        assert_eq!(handles.len(), 0);
        assert_eq!(registry.snapshot().await.len(), 0);
    }

    #[tokio::test]
    async fn n_sources_register_and_spawn_n_handles() {
        let registry = SourceRegistry::new();
        let sources: Vec<Arc<dyn AutoImportSource>> = vec![
            Arc::new(NullSource::new("alpha")),
            Arc::new(NullSource::new("beta")),
        ];
        let handles = spawn_sources(sources, DEFAULT_INTERVAL, &registry).await;
        assert_eq!(handles.len(), 2);
        assert_eq!(registry.snapshot().await.len(), 2);
        // Abort so the spawned tasks don't outlive the test.
        for h in handles {
            h.abort();
        }
    }
}
