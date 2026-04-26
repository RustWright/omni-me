//! Debounced sync push.
//!
//! A background task that waits for buffer-flush notifications, debounces
//! further flushes, then calls `SyncClient::push_only`. On failure, the
//! exponential-backoff retry loop in `retry.rs` takes over.
//!
//! Design:
//! - The pusher subscribes to a [`SyncBuffer`]'s flush channel.
//! - After a flush, it waits `DEFAULT_PUSH_DELAY` (2s) for quiet.
//! - If another flush arrives inside that window, the timer resets.
//! - When the window expires, it invokes `SyncClient::push_only` against the
//!   database's current `sync_state` timestamp.
//! - Success and failure are reported out via a broadcast channel so the
//!   status reporter (Task 2.6) and retry loop (Task 2.3) can react.
//!
//! The pusher does not implement retry itself — that's `retry.rs`'s job. It
//! just signals success/failure once and lets the caller decide.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify, broadcast};
use tokio::task::JoinHandle;

use crate::db::Database;

use super::buffer::SyncBuffer;
use super::client::{PushOutcome, SyncClient, SyncError};

/// Default quiet period after a local flush before a sync push is attempted.
pub const DEFAULT_PUSH_DELAY: Duration = Duration::from_secs(2);

/// Channel capacity for push-outcome broadcasts.
const OUTCOME_CHANNEL_CAPACITY: usize = 16;

/// Outcome of a push attempt, broadcast to consumers.
#[derive(Debug, Clone)]
pub enum PushEvent {
    Started,
    Succeeded {
        pushed: usize,
    },
    Failed {
        error: String,
    },
}

struct Inner {
    client: SyncClient,
    db: Database,
    trigger: Notify,
    shutdown: Notify,
    outcomes: broadcast::Sender<PushEvent>,
    last_known_events: Mutex<u64>,
    delay: Duration,
}

/// Debounced push orchestrator. Clone is cheap.
#[derive(Clone)]
pub struct PushDebouncer {
    inner: Arc<Inner>,
}

impl PushDebouncer {
    /// Spawn a debouncer wired to the given `buffer`'s flush channel. Returns
    /// the handle plus a join on the background task.
    pub fn spawn(
        client: SyncClient,
        db: Database,
        buffer: &SyncBuffer,
    ) -> (Self, JoinHandle<()>) {
        Self::spawn_with_delay(client, db, buffer, DEFAULT_PUSH_DELAY)
    }

    pub fn spawn_with_delay(
        client: SyncClient,
        db: Database,
        buffer: &SyncBuffer,
        delay: Duration,
    ) -> (Self, JoinHandle<()>) {
        let (outcomes_tx, _rx) = broadcast::channel(OUTCOME_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            client,
            db,
            trigger: Notify::new(),
            shutdown: Notify::new(),
            outcomes: outcomes_tx,
            last_known_events: Mutex::new(0),
            delay,
        });
        let debouncer = Self { inner: inner.clone() };

        // Fan the buffer flush notifications into the trigger Notify.
        let flush_rx = buffer.subscribe();
        let inner_for_flush = inner.clone();
        tokio::spawn(forward_flush(flush_rx, inner_for_flush));

        let handle = tokio::spawn(run_loop(inner));
        (debouncer, handle)
    }

    /// Manually nudge the debouncer to consider a push. The debounce window
    /// still applies — this only resets/starts the pending timer.
    pub fn trigger(&self) {
        self.inner.trigger.notify_one();
    }

    /// Subscribe to push outcomes.
    pub fn subscribe(&self) -> broadcast::Receiver<PushEvent> {
        self.inner.outcomes.subscribe()
    }

    /// Stop the debouncer. No final push is performed.
    pub fn shutdown(&self) {
        self.inner.shutdown.notify_one();
    }
}

async fn forward_flush(
    mut rx: broadcast::Receiver<super::buffer::BufferEvent>,
    inner: Arc<Inner>,
) {
    use super::buffer::BufferEvent;
    loop {
        match rx.recv().await {
            Ok(BufferEvent::Flushed { .. }) => {
                inner.trigger.notify_one();
            }
            Ok(BufferEvent::FlushFailed { .. }) | Ok(BufferEvent::Overflow { .. }) => {
                // Local persist failed or backpressure — nothing to push, so
                // don't trigger. The error is broadcast separately for any
                // upstream surface that wants to react.
            }
            Err(broadcast::error::RecvError::Closed) => return,
            Err(broadcast::error::RecvError::Lagged(_)) => {
                // Treat a lag as a trigger — we still want to push soon.
                inner.trigger.notify_one();
            }
        }
    }
}

async fn run_loop(inner: Arc<Inner>) {
    loop {
        tokio::select! {
            _ = inner.shutdown.notified() => return,
            _ = inner.trigger.notified() => {
                // Debounce: each new trigger resets the wait.
                loop {
                    tokio::select! {
                        _ = inner.shutdown.notified() => return,
                        _ = tokio::time::sleep(inner.delay) => break,
                        _ = inner.trigger.notified() => continue,
                    }
                }

                attempt_push(&inner).await;
            }
        }
    }
}

async fn attempt_push(inner: &Arc<Inner>) {
    let _ = inner.outcomes.send(PushEvent::Started);

    let since = match inner.client.last_sync_timestamp(&inner.db).await {
        Ok(t) => t,
        Err(e) => {
            let _ = inner.outcomes.send(PushEvent::Failed {
                error: e.to_string(),
            });
            return;
        }
    };

    match inner.client.push_only(&inner.db, &since).await {
        Ok(PushOutcome { pushed }) => {
            let _ = inner.outcomes.send(PushEvent::Succeeded { pushed });
            let mut counter = inner.last_known_events.lock().await;
            *counter = counter.saturating_add(pushed as u64);
        }
        Err(e) => {
            let msg = match &e {
                SyncError::Network(m) => format!("network: {m}"),
                SyncError::Server(m) => format!("server: {m}"),
                SyncError::Local(m) => format!("local: {m}"),
            };
            let _ = inner.outcomes.send(PushEvent::Failed { error: msg });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventStore, NewEvent, SurrealEventStore};
    use chrono::Utc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("push.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn sample_event(device_id: &str, aggregate_id: &str) -> NewEvent {
        NewEvent {
            id: None,
            event_type: "journal_entry_created".into(),
            aggregate_id: aggregate_id.into(),
            timestamp: Utc::now(),
            device_id: device_id.into(),
            payload: serde_json::json!({
                "journal_id": aggregate_id,
                "date": "2026-04-19",
                "raw_text": "pusher test"
            }),
        }
    }

    /// Debouncer wired to a dummy unreachable URL — verifies the Failed event
    /// is emitted after the push attempt.
    #[tokio::test]
    async fn push_failure_is_reported() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        store
            .append(sample_event("device-x", "note-1"))
            .await
            .unwrap();

        let client = SyncClient::new(
            "http://127.0.0.1:1".into(), // unreachable
            "device-x".into(),
        );
        let (buffer, _bh) = SyncBuffer::with_delay(store.clone(), Duration::from_millis(20));
        let (pusher, _ph) = PushDebouncer::spawn_with_delay(
            client,
            db.clone(),
            &buffer,
            Duration::from_millis(30),
        );
        let mut sub = pusher.subscribe();

        // Manually trigger — no need to go through the buffer.
        pusher.trigger();

        // Expect: Started → Failed.
        let mut saw_started = false;
        let mut saw_failed = false;
        for _ in 0..4 {
            let ev = tokio::time::timeout(Duration::from_millis(2000), sub.recv())
                .await
                .expect("should get an event");
            if let Ok(ev) = ev {
                match ev {
                    PushEvent::Started => saw_started = true,
                    PushEvent::Failed { error } => {
                        saw_failed = true;
                        assert!(error.contains("network"), "expected network error, got {error}");
                        break;
                    }
                    _ => {}
                }
            }
        }
        assert!(saw_started);
        assert!(saw_failed);
        pusher.shutdown();
    }

    /// Multiple rapid triggers inside the debounce window coalesce into ONE
    /// Started event.
    #[tokio::test]
    async fn triggers_coalesce_within_window() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        store
            .append(sample_event("device-x", "note-coalesce"))
            .await
            .unwrap();

        let client = SyncClient::new("http://127.0.0.1:1".into(), "device-x".into());
        let (buffer, _bh) = SyncBuffer::with_delay(store, Duration::from_secs(60));
        let (pusher, _ph) = PushDebouncer::spawn_with_delay(
            client,
            db,
            &buffer,
            Duration::from_millis(80),
        );
        let mut sub = pusher.subscribe();

        for _ in 0..5 {
            pusher.trigger();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        // Within ~300ms we should see exactly one Started (then a Failed,
        // because the URL is unreachable).
        let first = tokio::time::timeout(Duration::from_millis(800), sub.recv())
            .await
            .expect("first event")
            .unwrap();
        assert!(matches!(first, PushEvent::Started));

        // Drain the Failed that follows. Unreachable URL → network error.
        let second = tokio::time::timeout(Duration::from_millis(2000), sub.recv())
            .await
            .expect("second event");
        assert!(
            matches!(second, Ok(PushEvent::Failed { .. })),
            "expected Failed, got {second:?}"
        );

        // No further Started inside a 200ms idle window.
        let third = tokio::time::timeout(Duration::from_millis(200), sub.recv()).await;
        assert!(third.is_err(), "no second push inside idle window");
        pusher.shutdown();
    }
}
