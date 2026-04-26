//! Retry accelerator — wires [`NetworkMonitor`] events into [`RetryEngine`]
//! hints.
//!
//! When the OS reports the network coming back online, forward the hint to
//! the retry engine so it can shorten its next wait. Hints are advisory: the
//! engine still follows its own backoff schedule (see `retry.rs` docs).

use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use super::network::{NetworkEvent, NetworkMonitor};
use super::retry::RetryEngine;

/// Spawn a task that forwards `NetworkEvent::Online` from `monitor` to
/// `engine.hint()`. The task exits when the monitor's broadcast channel
/// closes.
pub fn wire(monitor: &NetworkMonitor, engine: RetryEngine) -> JoinHandle<()> {
    let rx = monitor.subscribe();
    tokio::spawn(forward_loop(rx, engine))
}

async fn forward_loop(mut rx: broadcast::Receiver<NetworkEvent>, engine: RetryEngine) {
    loop {
        match rx.recv().await {
            Ok(NetworkEvent::Online) => engine.hint(),
            Ok(NetworkEvent::Offline) => {}
            Err(broadcast::error::RecvError::Closed) => return,
            Err(broadcast::error::RecvError::Lagged(_)) => {
                // Lag = we may have missed an Online; hint to be safe.
                engine.hint();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::buffer::SyncBuffer;
    use super::super::client::SyncClient;
    use super::super::pusher::PushDebouncer;
    use super::super::retry::{RetryEngine, RetryEvent};
    use crate::events::{EventStore, NewEvent, SurrealEventStore};
    use chrono::Utc;
    use std::sync::Arc;
    use std::time::Duration;

    async fn test_db() -> crate::db::Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accel.db");
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
                "raw_text": "accel test"
            }),
        }
    }

    /// When the monitor fires an Online event, the retry engine should
    /// receive a hint and emit HintReceived.
    #[tokio::test]
    async fn online_event_hints_retry_engine() {
        // Set up pieces.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        store
            .append(sample_event("device-x", "note-accel"))
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
            Duration::from_secs(10),
            Duration::from_secs(60),
        );

        // Bind a listener so monitor sees Online quickly.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let _accept = tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });

        let (monitor, _mh) = NetworkMonitor::spawn_with(
            addr,
            Duration::from_secs(60),
            Duration::from_millis(500),
        );

        // Wire accelerator.
        let _wh = wire(&monitor, retry.clone());

        let mut retry_sub = retry.subscribe();

        // Trigger a push failure first so retry engine enters its sleep.
        pusher.trigger();

        // Wait for Scheduled — then the Online event (from initial probe)
        // should hint.
        let mut saw_hint = false;
        for _ in 0..20 {
            let ev = tokio::time::timeout(Duration::from_millis(2000), retry_sub.recv())
                .await
                .expect("retry event")
                .unwrap();
            if matches!(ev, RetryEvent::HintReceived) {
                saw_hint = true;
                break;
            }
        }
        assert!(saw_hint, "HintReceived should fire after monitor reports Online");
        retry.shutdown();
        pusher.shutdown();
        monitor.shutdown();
    }
}
