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
use crate::events::{Event, ProjectionRunner};

use super::client::SyncClient;

/// Default quiet interval between background pulls.
pub const DEFAULT_PULL_INTERVAL: Duration = Duration::from_secs(20);

/// Default warm-up before the first pull, so the UI's initial reads settle
/// before a (potentially large) backfill write batch competes for the DB.
pub const DEFAULT_PULL_WARMUP: Duration = Duration::from_secs(4);

/// Channel capacity for pull-outcome broadcasts.
const OUTCOME_CHANNEL_CAPACITY: usize = 16;

/// Events per apply chunk on a backfill.
///
/// The apply used to run the whole batch in one call that logged nothing until
/// it returned. A dev phone sat on `Restoring 16052 events…` for eight hours
/// with every runtime thread asleep and produced not one line saying how far it
/// had got — the operation was unbounded *and* silent, so a stall and slow
/// progress looked identical. Chunking costs nothing (the apply is a sequential
/// per-event loop either way) and makes both the log and the UI count down.
const APPLY_CHUNK: usize = 500;

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
    ///
    /// Re-emitted after every [`APPLY_CHUNK`] with the count still **remaining**,
    /// so `pulled` counts down to 0. A consumer that only wants the announcement
    /// can take the first one; a consumer showing progress gets it for free, and
    /// a restore that has stopped dead shows a number that stops moving instead
    /// of an animation that means nothing.
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

/// Project `events` in [`APPLY_CHUNK`]-sized pieces, announcing how many are
/// still outstanding after each one. Returns the total that failed to project.
///
/// Split out of [`pull_once`] because this is the part worth testing on its own:
/// driving it through a real pull needs a reachable server, and the property
/// that matters — that a long apply keeps saying where it is — has nothing to do
/// with the network.
async fn apply_in_chunks(
    projections: &ProjectionRunner,
    events: &[Event],
    outcomes: &broadcast::Sender<PullEvent>,
) -> usize {
    let total = events.len();
    // Announce the batch BEFORE projecting it. The apply is the slow part on a
    // backfill, and `Applied` only fires once it is done.
    let _ = outcomes.send(PullEvent::Applying { pulled: total });
    let mut failed = 0usize;
    let mut done = 0usize;
    for chunk in events.chunks(APPLY_CHUNK) {
        failed += projections.apply_events_resilient(chunk).await;
        done += chunk.len();
        // Re-announce what is LEFT, so the indicator counts down and a stall
        // shows up as a number that stops moving.
        let _ = outcomes.send(PullEvent::Applying {
            pulled: total.saturating_sub(done),
        });
        tracing::info!(done, total, failed, "auto-pull projecting");
    }
    failed
}

async fn pull_once(inner: &Arc<Inner>) {
    match inner.client.pull_only(&inner.db).await {
        Ok(outcome) if outcome.pulled > 0 => {
            let failed =
                apply_in_chunks(&inner.projections, &outcome.pulled_events, &inner.outcomes).await;
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

    fn note_events(count: usize) -> Vec<Event> {
        let now = Utc::now();
        (0..count)
            .map(|i| Event {
                id: format!("ev-{i:05}"),
                event_type: "generic_note_created".into(),
                aggregate_id: format!("note-{i:05}"),
                timestamp: now,
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "note_id": format!("note-{i:05}"),
                    "title": "t",
                    "raw_text": "x",
                }),
                received_at: Some(now),
            })
            .collect()
    }

    /// A long apply reports how far it has got, rather than going silent until
    /// it finishes.
    ///
    /// The regression this pins: the apply ran the whole batch in one call and
    /// emitted nothing until it returned, so a device stuck partway through was
    /// indistinguishable from one making progress — for eight hours, on real
    /// hardware, with an animated banner as the only output.
    #[tokio::test]
    async fn a_long_apply_counts_down_as_it_goes() {
        let db = test_db().await;
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let (tx, mut rx) = broadcast::channel(64);
        let events = note_events(APPLY_CHUNK * 2 + 10);
        let total = events.len();

        let failed = apply_in_chunks(&runner, &events, &tx).await;
        assert_eq!(failed, 0, "plain note events all project");

        let mut announced = Vec::new();
        while let Ok(PullEvent::Applying { pulled }) = rx.try_recv() {
            announced.push(pulled);
        }
        assert_eq!(
            announced,
            vec![total, total - APPLY_CHUNK, total - APPLY_CHUNK * 2, 0],
            "the opening announcement, then one countdown per chunk, ending at 0",
        );
    }

    /// A batch smaller than one chunk still announces, then clears. Without the
    /// trailing 0 the indicator would stay up forever on a small pull.
    #[tokio::test]
    async fn a_short_apply_still_clears_the_indicator() {
        let db = test_db().await;
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let (tx, mut rx) = broadcast::channel(16);
        let events = note_events(3);
        assert_eq!(apply_in_chunks(&runner, &events, &tx).await, 0);

        let mut announced = Vec::new();
        while let Ok(PullEvent::Applying { pulled }) = rx.try_recv() {
            announced.push(pulled);
        }
        assert_eq!(announced, vec![3, 0]);
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
