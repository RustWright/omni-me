//! Aggregated sync status reporter.
//!
//! Watches [`PushEvent`](super::pusher::PushEvent) and
//! [`RetryEvent`](super::retry::RetryEvent) broadcasts and exposes a single
//! 4-state status value consumers can poll:
//!
//! - `Idle` — no push in flight, retry engine dormant
//! - `Syncing` — push in flight
//! - `Retrying` — push has failed and the retry engine is backing off
//! - `Error` — retry has failed so many times backoff has reached the 60s cap
//!
//! The status is exposed via the `get_sync_status` Tauri command, which
//! serializes the enum as kebab-case strings for easy JS consumption.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;

use super::pusher::{PushDebouncer, PushEvent};
use super::retry::{RetryEngine, RetryEvent, DEFAULT_RETRY_CAP};

/// Four aggregate states the UI can display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncStatus {
    Idle,
    Syncing,
    Retrying,
    Error,
}

impl SyncStatus {
    /// Stable kebab-case string representation for API surfaces.
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncStatus::Idle => "idle",
            SyncStatus::Syncing => "syncing",
            SyncStatus::Retrying => "retrying",
            SyncStatus::Error => "error",
        }
    }
}

/// Detailed status snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct SyncStatusSnapshot {
    pub status: SyncStatus,
    /// Retry attempt counter (0 = not currently retrying).
    pub retry_attempt: u32,
    /// Last error message, if any. Cleared on recovery.
    pub last_error: Option<String>,
}

/// Status reporter. Cloneable handle; background tasks own state.
#[derive(Clone)]
pub struct StatusReporter {
    inner: Arc<Inner>,
}

struct Inner {
    snapshot: Mutex<SyncStatusSnapshot>,
    shutdown: tokio::sync::Notify,
}

impl StatusReporter {
    /// Spawn a reporter wired to the given pusher and retry engine. Returns a
    /// handle plus two background tasks (join via the returned handles).
    pub fn spawn(
        pusher: &PushDebouncer,
        retry: &RetryEngine,
    ) -> (Self, JoinHandle<()>, JoinHandle<()>) {
        let inner = Arc::new(Inner {
            snapshot: Mutex::new(SyncStatusSnapshot {
                status: SyncStatus::Idle,
                retry_attempt: 0,
                last_error: None,
            }),
            shutdown: tokio::sync::Notify::new(),
        });

        let reporter = Self { inner: inner.clone() };
        let push_rx = pusher.subscribe();
        let retry_rx = retry.subscribe();

        let push_inner = inner.clone();
        let push_task = tokio::spawn(watch_push(push_rx, push_inner));

        let retry_inner = inner.clone();
        let retry_task = tokio::spawn(watch_retry(retry_rx, retry_inner));

        (reporter, push_task, retry_task)
    }

    /// Current status snapshot.
    pub async fn snapshot(&self) -> SyncStatusSnapshot {
        self.inner.snapshot.lock().await.clone()
    }

    /// Current status only.
    pub async fn status(&self) -> SyncStatus {
        self.inner.snapshot.lock().await.status
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.notify_one();
    }
}

async fn watch_push(mut rx: broadcast::Receiver<PushEvent>, inner: Arc<Inner>) {
    loop {
        tokio::select! {
            _ = inner.shutdown.notified() => return,
            ev = rx.recv() => {
                match ev {
                    Ok(PushEvent::Started) => {
                        let mut s = inner.snapshot.lock().await;
                        // Don't clobber Retrying/Error if a retry is underway —
                        // Started comes from the debouncer path, which only
                        // fires after a successful flush completion. If we're
                        // mid-retry, keep that signal.
                        if s.retry_attempt == 0 {
                            s.status = SyncStatus::Syncing;
                            s.last_error = None;
                        }
                    }
                    Ok(PushEvent::Succeeded { .. }) => {
                        let mut s = inner.snapshot.lock().await;
                        s.status = SyncStatus::Idle;
                        s.retry_attempt = 0;
                        s.last_error = None;
                    }
                    Ok(PushEvent::Failed { error }) => {
                        let mut s = inner.snapshot.lock().await;
                        // Retrying covers failures; Error is reserved for the
                        // cap state tracked by watch_retry.
                        if s.status != SyncStatus::Error {
                            s.status = SyncStatus::Retrying;
                        }
                        s.last_error = Some(error);
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }
}

async fn watch_retry(mut rx: broadcast::Receiver<RetryEvent>, inner: Arc<Inner>) {
    loop {
        tokio::select! {
            _ = inner.shutdown.notified() => return,
            ev = rx.recv() => {
                match ev {
                    Ok(RetryEvent::Scheduled { attempt, delay }) => {
                        let mut s = inner.snapshot.lock().await;
                        s.retry_attempt = attempt;
                        // Promote to Error once we're at or above the cap —
                        // user-visible signal that the network isn't coming
                        // back on its own.
                        if delay >= DEFAULT_RETRY_CAP.saturating_sub(std::time::Duration::from_millis(100)) {
                            s.status = SyncStatus::Error;
                        } else {
                            s.status = SyncStatus::Retrying;
                        }
                    }
                    Ok(RetryEvent::Attempting { attempt }) => {
                        let mut s = inner.snapshot.lock().await;
                        s.retry_attempt = attempt;
                        // Visible as Syncing when actually attempting, but only
                        // if we haven't been promoted to Error.
                        if s.status != SyncStatus::Error {
                            s.status = SyncStatus::Syncing;
                        }
                    }
                    Ok(RetryEvent::Recovered { .. }) | Ok(RetryEvent::Idle) => {
                        let mut s = inner.snapshot.lock().await;
                        s.status = SyncStatus::Idle;
                        s.retry_attempt = 0;
                        s.last_error = None;
                    }
                    Ok(RetryEvent::Failed { attempt, error }) => {
                        let mut s = inner.snapshot.lock().await;
                        s.retry_attempt = attempt;
                        s.last_error = Some(error);
                        // Stay in Retrying unless watch_retry's Scheduled
                        // path has promoted us to Error.
                        if s.status != SyncStatus::Error {
                            s.status = SyncStatus::Retrying;
                        }
                    }
                    Ok(RetryEvent::HintReceived) => {} // no status change
                    Err(broadcast::error::RecvError::Closed) => return,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::buffer::SyncBuffer;
    use super::super::client::SyncClient;
    use crate::events::{EventStore, NewEvent, SurrealEventStore};
    use chrono::Utc;
    use std::sync::Arc;
    use std::time::Duration;

    async fn test_db() -> crate::db::Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("status.db");
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
                "raw_text": "status test"
            }),
        }
    }

    #[test]
    fn sync_status_serializes_as_kebab_case() {
        let idle = serde_json::to_string(&SyncStatus::Idle).unwrap();
        let syncing = serde_json::to_string(&SyncStatus::Syncing).unwrap();
        let retrying = serde_json::to_string(&SyncStatus::Retrying).unwrap();
        let error = serde_json::to_string(&SyncStatus::Error).unwrap();
        assert_eq!(idle, "\"idle\"");
        assert_eq!(syncing, "\"syncing\"");
        assert_eq!(retrying, "\"retrying\"");
        assert_eq!(error, "\"error\"");
    }

    /// Starts Idle, transitions to Retrying on push failure.
    #[tokio::test]
    async fn failure_transitions_to_retrying() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        store
            .append(sample_event("device-x", "note-status"))
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
        let (reporter, _t1, _t2) = StatusReporter::spawn(&pusher, &retry);

        assert_eq!(reporter.status().await, SyncStatus::Idle);

        pusher.trigger();

        // Wait for status to transition. Allow up to 2s.
        let mut seen_retrying = false;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let s = reporter.snapshot().await;
            if s.status == SyncStatus::Retrying || s.status == SyncStatus::Error {
                seen_retrying = true;
                assert!(s.retry_attempt >= 1 || s.last_error.is_some());
                break;
            }
        }
        assert!(seen_retrying, "status should reach Retrying");

        reporter.shutdown();
        retry.shutdown();
        pusher.shutdown();
    }
}
