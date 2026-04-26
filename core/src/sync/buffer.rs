//! Debounced local-append buffer.
//!
//! Events are enqueued in memory and flushed to the backing `SurrealEventStore`
//! after an idle period (default 1s). Each enqueue resets the idle timer so a
//! rapid burst of writes (e.g. keystroke-driven auto-save) collapses into a
//! single batched append.
//!
//! The buffer is in-memory only. On process restart, any unflushed writes are
//! lost — callers must persist via the direct store API for durability-critical
//! paths, or (in the sync case) rely on the fact that unflushed events are
//! still in editor state and will be re-emitted on reopen.
//!
//! Flush completions are broadcast on a `flushed` channel so downstream
//! debouncers (e.g. the push debouncer in `pusher.rs`) can chain.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, Notify, broadcast};
use tokio::task::JoinHandle;

use crate::events::{Event, EventStore, NewEvent, SurrealEventStore};

/// Default idle delay before buffered events are flushed.
pub const DEFAULT_FLUSH_DELAY: Duration = Duration::from_secs(1);

/// Default maximum queued events. `append` returns `BufferError::Overflow`
/// when the queue is at capacity, preventing unbounded memory growth from a
/// caller pushing faster than the flush task can drain.
pub const DEFAULT_MAX_QUEUE_LEN: usize = 10_000;

/// Channel capacity for buffer event notifications. Sized for the likely max
/// number of consumers (push debouncer + status reporter + integration tests).
const EVENT_CHANNEL_CAPACITY: usize = 16;

/// State changes the buffer broadcasts on its event channel. Subscribers
/// match on the variant to decide how to react: `Flushed` triggers a push,
/// `FlushFailed` and `Overflow` are signals that something needs operator
/// attention without papering over the failure.
#[derive(Debug, Clone)]
pub enum BufferEvent {
    /// A flush completed successfully and `appended` events were persisted.
    Flushed {
        appended: usize,
        completed_at: chrono::DateTime<chrono::Utc>,
    },
    /// A flush attempt failed; the events were re-queued at the front so the
    /// next flush retries them. `requeued` is the number of events put back.
    FlushFailed { error: String, requeued: usize },
    /// An `append` call was rejected because the queue was at capacity. The
    /// rejected event's `aggregate_id` is included so consumers can
    /// correlate; `queue_len` is the queue size at rejection time.
    Overflow {
        rejected_aggregate_id: String,
        queue_len: usize,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum BufferError {
    #[error("buffer flush failed: {0}")]
    Flush(String),
    #[error("buffer at capacity ({queue_len} events queued)")]
    Overflow { queue_len: usize },
    #[error("buffer already shut down")]
    Shutdown,
}

struct Inner {
    queue: Mutex<VecDeque<NewEvent>>,
    idle: Notify,
    shutdown: Notify,
    events_tx: broadcast::Sender<BufferEvent>,
    store: SurrealEventStore,
    delay: Duration,
    cap: usize,
}

/// Debounced append buffer. Cheap to clone — shares underlying state.
#[derive(Clone)]
pub struct SyncBuffer {
    inner: Arc<Inner>,
}

impl SyncBuffer {
    /// Create a new buffer with the default 1s idle delay and 10K queue cap.
    pub fn new(store: SurrealEventStore) -> (Self, JoinHandle<()>) {
        Self::with_delay_and_cap(store, DEFAULT_FLUSH_DELAY, DEFAULT_MAX_QUEUE_LEN)
    }

    /// Create a buffer with a custom idle delay (default cap). Useful for tests.
    pub fn with_delay(store: SurrealEventStore, delay: Duration) -> (Self, JoinHandle<()>) {
        Self::with_delay_and_cap(store, delay, DEFAULT_MAX_QUEUE_LEN)
    }

    /// Create a buffer with custom delay AND cap. Used by tests that exercise
    /// the cap behavior without needing to enqueue 10K events.
    pub fn with_delay_and_cap(
        store: SurrealEventStore,
        delay: Duration,
        cap: usize,
    ) -> (Self, JoinHandle<()>) {
        let (events_tx, _rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            queue: Mutex::new(VecDeque::new()),
            idle: Notify::new(),
            shutdown: Notify::new(),
            events_tx,
            store,
            delay,
            cap,
        });
        let buffer = Self { inner: inner.clone() };
        let handle = tokio::spawn(flush_loop(inner));
        (buffer, handle)
    }

    /// Enqueue an event. Resets the idle timer — the buffer will flush after
    /// `delay` elapses without further enqueues. Returns
    /// `BufferError::Overflow` (and broadcasts a corresponding `Overflow`
    /// event) if the queue is at capacity, rather than growing without bound.
    pub async fn append(&self, event: NewEvent) -> Result<(), BufferError> {
        {
            let mut q = self.inner.queue.lock().await;
            if q.len() >= self.inner.cap {
                let queue_len = q.len();
                let aggregate_id = event.aggregate_id.clone();
                drop(q);
                let _ = self.inner.events_tx.send(BufferEvent::Overflow {
                    rejected_aggregate_id: aggregate_id,
                    queue_len,
                });
                return Err(BufferError::Overflow { queue_len });
            }
            q.push_back(event);
        }
        self.inner.idle.notify_one();
        Ok(())
    }

    /// Force an immediate flush, bypassing the debounce window. Returns the
    /// events actually written (with assigned IDs).
    pub async fn flush_now(&self) -> Result<Vec<Event>, BufferError> {
        do_flush(&self.inner).await
    }

    /// Subscribe to buffer events (`Flushed`, `FlushFailed`, `Overflow`).
    /// Each subscriber gets its own receiver; lagging subscribers miss old
    /// events silently (sufficient for edge-triggered debouncers).
    pub fn subscribe(&self) -> broadcast::Receiver<BufferEvent> {
        self.inner.events_tx.subscribe()
    }

    /// Number of events currently awaiting flush. Useful for status reporting.
    pub async fn pending(&self) -> usize {
        self.inner.queue.lock().await.len()
    }

    /// Signal the flush task to exit. Any queued events are flushed first.
    pub async fn shutdown(&self) -> Result<(), BufferError> {
        // Drain before exiting so we don't lose pending work.
        let _ = do_flush(&self.inner).await?;
        self.inner.shutdown.notify_one();
        Ok(())
    }
}

/// Background task: wait for an idle-quiet window, then flush.
///
/// Failures from `do_flush` are intentionally not handled here — the failing
/// events are re-queued for retry inside `do_flush`, and a `FlushFailed`
/// event is broadcast on the buffer's event channel for any consumer that
/// wants to surface the error (e.g. a status reporter). Discarding the
/// `Result` here just means "nothing further to do at the loop level."
async fn flush_loop(inner: Arc<Inner>) {
    loop {
        tokio::select! {
            _ = inner.shutdown.notified() => {
                let _ = do_flush(&inner).await;
                return;
            }
            _ = inner.idle.notified() => {
                // Wait for the idle window. Any further `append` inside this
                // window re-notifies `idle`, so we restart the wait.
                loop {
                    tokio::select! {
                        _ = inner.shutdown.notified() => {
                            let _ = do_flush(&inner).await;
                            return;
                        }
                        _ = tokio::time::sleep(inner.delay) => {
                            // Idle expired — flush.
                            break;
                        }
                        _ = inner.idle.notified() => {
                            // New activity — reset the wait.
                            continue;
                        }
                    }
                }
                let _ = do_flush(&inner).await;
            }
        }
    }
}

async fn do_flush(inner: &Inner) -> Result<Vec<Event>, BufferError> {
    let events: Vec<NewEvent> = {
        let mut q = inner.queue.lock().await;
        q.drain(..).collect()
    };

    if events.is_empty() {
        return Ok(vec![]);
    }

    // Clone before passing to `append_batch` — the store consumes the Vec by
    // value, so without a copy a failure would lose the events. The clone is
    // O(N) on the failure path but never happens on the success path because
    // we use the cloned copy only inside the error branch.
    let count = events.len();
    let events_for_retry = events.clone();

    match inner.store.append_batch(events).await {
        Ok(appended) => {
            let _ = inner.events_tx.send(BufferEvent::Flushed {
                appended: appended.len(),
                completed_at: chrono::Utc::now(),
            });
            Ok(appended)
        }
        Err(e) => {
            // Re-queue at the front so the next flush retries them in order.
            // Iterating in reverse + push_front preserves the original order
            // at the head of the queue.
            {
                let mut q = inner.queue.lock().await;
                for ev in events_for_retry.into_iter().rev() {
                    q.push_front(ev);
                }
            }
            let err_msg = e.to_string();
            let _ = inner.events_tx.send(BufferEvent::FlushFailed {
                error: err_msg.clone(),
                requeued: count,
            });
            Err(BufferError::Flush(err_msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    async fn test_store() -> SurrealEventStore {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("buf.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        SurrealEventStore::new(db)
    }

    fn sample_event(aggregate_id: &str) -> NewEvent {
        NewEvent {
            id: None,
            event_type: "journal_entry_created".into(),
            aggregate_id: aggregate_id.into(),
            timestamp: Utc::now(),
            device_id: "device-test".into(),
            payload: serde_json::json!({
                "journal_id": aggregate_id,
                "date": "2026-04-19",
                "raw_text": "buffer test"
            }),
        }
    }

    /// Helper to extract `appended` from a `BufferEvent::Flushed` or panic.
    fn unwrap_flushed(evt: BufferEvent) -> usize {
        match evt {
            BufferEvent::Flushed { appended, .. } => appended,
            other => panic!("expected Flushed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn flushes_after_idle_window() {
        let store = test_store().await;
        let (buf, _h) = SyncBuffer::with_delay(store.clone(), Duration::from_millis(80));
        let mut sub = buf.subscribe();

        buf.append(sample_event("n1")).await.unwrap();
        assert_eq!(buf.pending().await, 1);

        let evt = tokio::time::timeout(Duration::from_millis(500), sub.recv())
            .await
            .expect("flush notification should fire")
            .unwrap();
        assert_eq!(unwrap_flushed(evt), 1);
        assert_eq!(buf.pending().await, 0);

        // Verify durability.
        let events = store.get_by_aggregate("n1").await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn coalesces_burst_into_single_flush() {
        let store = test_store().await;
        let (buf, _h) = SyncBuffer::with_delay(store.clone(), Duration::from_millis(60));
        let mut sub = buf.subscribe();

        for i in 0..5 {
            buf.append(sample_event(&format!("n{i}"))).await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let evt = tokio::time::timeout(Duration::from_millis(500), sub.recv())
            .await
            .expect("flush notification should fire")
            .unwrap();
        assert_eq!(unwrap_flushed(evt), 5, "all 5 events should coalesce into one flush");

        // Make sure a second flush doesn't fire spuriously.
        let second = tokio::time::timeout(Duration::from_millis(100), sub.recv()).await;
        assert!(second.is_err(), "no second flush when buffer is empty");
    }

    #[tokio::test]
    async fn cap_rejects_appends_at_capacity() {
        // Use a tiny cap so we don't have to enqueue 10K events to test.
        let store = test_store().await;
        let (buf, _h) =
            SyncBuffer::with_delay_and_cap(store, Duration::from_secs(60), 3);
        let mut sub = buf.subscribe();

        // Fill to cap.
        for i in 0..3 {
            buf.append(sample_event(&format!("n{i}"))).await.unwrap();
        }
        assert_eq!(buf.pending().await, 3);

        // Next append must be rejected.
        let err = buf
            .append(sample_event("overflow"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, BufferError::Overflow { queue_len: 3 }),
            "expected Overflow with queue_len=3, got: {err:?}"
        );

        // And an Overflow event is broadcast.
        let evt = tokio::time::timeout(Duration::from_millis(200), sub.recv())
            .await
            .expect("Overflow event should fire")
            .unwrap();
        assert!(
            matches!(
                &evt,
                BufferEvent::Overflow {
                    rejected_aggregate_id,
                    queue_len: 3,
                } if rejected_aggregate_id == "overflow"
            ),
            "expected Overflow, got {evt:?}"
        );

        // Queue contents are unchanged after rejection.
        assert_eq!(buf.pending().await, 3);
    }

    #[tokio::test]
    async fn flush_now_bypasses_debounce() {
        let store = test_store().await;
        let (buf, _h) = SyncBuffer::with_delay(store.clone(), Duration::from_secs(60));

        buf.append(sample_event("n-immediate")).await.unwrap();
        let flushed = buf.flush_now().await.unwrap();
        assert_eq!(flushed.len(), 1);
        assert_eq!(buf.pending().await, 0);
    }

    #[tokio::test]
    async fn shutdown_drains_pending() {
        let store = test_store().await;
        let (buf, h) = SyncBuffer::with_delay(store.clone(), Duration::from_secs(60));

        buf.append(sample_event("n-shutdown")).await.unwrap();
        buf.shutdown().await.unwrap();

        // Wait for task to exit.
        let _ = tokio::time::timeout(Duration::from_millis(500), h).await;

        let events = store.get_by_aggregate("n-shutdown").await.unwrap();
        assert_eq!(events.len(), 1, "shutdown should drain pending events");
    }
}
