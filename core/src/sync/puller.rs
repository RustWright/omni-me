//! Periodic + on-demand pull scheduler.
//!
//! The server has no push channel to clients, so a receiving device must poll to
//! see edits made elsewhere. Nothing did this before — inbound events only landed
//! when the user pressed **Sync** — the "auto-sync never fires" bug from the
//! receiver's side. This task pulls once shortly after boot (the fresh-device
//! backfill), then on a fixed interval, and immediately when nudged (e.g. the
//! network just came back online), applying pulled events **best-effort** through
//! the projection runner so one bad remote event can't strand the batch (see
//! `apply_events_resilient`).
//!
//! Outcomes are broadcast on a channel so an upstream surface (the Tauri layer)
//! can tell the UI to refetch after new events land, and tests can observe it.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Notify, broadcast};
use tokio::task::JoinHandle;

use crate::db::Database;
use crate::events::ProjectionRunner;

use super::client::SyncClient;

/// Default quiet interval between background pulls.
pub const DEFAULT_PULL_INTERVAL: Duration = Duration::from_secs(20);

/// Default warm-up before the first pull, so the UI's initial reads settle
/// before a (potentially large) backfill write batch competes for the DB.
pub const DEFAULT_PULL_WARMUP: Duration = Duration::from_secs(4);

/// Channel capacity for pull-outcome broadcasts.
const OUTCOME_CHANNEL_CAPACITY: usize = 16;

/// Outcome of a pull attempt, broadcast to consumers.
#[derive(Debug, Clone)]
pub enum PullEvent {
    /// A pull fetched `pulled` new events and is **about to project them**.
    /// Emitted before the (potentially long) apply, so a UI can say so while it
    /// runs — [`Applied`](Self::Applied) is too late for that by construction,
    /// since it reports work already finished.
    ///
    /// This matters on a fresh or just-wiped device, where the first backfill
    /// *is* the app appearing: tens of thousands of events project behind a UI
    /// that would otherwise show an empty screen and a "Synced" chip.
    Applying { pulled: usize },
    /// A pull applied `pulled` new events (only emitted when `pulled > 0`), with
    /// `failed` of them failing to project. Consumers refetch on this.
    Applied { pulled: usize, failed: usize },
    /// A pull completed with nothing new.
    Idle,
    /// A pull attempt failed (network/server/local). Advisory — the next tick
    /// retries; the cursor only advances on success.
    Failed { error: String },
}

struct Inner {
    client: SyncClient,
    db: Database,
    projections: ProjectionRunner,
    trigger: Notify,
    shutdown: Notify,
    outcomes: broadcast::Sender<PullEvent>,
    interval: Duration,
    warmup: Duration,
}

/// Background pull scheduler. Clone is cheap (shares state).
#[derive(Clone)]
pub struct PullScheduler {
    inner: Arc<Inner>,
}

impl PullScheduler {
    /// Spawn with the default interval + warm-up.
    pub fn spawn(
        client: SyncClient,
        db: Database,
        projections: ProjectionRunner,
    ) -> (Self, JoinHandle<()>) {
        Self::spawn_with(
            client,
            db,
            projections,
            DEFAULT_PULL_INTERVAL,
            DEFAULT_PULL_WARMUP,
        )
    }

    /// Spawn with a custom interval + warm-up (tests use tiny values).
    pub fn spawn_with(
        client: SyncClient,
        db: Database,
        projections: ProjectionRunner,
        interval: Duration,
        warmup: Duration,
    ) -> (Self, JoinHandle<()>) {
        let (outcomes_tx, _rx) = broadcast::channel(OUTCOME_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            client,
            db,
            projections,
            trigger: Notify::new(),
            shutdown: Notify::new(),
            outcomes: outcomes_tx,
            interval,
            warmup,
        });
        let scheduler = Self {
            inner: inner.clone(),
        };
        let handle = tokio::spawn(run_loop(inner));
        (scheduler, handle)
    }

    /// Nudge an immediate pull (debounce-free — the loop pulls at once).
    pub fn trigger(&self) {
        self.inner.trigger.notify_one();
    }

    /// Subscribe to pull outcomes.
    pub fn subscribe(&self) -> broadcast::Receiver<PullEvent> {
        self.inner.outcomes.subscribe()
    }

    /// Stop the scheduler.
    pub fn shutdown(&self) {
        self.inner.shutdown.notify_one();
    }
}

async fn run_loop(inner: Arc<Inner>) {
    // Warm-up before the first (backfill) pull, but stay responsive to shutdown.
    tokio::select! {
        _ = inner.shutdown.notified() => return,
        _ = tokio::time::sleep(inner.warmup) => {}
    }
    pull_once(&inner).await;

    loop {
        tokio::select! {
            _ = inner.shutdown.notified() => return,
            _ = tokio::time::sleep(inner.interval) => pull_once(&inner).await,
            _ = inner.trigger.notified() => pull_once(&inner).await,
        }
    }
}

async fn pull_once(inner: &Arc<Inner>) {
    match inner.client.pull_only(&inner.db).await {
        Ok(outcome) if outcome.pulled > 0 => {
            // Announce the batch BEFORE projecting it. The apply below is the
            // slow part on a backfill, and `Applied` only fires once it's done.
            let _ = inner.outcomes.send(PullEvent::Applying {
                pulled: outcome.pulled,
            });
            let failed = inner
                .projections
                .apply_events_resilient(&outcome.pulled_events)
                .await;
            if failed > 0 {
                tracing::warn!(
                    failed,
                    pulled = outcome.pulled,
                    "auto-pull: some events failed to project"
                );
            } else {
                tracing::info!(pulled = outcome.pulled, "auto-pull applied");
            }
            let _ = inner.outcomes.send(PullEvent::Applied {
                pulled: outcome.pulled,
                failed,
            });
        }
        Ok(_) => {
            let _ = inner.outcomes.send(PullEvent::Idle);
        }
        Err(e) => {
            // Offline/unreachable is the common case — keep it quiet, retry next tick.
            tracing::debug!(error = %e, "auto-pull failed (retry next tick)");
            let _ = inner.outcomes.send(PullEvent::Failed {
                error: e.to_string(),
            });
        }
    }
}

/// Forward `NetworkEvent::Online` from a monitor into an immediate pull, so a
/// device that just regained connectivity converges without waiting a full
/// interval. Mirrors `accelerator::wire`. Exits when the monitor channel closes.
pub fn wire_network(
    monitor: &super::network::NetworkMonitor,
    scheduler: PullScheduler,
) -> JoinHandle<()> {
    let rx = monitor.subscribe();
    tokio::spawn(network_forward_loop(rx, scheduler))
}

async fn network_forward_loop(
    mut rx: broadcast::Receiver<super::network::NetworkEvent>,
    scheduler: PullScheduler,
) {
    use super::network::NetworkEvent;
    loop {
        match rx.recv().await {
            Ok(NetworkEvent::Online) => scheduler.trigger(),
            Ok(NetworkEvent::Offline) => {}
            Err(broadcast::error::RecvError::Closed) => return,
            // A lag may have hidden an Online — pull to be safe.
            Err(broadcast::error::RecvError::Lagged(_)) => scheduler.trigger(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventStore, NewEvent, NotesProjection, SurrealEventStore};
    use chrono::Utc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pull.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    /// A pull against an unreachable server reports `Failed` (not a panic) and
    /// the loop keeps ticking. Also proves warm-up is honored.
    #[tokio::test]
    async fn unreachable_server_reports_failed_and_keeps_ticking() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        // Seed a device event so there IS local state (pull still fails on the net).
        store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: "2026-04-19".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({ "journal_id": "2026-04-19", "date": "2026-04-19", "raw_text": "x" }),
            })
            .await
            .unwrap();

        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let client = SyncClient::new("http://127.0.0.1:1".into(), "d1".into()); // unreachable
        let (sched, _h) = PullScheduler::spawn_with(
            client,
            db.clone(),
            runner,
            Duration::from_millis(40),
            Duration::from_millis(10), // tiny warm-up
        );
        let mut sub = sched.subscribe();

        // Should see at least two Failed events (warm-up pull + one interval pull).
        let mut failures = 0;
        for _ in 0..6 {
            if let Ok(Ok(PullEvent::Failed { .. })) =
                tokio::time::timeout(Duration::from_millis(500), sub.recv()).await
            {
                failures += 1;
                if failures >= 2 {
                    break;
                }
            }
        }
        assert!(
            failures >= 2,
            "loop keeps retrying after failures (saw {failures})"
        );
        sched.shutdown();
    }

    /// `trigger()` forces an immediate pull rather than waiting a full interval.
    #[tokio::test]
    async fn trigger_forces_immediate_pull() {
        let db = test_db().await;
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let client = SyncClient::new("http://127.0.0.1:1".into(), "d1".into());
        // Long interval so only the trigger (or warm-up) can produce a prompt pull.
        let (sched, _h) = PullScheduler::spawn_with(
            client,
            db.clone(),
            runner,
            Duration::from_secs(3600),
            Duration::from_millis(10),
        );
        let mut sub = sched.subscribe();

        // Drain the warm-up pull outcome first.
        let _ = tokio::time::timeout(Duration::from_millis(500), sub.recv()).await;

        sched.trigger();
        // A triggered pull outcome should arrive well inside the (1h) interval.
        let got = tokio::time::timeout(Duration::from_millis(500), sub.recv()).await;
        assert!(got.is_ok(), "trigger() produced a prompt pull outcome");
        sched.shutdown();
    }
}
