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

/// Channel capacity for flush notifications. Sized for the likely max number
/// of consumers (push debouncer + status reporter + integration tests).
const FLUSH_CHANNEL_CAPACITY: usize = 16;

/// Result of a flush operation.
#[derive(Debug, Clone)]
pub struct FlushResult {
    /// Number of events that were successfully appended to the store.
    pub appended: usize,
    /// Timestamp the flush completed.
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum BufferError {
    #[error("buffer flush failed: {0}")]
    Flush(String),
    #[error("buffer already shut down")]
    Shutdown,
}

struct Inner {
    queue: Mutex<VecDeque<NewEvent>>,
    idle: Notify,
    shutdown: Notify,
    flushed_tx: broadcast::Sender<FlushResult>,
    store: SurrealEventStore,
    delay: Duration,
}

/// Debounced append buffer. Cheap to clone — shares underlying state.
#[derive(Clone)]
pub struct SyncBuffer {
    inner: Arc<Inner>,
}

impl SyncBuffer {
    /// Create a new buffer with the default 1s idle delay and spawn its flush task.
    pub fn new(store: SurrealEventStore) -> (Self, JoinHandle<()>) {
        Self::with_delay(store, DEFAULT_FLUSH_DELAY)
    }

    /// Create a buffer with a custom idle delay. Useful for tests.
    pub fn with_delay(store: SurrealEventStore, delay: Duration) -> (Self, JoinHandle<()>) {
        let (flushed_tx, _rx) = broadcast::channel(FLUSH_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            queue: Mutex::new(VecDeque::new()),
            idle: Notify::new(),
            shutdown: Notify::new(),
            flushed_tx,
            store,
            delay,
        });
        let buffer = Self { inner: inner.clone() };
        let handle = tokio::spawn(flush_loop(inner));
        (buffer, handle)
    }

    /// Enqueue an event. Resets the idle timer — the buffer will flush after
    /// `delay` elapses without further enqueues.
    pub async fn append(&self, event: NewEvent) -> Result<(), BufferError> {
        {
            let mut q = self.inner.queue.lock().await;
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

    /// Subscribe to flush completion events. Each subscriber gets its own
    /// receiver; lagging subscribers miss old events silently (sufficient for
    /// edge-triggered debouncers).
    pub fn subscribe(&self) -> broadcast::Receiver<FlushResult> {
        self.inner.flushed_tx.subscribe()
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
async fn flush_loop(inner: Arc<Inner>) {
    loop {
        tokio::select! {
            _ = inner.shutdown.notified() => {
                // Final drain in case anything snuck in between shutdown's
                // flush and the notify.
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

    let appended = inner
        .store
        .append_batch(events)
        .await
        .map_err(|e| BufferError::Flush(e.to_string()))?;

    // Notify subscribers (don't care if nobody is listening).
    let _ = inner.flushed_tx.send(FlushResult {
        appended: appended.len(),
        completed_at: chrono::Utc::now(),
    });

    Ok(appended)
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

    #[tokio::test]
    async fn flushes_after_idle_window() {
        let store = test_store().await;
        let (buf, _h) = SyncBuffer::with_delay(store.clone(), Duration::from_millis(80));
        let mut sub = buf.subscribe();

        buf.append(sample_event("n1")).await.unwrap();
        assert_eq!(buf.pending().await, 1);

        let flushed = tokio::time::timeout(Duration::from_millis(500), sub.recv())
            .await
            .expect("flush notification should fire")
            .unwrap();
        assert_eq!(flushed.appended, 1);
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

        let flushed = tokio::time::timeout(Duration::from_millis(500), sub.recv())
            .await
            .expect("flush notification should fire")
            .unwrap();
        assert_eq!(flushed.appended, 5, "all 5 events should coalesce into one flush");

        // Make sure a second flush doesn't fire spuriously.
        let second = tokio::time::timeout(Duration::from_millis(100), sub.recv()).await;
        assert!(second.is_err(), "no second flush when buffer is empty");
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
