//! Keeps the vector index current after the startup sweep: re-sweeps after a
//! pull lands changes, at most once per [`MIN_GAP`]. Why the gap, and what the
//! index missed without this: `docs/src/retrieval.md` § Keeping the index current.

use std::time::{Duration, Instant};

use omni_me_core::assistant::Embedder;
use omni_me_core::assistant::vector_store;
use omni_me_core::config::ResolvedConfig;
use omni_me_core::db::Database;
use omni_me_core::sync::{PullEvent, PullScheduler};
use tokio::sync::broadcast::error::RecvError;

/// The shortest time between two refreshes. The sweep is incremental, but each
/// one still reads every record to compare hashes.
pub const MIN_GAP: Duration = Duration::from_secs(15 * 60);

/// Refresh the index after pulls, forever. Never returns: when the pull
/// scheduler stops, the index stops refreshing and the agent keeps answering.
pub async fn run(
    db: &Database,
    config: &ResolvedConfig,
    embedder: &Embedder,
    pulls: &PullScheduler,
) {
    let mut outcomes = pulls.subscribe();
    let mut dirty = false;
    // The startup sweep has just run.
    let mut last = Instant::now();
    loop {
        let wait = MIN_GAP.saturating_sub(last.elapsed());
        tokio::select! {
            received = outcomes.recv() => match received {
                Ok(PullEvent::Applied { .. }) | Err(RecvError::Lagged(_)) => dirty = true,
                Ok(_) => {}
                Err(RecvError::Closed) => {
                    tracing::warn!("pull scheduler stopped; the vector index will not refresh");
                    std::future::pending::<()>().await;
                }
            },
            _ = tokio::time::sleep(wait), if dirty => {
                let result = vector_store::sweep(db, config, embedder).await;
                match &result {
                    Ok(report) => tracing::info!(?report, "vector index refreshed"),
                    Err(e) => tracing::warn!(error = %e, "vector index refresh failed; retrying later"),
                }
                dirty = result.is_err();
                last = Instant::now();
            }
        }
    }
}
