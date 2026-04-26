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

use crate::events::{Event, EventStore, NewEvent};

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
    store: Arc<dyn EventStore + Send + Sync>,
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
    pub fn new(store: Arc<dyn EventStore + Send + Sync>) -> (Self, JoinHandle<()>) {
        Self::with_delay_and_cap(store, DEFAULT_FLUSH_DELAY, DEFAULT_MAX_QUEUE_LEN)
    }

    /// Create a buffer with a custom idle delay (default cap). Useful for tests.
    pub fn with_delay(store: Arc<dyn EventStore + Send + Sync>, delay: Duration) -> (Self, JoinHandle<()>) {
        Self::with_delay_and_cap(store, delay, DEFAULT_MAX_QUEUE_LEN)
    }

    /// Create a buffer with custom delay AND cap. Used by tests that exercise
    /// the cap behavior without needing to enqueue 10K events.
    pub fn with_delay_and_cap(
        store: Arc<dyn EventStore + Send + Sync>,
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
    use crate::events::{EventError, SurrealEventStore};
    use chrono::Utc;
    use std::collections::VecDeque;

    async fn test_store() -> Arc<dyn EventStore + Send + Sync> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("buf.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        Arc::new(SurrealEventStore::new(db))
    }

    /// Test-only mock that returns pre-canned responses to `append_batch` calls.
    /// Captures every batch passed in so tests can assert exact call sequences.
    struct ScriptedStore {
        /// Pre-canned responses popped from the front in order. Panics if exhausted
        /// (test setup bug — always queue enough responses).
        responses: tokio::sync::Mutex<VecDeque<Result<(), EventError>>>,
        /// Captured `append_batch` arguments, in call order.
        received: tokio::sync::Mutex<Vec<Vec<NewEvent>>>,
        /// Optional gate: if set, `append_batch` blocks on this Notify before
        /// returning. Lets tests interleave a concurrent `append` with an
        /// in-flight flush.
        gate: Option<std::sync::Arc<tokio::sync::Notify>>,
    }

    #[async_trait::async_trait]
    impl EventStore for ScriptedStore {
        async fn append_batch(&self, events: Vec<NewEvent>) -> Result<Vec<Event>, EventError> {
            if let Some(gate) = &self.gate {
                gate.notified().await;
            }
            self.received.lock().await.push(events.clone());
            match self.responses.lock().await.pop_front() {
                Some(Ok(())) => Ok(events.into_iter().map(synthesize_event).collect()),
                Some(Err(e)) => Err(e),
                None => panic!("ScriptedStore: no more pre-canned responses"),
            }
        }
        // SyncBuffer never calls these — fail loudly if that ever changes:
        async fn append(&self, _: NewEvent) -> Result<Event, EventError> {
            unimplemented!("ScriptedStore: append never called by SyncBuffer")
        }
        async fn get_since(
            &self,
            _: chrono::DateTime<chrono::Utc>,
            _: Option<&str>,
        ) -> Result<Vec<Event>, EventError> {
            unimplemented!()
        }
        async fn get_since_by_device(
            &self,
            _: chrono::DateTime<chrono::Utc>,
            _: &str,
        ) -> Result<Vec<Event>, EventError> {
            unimplemented!()
        }
        async fn get_by_aggregate(&self, _: &str) -> Result<Vec<Event>, EventError> {
            unimplemented!()
        }
        async fn purge_all(&self) -> Result<(), EventError> {
            unimplemented!()
        }
    }

    fn synthesize_event(ne: NewEvent) -> Event {
        Event {
            id: ne.id.unwrap_or_else(|| ulid::Ulid::new().to_string()),
            event_type: ne.event_type,
            aggregate_id: ne.aggregate_id,
            timestamp: ne.timestamp,
            device_id: ne.device_id,
            payload: ne.payload,
        }
    }

    fn scripted(responses: Vec<Result<(), EventError>>) -> Arc<dyn EventStore + Send + Sync> {
        Arc::new(ScriptedStore {
            responses: tokio::sync::Mutex::new(responses.into_iter().collect()),
            received: tokio::sync::Mutex::new(Vec::new()),
            gate: None,
        })
    }

    fn scripted_with_gate(
        responses: Vec<Result<(), EventError>>,
        gate: std::sync::Arc<tokio::sync::Notify>,
    ) -> Arc<ScriptedStore> {
        Arc::new(ScriptedStore {
            responses: tokio::sync::Mutex::new(responses.into_iter().collect()),
            received: tokio::sync::Mutex::new(Vec::new()),
            gate: Some(gate),
        })
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

    #[tokio::test]
    async fn flush_failure_keeps_events_in_queue_in_order() {
        let store_raw = Arc::new(ScriptedStore {
            responses: tokio::sync::Mutex::new(
                vec![
                    Err(EventError::Validation("scripted failure".into())),
                    Ok(()),
                ]
                .into_iter()
                .collect(),
            ),
            received: tokio::sync::Mutex::new(Vec::new()),
            gate: None,
        });
        let store: Arc<dyn EventStore + Send + Sync> = store_raw.clone();
        let (buf, _h) = SyncBuffer::with_delay(store, Duration::from_secs(60));

        buf.append(sample_event("e1")).await.unwrap();
        buf.append(sample_event("e2")).await.unwrap();
        buf.append(sample_event("e3")).await.unwrap();

        // First flush fails — events should be re-queued.
        let _ = buf.flush_now().await;
        assert_eq!(buf.pending().await, 3, "events must stay queued after failure");

        // Second flush succeeds — the same 3 events arrive in original order.
        let flushed = buf.flush_now().await.unwrap();
        assert_eq!(flushed.len(), 3);

        let received = store_raw.received.lock().await;
        assert_eq!(received.len(), 2);
        let second_batch_ids: Vec<_> = received[1].iter().map(|e| e.aggregate_id.as_str()).collect();
        assert_eq!(second_batch_ids, ["e1", "e2", "e3"]);
    }

    #[tokio::test]
    async fn flush_failure_then_success_persists_in_order() {
        let store_raw = Arc::new(ScriptedStore {
            responses: tokio::sync::Mutex::new(
                vec![
                    Err(EventError::Validation("scripted failure".into())),
                    Ok(()),
                ]
                .into_iter()
                .collect(),
            ),
            received: tokio::sync::Mutex::new(Vec::new()),
            gate: None,
        });
        let store: Arc<dyn EventStore + Send + Sync> = store_raw.clone();
        let (buf, _h) = SyncBuffer::with_delay(store, Duration::from_secs(60));

        buf.append(sample_event("a1")).await.unwrap();
        buf.append(sample_event("a2")).await.unwrap();

        let _ = buf.flush_now().await;

        // Subscribe AFTER the failed flush so `sub.recv()` only sees the
        // success-path Flushed event below — broadcast::Receiver delivers
        // events oldest-first, so subscribing earlier would surface the
        // FlushFailed first and the assertion would never reach Flushed.
        let mut sub = buf.subscribe();

        let flushed = buf.flush_now().await.unwrap();
        assert_eq!(flushed.len(), 2);

        let received = store_raw.received.lock().await;
        // Both attempts carried the same two events in the same order.
        assert_eq!(received[0].len(), 2);
        assert_eq!(received[1].len(), 2);
        let first_ids: Vec<_> = received[0].iter().map(|e| e.aggregate_id.as_str()).collect();
        let second_ids: Vec<_> = received[1].iter().map(|e| e.aggregate_id.as_str()).collect();
        assert_eq!(first_ids, second_ids);
        drop(received);

        // A Flushed broadcast fired after the successful second flush.
        let evt = tokio::time::timeout(Duration::from_millis(200), sub.recv())
            .await
            .expect("Flushed event should be buffered")
            .unwrap();
        assert_eq!(unwrap_flushed(evt), 2);
    }

    #[tokio::test]
    async fn flush_failure_broadcasts_flush_failed_with_correct_count() {
        let store = scripted(vec![Err(EventError::Validation("scripted failure".into()))]);
        let (buf, _h) = SyncBuffer::with_delay(store, Duration::from_secs(60));
        let mut sub = buf.subscribe();

        buf.append(sample_event("f1")).await.unwrap();
        buf.append(sample_event("f2")).await.unwrap();

        let _ = buf.flush_now().await;

        let evt = tokio::time::timeout(Duration::from_millis(200), sub.recv())
            .await
            .expect("FlushFailed event should be buffered")
            .unwrap();
        match evt {
            BufferEvent::FlushFailed { requeued, error } => {
                assert_eq!(requeued, 2);
                assert!(!error.is_empty());
            }
            other => panic!("expected FlushFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn concurrent_append_during_failed_flush_preserves_order() {
        let gate = std::sync::Arc::new(tokio::sync::Notify::new());
        let store_raw = scripted_with_gate(
            vec![
                Err(EventError::Validation("scripted failure".into())),
                Ok(()),
            ],
            gate.clone(),
        );
        let store: Arc<dyn EventStore + Send + Sync> = store_raw.clone();
        let (buf, _h) = SyncBuffer::with_delay(store, Duration::from_secs(60));

        // Push 3 events and start a flush — it will block on the gate.
        buf.append(sample_event("c1")).await.unwrap();
        buf.append(sample_event("c2")).await.unwrap();
        buf.append(sample_event("c3")).await.unwrap();

        let buf2 = buf.clone();
        let flush_task = tokio::spawn(async move { buf2.flush_now().await });

        // Spin until the store has received the in-flight batch (gate blocked).
        for _ in 0..200 {
            if !store_raw.received.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // Append a 4th event while the flush is blocked inside append_batch.
        buf.append(sample_event("c4")).await.unwrap();

        // Release the gate — store returns Err, events re-queued at front.
        gate.notify_one();
        let _ = flush_task.await.unwrap();

        // The gate guards EVERY append_batch call, not just the first. Notify
        // it again so the success-path flush below isn't blocked. (Notify
        // stores one permit; the next `notified().await` consumes it.)
        gate.notify_one();

        // Second flush (success): should see [c1, c2, c3, c4] in that order.
        let flushed = buf.flush_now().await.unwrap();
        assert_eq!(flushed.len(), 4);

        let received = store_raw.received.lock().await;
        assert_eq!(received.len(), 2);
        let second_ids: Vec<_> = received[1].iter().map(|e| e.aggregate_id.as_str()).collect();
        assert_eq!(second_ids, ["c1", "c2", "c3", "c4"]);
    }

    #[tokio::test]
    async fn repeated_failures_recover_on_eventual_success() {
        let store_raw = Arc::new(ScriptedStore {
            responses: tokio::sync::Mutex::new(
                vec![
                    Err(EventError::Validation("fail 1".into())),
                    Err(EventError::Validation("fail 2".into())),
                    Ok(()),
                ]
                .into_iter()
                .collect(),
            ),
            received: tokio::sync::Mutex::new(Vec::new()),
            gate: None,
        });
        let store: Arc<dyn EventStore + Send + Sync> = store_raw.clone();
        let (buf, _h) = SyncBuffer::with_delay(store, Duration::from_secs(60));

        buf.append(sample_event("r1")).await.unwrap();
        buf.append(sample_event("r2")).await.unwrap();

        // Three flush cycles: fail, fail, succeed.
        let _ = buf.flush_now().await;
        let _ = buf.flush_now().await;
        // Subscribe just before the success flush — see the comment in
        // `flush_failure_then_success_persists_in_order` for why we don't
        // subscribe at construction time.
        let mut sub = buf.subscribe();
        let flushed = buf.flush_now().await.unwrap();
        assert_eq!(flushed.len(), 2);
        assert_eq!(buf.pending().await, 0);

        let received = store_raw.received.lock().await;
        assert_eq!(received.len(), 3);
        // All three attempts carried the same two events.
        assert_eq!(received[0], received[1]);
        assert_eq!(received[1], received[2]);
        drop(received);

        // Final Flushed broadcast reflects the successful third flush.
        let evt = tokio::time::timeout(Duration::from_millis(200), sub.recv())
            .await
            .expect("Flushed event should be buffered after eventual success")
            .unwrap();
        assert_eq!(unwrap_flushed(evt), 2);
    }
}
