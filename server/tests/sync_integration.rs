use std::sync::Arc;

use axum::{Router, routing::get, Json};
use chrono::{DateTime, Utc};
use omni_me_core::db;
use omni_me_core::events::{Event, EventStore, NewEvent, SurrealEventStore};
use omni_me_server::{AppState, routes};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

/// Spin up a real Axum server on a random port with its own temp SurrealDB.
/// Returns (server_url, join_handle).
async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.db");
    let server_db = db::connect(path.to_str().unwrap()).await.unwrap();
    // Leak the tempdir so it lives for the duration of the test
    std::mem::forget(dir);

    let state = AppState {
        db: Arc::new(server_db),
    };

    let app = Router::new()
        .route("/health", get(health))
        .merge(routes::sync_routes())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (url, handle)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Create a temp SurrealDB instance (simulates a device's local DB).
async fn device_db() -> db::Database {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device.db");
    let db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);
    db
}

// --- Sync protocol types (matching server routes) ---

#[derive(Debug, Serialize)]
struct PushRequest {
    device_id: String,
    events: Vec<NewEvent>,
}

#[derive(Debug, Deserialize)]
struct PushResponse {
    count: usize,
}

#[derive(Debug, Serialize)]
struct PullRequest {
    device_id: String,
    since: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct PullResponse {
    events: Vec<Event>,
    #[allow(dead_code)]
    sync_timestamp: DateTime<Utc>,
}

#[tokio::test]
async fn device_a_pushes_device_b_pulls() {
    let (server_url, _handle) = start_server().await;
    let http = reqwest::Client::new();

    let device_a_db = device_db().await;
    let device_b_db = device_db().await;
    let store_a = SurrealEventStore::new(device_a_db.clone());

    // Device A creates a note event locally
    let event = store_a
        .append(NewEvent {
            id: None,
            event_type: "note_created".into(),
            aggregate_id: "note-sync-1".into(),
            timestamp: Utc::now(),
            device_id: "device-a".into(),
            payload: serde_json::json!({
                "raw_text": "Synced note from device A",
                "date": "2026-03-27"
            }),
        })
        .await
        .unwrap();

    // Device A pushes to server
    let push_resp: PushResponse = http
        .post(format!("{server_url}/sync/push"))
        .json(&PushRequest {
            device_id: "device-a".into(),
            events: vec![NewEvent {
                id: Some(event.id.clone()),
                event_type: event.event_type.clone(),
                aggregate_id: event.aggregate_id.clone(),
                timestamp: event.timestamp,
                device_id: event.device_id.clone(),
                payload: event.payload.clone(),
            }],
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(push_resp.count, 1);

    // Device B pulls from server
    let epoch = DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let pull_resp: PullResponse = http
        .post(format!("{server_url}/sync/pull"))
        .json(&PullRequest {
            device_id: "device-b".into(),
            since: epoch,
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // Device B should see device A's event
    assert_eq!(pull_resp.events.len(), 1);
    assert_eq!(pull_resp.events[0].event_type, "note_created");
    assert_eq!(pull_resp.events[0].aggregate_id, "note-sync-1");
    assert_eq!(pull_resp.events[0].device_id, "device-a");

    // Device B stores the pulled event locally
    let store_b = SurrealEventStore::new(device_b_db.clone());
    for pulled_event in &pull_resp.events {
        store_b
            .append(NewEvent {
                id: Some(pulled_event.id.clone()),
                event_type: pulled_event.event_type.clone(),
                aggregate_id: pulled_event.aggregate_id.clone(),
                timestamp: pulled_event.timestamp,
                device_id: pulled_event.device_id.clone(),
                payload: pulled_event.payload.clone(),
            })
            .await
            .unwrap();
    }

    // Verify device B has the event
    let b_events = store_b.get_by_aggregate("note-sync-1").await.unwrap();
    assert_eq!(b_events.len(), 1);
    assert_eq!(b_events[0].event_type, "note_created");
}

#[tokio::test]
async fn concurrent_events_sync_both_devices() {
    let (server_url, _handle) = start_server().await;
    let http = reqwest::Client::new();

    let device_a_db = device_db().await;
    let device_b_db = device_db().await;
    let store_a = SurrealEventStore::new(device_a_db.clone());
    let store_b = SurrealEventStore::new(device_b_db.clone());

    // Device A creates an event
    let event_a = store_a
        .append(NewEvent {
            id: None,
            event_type: "note_created".into(),
            aggregate_id: "note-a".into(),
            timestamp: Utc::now(),
            device_id: "device-a".into(),
            payload: serde_json::json!({"raw_text": "From A", "date": "2026-03-27"}),
        })
        .await
        .unwrap();

    // Device B creates an event
    let event_b = store_b
        .append(NewEvent {
            id: None,
            event_type: "routine_group_created".into(),
            aggregate_id: "routine-b".into(),
            timestamp: Utc::now(),
            device_id: "device-b".into(),
            payload: serde_json::json!({
                "name": "Morning",
                "frequency": "daily",
                "time_of_day": "morning"
            }),
        })
        .await
        .unwrap();

    // Both push to server
    let push_a: PushResponse = http
        .post(format!("{server_url}/sync/push"))
        .json(&PushRequest {
            device_id: "device-a".into(),
            events: vec![NewEvent {
                id: Some(event_a.id.clone()),
                event_type: event_a.event_type.clone(),
                aggregate_id: event_a.aggregate_id.clone(),
                timestamp: event_a.timestamp,
                device_id: event_a.device_id.clone(),
                payload: event_a.payload.clone(),
            }],
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(push_a.count, 1);

    let push_b: PushResponse = http
        .post(format!("{server_url}/sync/push"))
        .json(&PushRequest {
            device_id: "device-b".into(),
            events: vec![NewEvent {
                id: Some(event_b.id.clone()),
                event_type: event_b.event_type.clone(),
                aggregate_id: event_b.aggregate_id.clone(),
                timestamp: event_b.timestamp,
                device_id: event_b.device_id.clone(),
                payload: event_b.payload.clone(),
            }],
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(push_b.count, 1);

    let epoch = DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    // Device A pulls (should get device B's event, not its own)
    let pull_a: PullResponse = http
        .post(format!("{server_url}/sync/pull"))
        .json(&PullRequest {
            device_id: "device-a".into(),
            since: epoch,
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(pull_a.events.len(), 1);
    assert_eq!(pull_a.events[0].device_id, "device-b");

    // Device B pulls (should get device A's event, not its own)
    let pull_b: PullResponse = http
        .post(format!("{server_url}/sync/pull"))
        .json(&PullRequest {
            device_id: "device-b".into(),
            since: epoch,
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(pull_b.events.len(), 1);
    assert_eq!(pull_b.events[0].device_id, "device-a");

    // After syncing, both devices should have all events locally
    // Device A stores pulled events, preserving server IDs
    for e in &pull_a.events {
        store_a
            .append(NewEvent {
                id: Some(e.id.clone()),
                event_type: e.event_type.clone(),
                aggregate_id: e.aggregate_id.clone(),
                timestamp: e.timestamp,
                device_id: e.device_id.clone(),
                payload: e.payload.clone(),
            })
            .await
            .unwrap();
    }

    // Device B stores pulled events, preserving server IDs
    for e in &pull_b.events {
        store_b
            .append(NewEvent {
                id: Some(e.id.clone()),
                event_type: e.event_type.clone(),
                aggregate_id: e.aggregate_id.clone(),
                timestamp: e.timestamp,
                device_id: e.device_id.clone(),
                payload: e.payload.clone(),
            })
            .await
            .unwrap();
    }

    // Both should now have 2 events each
    let a_all = store_a.get_since(epoch, None).await.unwrap();
    let b_all = store_b.get_since(epoch, None).await.unwrap();
    assert_eq!(a_all.len(), 2);
    assert_eq!(b_all.len(), 2);
}
