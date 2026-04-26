//! Exponential-backoff retry engine for sync pushes.
//!
//! Listens on a [`PushDebouncer`]'s outcome channel. When a push Fails, the
//! engine schedules a retry attempt at `base * 2^n` seconds (jittered ±10%,
//! capped at 60s). When a push Succeeds or the retry itself succeeds, the
//! backoff counter resets to zero.
//!
//! Retry attempts call `SyncClient::push_only` directly — they do not go
//! through the debouncer (which is driven by buffer flushes).
//!
//! External consumers can call [`RetryEngine::hint`] to signal that the
//! network might have come back (e.g. from Task 2.5's OS listener). Hints
//! nudge the sleep: if the scheduled retry is more than ~2s away, the engine
//! fires a retry now. The backoff counter is NOT reset — the hint shortens
//! wait time, not step count.

use std::sync::Arc;
use std::time::Duration;

use rand::Rng;
use tokio::sync::{Mutex, Notify, broadcast};
use tokio::task::JoinHandle;

use crate::db::Database;

use super::client::{PushOutcome, SyncClient, SyncError};
use super::pusher::{PushDebouncer, PushEvent};

/// Base backoff delay (first retry = 1s + jitter).
pub const DEFAULT_RETRY_BASE: Duration = Duration::from_secs(1);

/// Max backoff delay (±jitter applied before capping).
pub const DEFAULT_RETRY_CAP: Duration = Duration::from_secs(60);

/// Fraction of jitter to apply: 0.10 = ±10%.
const JITTER_FRACTION: f64 = 0.10;

/// Channel capacity for retry event broadcasts.
const RETRY_CHANNEL_CAPACITY: usize = 16;

/// Observable retry lifecycle event.
#[derive(Debug, Clone)]
pub enum RetryEvent {
    /// Retry scheduled at this attempt number with this delay.
    Scheduled { attempt: u32, delay: Duration },
    /// Retry attempt starting.
    Attempting { attempt: u32 },
    /// Retry succeeded — backoff counter reset.
    Recovered { attempt: u32, pushed: usize },
    /// Retry failed — will back off further (or hit cap).
    Failed { attempt: u32, error: String },
    /// Outside hint received (e.g. network came back) — engine may advance.
    HintReceived,
    /// Engine is idle (no retries pending).
    Idle,
}

/// Retry engine. Cloneable handle; background task owns state.
#[derive(Clone)]
pub struct RetryEngine {
    inner: Arc<Inner>,
}

struct Inner {
    client: SyncClient,
    db: Database,
    hint: Notify,
    shutdown: Notify,
    events: broadcast::Sender<RetryEvent>,
    /// Current consecutive failure count. 0 = idle / last attempt succeeded.
    attempt: Mutex<u32>,
    base: Duration,
    cap: Duration,
}

impl RetryEngine {
    /// Spawn a retry engine that watches `pusher` outcomes. Returns a handle
    /// plus the background task join handle.
    pub fn spawn(client: SyncClient, db: Database, pusher: &PushDebouncer) -> (Self, JoinHandle<()>) {
        Self::spawn_with(client, db, pusher, DEFAULT_RETRY_BASE, DEFAULT_RETRY_CAP)
    }

    pub fn spawn_with(
        client: SyncClient,
        db: Database,
        pusher: &PushDebouncer,
        base: Duration,
        cap: Duration,
    ) -> (Self, JoinHandle<()>) {
        let (events_tx, _rx) = broadcast::channel(RETRY_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            client,
            db,
            hint: Notify::new(),
            shutdown: Notify::new(),
            events: events_tx,
            attempt: Mutex::new(0),
            base,
            cap,
        });
        let engine = Self { inner: inner.clone() };
        let pusher_sub = pusher.subscribe();

        let handle = tokio::spawn(run_loop(inner, pusher_sub));
        (engine, handle)
    }

    /// Signal that external conditions (e.g. network) may have improved. If
    /// the engine is currently sleeping for a retry, it wakes immediately.
    /// Note: the attempt counter is NOT reset — only the wait is cut short.
    pub fn hint(&self) {
        self.inner.hint.notify_one();
    }

    /// Subscribe to retry events.
    pub fn subscribe(&self) -> broadcast::Receiver<RetryEvent> {
        self.inner.events.subscribe()
    }

    /// Current attempt counter (0 = idle).
    pub async fn current_attempt(&self) -> u32 {
        *self.inner.attempt.lock().await
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.notify_one();
    }
}

/// Compute backoff delay for an attempt (1-indexed). Attempt 1 => ~base,
/// attempt 2 => ~2*base, attempt 3 => ~4*base, etc, capped at `cap`. Applies
/// ±JITTER_FRACTION uniform jitter.
pub fn backoff_delay(attempt: u32, base: Duration, cap: Duration) -> Duration {
    // 2^(attempt-1) with saturation to avoid overflow.
    let exp = attempt.saturating_sub(1).min(31);
    let nominal_secs = (base.as_secs_f64()) * (1u64 << exp) as f64;
    let jitter_span = nominal_secs * JITTER_FRACTION;
    let mut rng = rand::thread_rng();
    let jitter = rng.gen_range(-jitter_span..=jitter_span);
    let total_secs = (nominal_secs + jitter).max(0.0);
    let dur = Duration::from_secs_f64(total_secs);
    dur.min(cap)
}

async fn run_loop(
    inner: Arc<Inner>,
    mut pusher_sub: broadcast::Receiver<PushEvent>,
) {
    let _ = inner.events.send(RetryEvent::Idle);

    loop {
        // Wait for something actionable: pusher outcome, shutdown, or hint
        // (hints are only meaningful when we have a pending retry).
        tokio::select! {
            _ = inner.shutdown.notified() => return,
            ev = pusher_sub.recv() => {
                match ev {
                    Ok(PushEvent::Succeeded { .. }) => {
                        reset_attempt(&inner).await;
                        let _ = inner.events.send(RetryEvent::Idle);
                        continue;
                    }
                    Ok(PushEvent::Failed { .. }) => {
                        // Drop into the retry loop below.
                        retry_until_success(&inner).await;
                    }
                    Ok(PushEvent::Started) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }
}

/// Back off + retry until a push succeeds or shutdown fires. On success, the
/// attempt counter is reset. This function returns to the outer select loop,
/// where further pusher events (including subsequent Failed) can trigger
/// another retry session.
async fn retry_until_success(inner: &Arc<Inner>) {
    loop {
        let attempt = {
            let mut a = inner.attempt.lock().await;
            *a = a.saturating_add(1);
            *a
        };

        let delay = backoff_delay(attempt, inner.base, inner.cap);
        let _ = inner.events.send(RetryEvent::Scheduled { attempt, delay });

        // Sleep with the ability to cut short on hint or shutdown.
        let sleep = tokio::time::sleep(delay);
        tokio::pin!(sleep);

        loop {
            tokio::select! {
                _ = inner.shutdown.notified() => return,
                _ = &mut sleep => break,
                _ = inner.hint.notified() => {
                    let _ = inner.events.send(RetryEvent::HintReceived);
                    // If the remaining sleep is more than ~2s, fire now.
                    // Otherwise let the sleep finish.
                    let remaining = sleep.as_mut().deadline() - tokio::time::Instant::now();
                    if remaining > Duration::from_millis(2000) {
                        break;
                    }
                }
            }
        }

        let _ = inner.events.send(RetryEvent::Attempting { attempt });

        let push_result = async {
            let since = inner.client.last_sync_timestamp(&inner.db).await?;
            inner.client.push_only(&inner.db, &since).await
        }.await;

        match push_result {
            Ok(PushOutcome { pushed }) => {
                reset_attempt(inner).await;
                let _ = inner.events.send(RetryEvent::Recovered { attempt, pushed });
                let _ = inner.events.send(RetryEvent::Idle);
                return;
            }
            Err(e) => {
                let msg = match &e {
                    SyncError::Network(m) => format!("network: {m}"),
                    SyncError::Server(m) => format!("server: {m}"),
                    SyncError::Local(m) => format!("local: {m}"),
                };
                let _ = inner.events.send(RetryEvent::Failed { attempt, error: msg });
                // Continue loop — next iteration backs off further.
            }
        }
    }
}

async fn reset_attempt(inner: &Inner) {
    let mut a = inner.attempt.lock().await;
    *a = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_follows_exponential_curve_within_jitter() {
        let base = Duration::from_millis(1000);
        let cap = Duration::from_secs(60);
        // Run many times — jitter must keep each attempt inside the expected
        // ±10% band around 2^(n-1) * base, capped at 60s.
        for attempt in 1..=10 {
            for _ in 0..50 {
                let d = backoff_delay(attempt, base, cap);
                let nominal = (1u64 << (attempt - 1).min(31)) as f64 * base.as_secs_f64();
                let nominal_capped = nominal.min(cap.as_secs_f64());
                let floor = (nominal_capped * 0.9).max(0.0);
                let ceil = (nominal_capped * 1.1).min(cap.as_secs_f64());
                let actual = d.as_secs_f64();
                assert!(
                    actual >= floor - 0.01 && actual <= ceil + 0.01,
                    "attempt {attempt}: got {actual}s, expected in [{floor}, {ceil}]"
                );
            }
        }
    }

    #[test]
    fn backoff_caps_at_60s() {
        // Any attempt far past the curve should be capped at 60s.
        let d = backoff_delay(20, DEFAULT_RETRY_BASE, DEFAULT_RETRY_CAP);
        assert!(d <= Duration::from_secs(60) + Duration::from_millis(1));
    }

    use crate::events::{EventStore, NewEvent, SurrealEventStore};
    use super::super::buffer::SyncBuffer;
    use chrono::Utc;
    use std::sync::Arc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("retry.db");
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
                "raw_text": "retry test"
            }),
        }
    }

    /// On push failure, the retry engine schedules at least one attempt.
    #[tokio::test]
    async fn schedules_retry_on_failure() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        store
            .append(sample_event("device-x", "note-r"))
            .await
            .unwrap();

        let client = SyncClient::new("http://127.0.0.1:1".into(), "device-x".into());
        let (buffer, _bh) = SyncBuffer::with_delay(Arc::new(store), Duration::from_secs(60));
        let (pusher, _ph) = PushDebouncer::spawn_with_delay(
            client.clone(),
            db.clone(),
            &buffer,
            Duration::from_millis(30),
        );
        let (retry, _rh) = RetryEngine::spawn_with(
            client,
            db,
            &pusher,
            Duration::from_millis(50),
            Duration::from_millis(500),
        );
        let mut sub = retry.subscribe();

        pusher.trigger();

        let mut saw_scheduled = false;
        for _ in 0..10 {
            let ev = tokio::time::timeout(Duration::from_millis(2000), sub.recv())
                .await
                .expect("retry event")
                .unwrap();
            if let RetryEvent::Scheduled { attempt, .. } = ev {
                assert!(attempt >= 1);
                saw_scheduled = true;
                break;
            }
        }
        assert!(saw_scheduled, "RetryEvent::Scheduled never arrived");
        retry.shutdown();
        pusher.shutdown();
    }

    /// A hint while retry is sleeping should cut the wait short (when the
    /// remaining time > 2s).
    #[tokio::test]
    async fn hint_cuts_long_sleep() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        store
            .append(sample_event("device-x", "note-hint"))
            .await
            .unwrap();

        // Use a big base so the very first attempt has a sleep > 2s.
        let client = SyncClient::new("http://127.0.0.1:1".into(), "device-x".into());
        let (buffer, _bh) = SyncBuffer::with_delay(Arc::new(store), Duration::from_secs(60));
        let (pusher, _ph) = PushDebouncer::spawn_with_delay(
            client.clone(),
            db.clone(),
            &buffer,
            Duration::from_millis(30),
        );
        let (retry, _rh) = RetryEngine::spawn_with(
            client,
            db,
            &pusher,
            Duration::from_secs(10), // huge base
            Duration::from_secs(60),
        );
        let mut sub = retry.subscribe();

        pusher.trigger();

        // Wait until the retry engine emits Scheduled (means it's now
        // sleeping).
        loop {
            let ev = tokio::time::timeout(Duration::from_millis(2000), sub.recv())
                .await
                .expect("should get events");
            if matches!(ev, Ok(RetryEvent::Scheduled { .. })) {
                break;
            }
        }

        // Hint — engine should cut sleep, emit HintReceived, and re-attempt.
        let t0 = std::time::Instant::now();
        retry.hint();

        let mut saw_hint = false;
        let mut saw_attempt = false;
        for _ in 0..10 {
            let ev = tokio::time::timeout(Duration::from_millis(3000), sub.recv())
                .await
                .expect("event after hint")
                .unwrap();
            match ev {
                RetryEvent::HintReceived => saw_hint = true,
                RetryEvent::Attempting { .. } => {
                    saw_attempt = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(saw_hint, "HintReceived should fire");
        assert!(saw_attempt, "hint should trigger a retry attempt");
        // Must have elapsed much less than base (10s).
        assert!(t0.elapsed() < Duration::from_secs(3));

        retry.shutdown();
        pusher.shutdown();
    }
}
