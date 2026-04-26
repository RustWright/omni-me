//! Phase 2 end-to-end sync test — Task 2.7.
//!
//! Scenario: kill the server, user creates a journal entry, the event is
//! buffered locally + a push is attempted + push fails + retry engine queues
//! another attempt; start a fresh server on the same port, hint the retry
//! engine, and verify the event reaches the server.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::{Router, Json, routing::get};
use chrono::Utc;
use omni_me_core::db;
use omni_me_core::events::{EventStore, NewEvent, SurrealEventStore};
use omni_me_core::llm::GeminiClient;
use omni_me_core::sync::{
    NetworkMonitor, PushDebouncer, PushEvent, RetryEngine, RetryEvent, StatusReporter, SyncBuffer,
    SyncClient, SyncStatus, wire_accelerator,
};
use omni_me_server::{AppState, routes};
use tower_http::cors::CorsLayer;

use common::device_db;

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Build the standard server router around a given AppState.
fn make_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(routes::sync_routes())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start an Axum server on a specific port. Returns the join handle so the
/// caller can abort it to simulate the server dying. The DB is leaked so
/// the server can be restarted on the same port with the same state.
async fn start_server_on_port(
    port: u16,
    server_db: omni_me_core::db::Database,
) -> tokio::task::JoinHandle<()> {
    let state = AppState {
        db: Arc::new(server_db),
        llm_client: Arc::new(GeminiClient::new("test-key-unused".into())),
    };

    let app = make_router(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .expect("failed to bind on specific port");

    tokio::spawn(async move {
        // Ignore the result — expected to be cancelled when we abort this task.
        let _ = axum::serve(listener, app).await;
    })
}

/// Start an Axum server on an ephemeral port. Returns (port, server_db,
/// join_handle). The server_db is held by the caller so a restart on the
/// same port can reuse state.
async fn start_server_ephemeral() -> (u16, omni_me_core::db::Database, tokio::task::JoinHandle<()>)
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.db");
    let server_db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);

    // Bind port 0 then discover the port so we can restart on it.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let port = listener.local_addr().unwrap().port();

    let state = AppState {
        db: Arc::new(server_db.clone()),
        llm_client: Arc::new(GeminiClient::new("test-key-unused".into())),
    };
    let app = make_router(state);

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (port, server_db, handle)
}

/// Full Phase 2 round-trip:
/// 1. Start server + client pipeline + monitor.
/// 2. Kill server.
/// 3. Append event through the buffer.
/// 4. Verify buffer flush + push fails + retry engine schedules.
/// 5. Restart server on same port.
/// 6. Hint the retry engine (simulating network recovery) + verify Recovered.
/// 7. Verify the event landed on the server.
#[tokio::test]
async fn kill_server_edit_queue_retry_recover() {
    // Step 1: real server up.
    let (port, server_db, server_handle) = start_server_ephemeral().await;
    let server_url = format!("http://127.0.0.1:{port}");
    let probe_target = format!("127.0.0.1:{port}");

    let local_db = device_db().await;
    let store = SurrealEventStore::new(local_db.clone());

    let client = SyncClient::new(server_url.clone(), "device-test".into());
    let (buffer, _buffer_h) =
        SyncBuffer::with_delay(Arc::new(store.clone()), Duration::from_millis(100));
    let (pusher, _pusher_h) = PushDebouncer::spawn_with_delay(
        client.clone(),
        local_db.clone(),
        &buffer,
        Duration::from_millis(150),
    );
    let (retry, _retry_h) = RetryEngine::spawn_with(
        client.clone(),
        local_db.clone(),
        &pusher,
        Duration::from_millis(200), // fast base so the test completes quickly
        Duration::from_secs(2),
    );
    let (monitor, _monitor_h) = NetworkMonitor::spawn_with(
        probe_target.clone(),
        Duration::from_millis(250),
        Duration::from_millis(200),
    );
    let _accel_h = wire_accelerator(&monitor, retry.clone());
    let (reporter, _t1, _t2) = StatusReporter::spawn(&pusher, &retry);

    // Starting status is Idle.
    assert_eq!(reporter.status().await, SyncStatus::Idle);

    // Step 2: Kill the server.
    server_handle.abort();
    // Give the OS a moment to release the socket.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Step 3: Append an event via the buffer — simulating a journal entry save.
    let event = NewEvent {
        id: None,
        event_type: "journal_entry_created".into(),
        aggregate_id: "journal-phase2-test".into(),
        timestamp: Utc::now(),
        device_id: "device-test".into(),
        payload: serde_json::json!({
            "journal_id": "journal-phase2-test",
            "date": "2026-04-19",
            "raw_text": "Phase 2 integration test"
        }),
    };

    let mut push_sub = pusher.subscribe();
    let mut retry_sub = retry.subscribe();

    buffer.append(event.clone()).await.unwrap();

    // Step 4: wait for buffer flush -> pusher trigger -> push Failed.
    let mut saw_push_failed = false;
    for _ in 0..30 {
        let ev = tokio::time::timeout(Duration::from_millis(3000), push_sub.recv())
            .await
            .expect("push event")
            .unwrap();
        if let PushEvent::Failed { .. } = ev {
            saw_push_failed = true;
            break;
        }
    }
    assert!(saw_push_failed, "push should fail against dead server");

    // Verify event is durably stored locally (buffer flushed to store).
    let local_events = store.get_by_aggregate("journal-phase2-test").await.unwrap();
    assert_eq!(
        local_events.len(),
        1,
        "event should be durably stored locally even while server is down"
    );

    // Status should now be Retrying or Error.
    let status_during_outage = reporter.status().await;
    assert!(
        status_during_outage == SyncStatus::Retrying || status_during_outage == SyncStatus::Error,
        "status during outage should be Retrying/Error, got {status_during_outage:?}"
    );

    // Wait for retry engine to schedule at least one retry.
    let mut saw_retry_scheduled = false;
    for _ in 0..30 {
        let ev = tokio::time::timeout(Duration::from_millis(3000), retry_sub.recv())
            .await
            .expect("retry event")
            .unwrap();
        if let RetryEvent::Scheduled { attempt, .. } = ev {
            assert!(attempt >= 1);
            saw_retry_scheduled = true;
            break;
        }
    }
    assert!(saw_retry_scheduled, "retry should schedule at least once");

    // Step 5: restart server on same port with the SAME db so test observes
    // the pushed event.
    let _new_server_handle = start_server_on_port(port, server_db.clone()).await;
    // Give the new server a moment to come up.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Step 6: Nudge monitor + hint retry engine (accelerator should handle
    // this, but we also hint directly to avoid racing the 250ms probe window).
    monitor.probe_now().await;
    retry.hint();

    // Step 7: wait for RetryEvent::Recovered.
    let mut saw_recovered = false;
    for _ in 0..50 {
        let ev = tokio::time::timeout(Duration::from_millis(5000), retry_sub.recv())
            .await
            .expect("retry event")
            .unwrap();
        if let RetryEvent::Recovered { pushed, .. } = ev {
            assert_eq!(pushed, 1, "should push the one queued event");
            saw_recovered = true;
            break;
        }
    }
    assert!(saw_recovered, "retry engine should report Recovered once server is back");

    // Status should return to Idle.
    // Allow a short moment for the status reporter to observe the recovery.
    for _ in 0..20 {
        let s = reporter.status().await;
        if s == SyncStatus::Idle {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(reporter.status().await, SyncStatus::Idle);

    // Verify the event landed on the server's store.
    let server_store = SurrealEventStore::new(server_db);
    let server_events = server_store
        .get_by_aggregate("journal-phase2-test")
        .await
        .unwrap();
    assert_eq!(server_events.len(), 1, "event should be on the server after recovery");
    assert_eq!(server_events[0].device_id, "device-test");

    retry.shutdown();
    pusher.shutdown();
    monitor.shutdown();
    reporter.shutdown();
}
