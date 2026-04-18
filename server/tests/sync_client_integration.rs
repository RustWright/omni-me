// Integration tests for SyncClient — exercises the client-side orchestration:
// get_last_sync_timestamp, pull + apply, update_timestamp, get_since_by_device, push.
// Complements sync_integration.rs (which hits the server endpoints directly).

mod common;

use chrono::{DateTime, Utc};
use omni_me_core::events::{EventStore, NewEvent, SurrealEventStore};
use omni_me_core::sync::{PullRequest, PullResponse, PushRequest, SyncClient};

use common::{device_db, start_server};

fn sample_event(device_id: &str, aggregate_id: &str) -> NewEvent {
    NewEvent {
        id: None,
        event_type: "note_created".into(),
        aggregate_id: aggregate_id.into(),
        timestamp: Utc::now(),
        device_id: device_id.into(),
        payload: serde_json::json!({
            "raw_text": "sync client test",
            "date": "2026-04-18"
        }),
    }
}

/// Basic push flow: SyncClient::sync() should push local events to the server
/// and report accurate pulled/pushed counts.
#[tokio::test]
async fn sync_pushes_local_events_and_reports_counts() {
    let (url, _h) = start_server().await;
    let local = device_db().await;
    let store = SurrealEventStore::new(local.clone());

    store
        .append(sample_event("device-a", "note-1"))
        .await
        .unwrap();
    store
        .append(sample_event("device-a", "note-2"))
        .await
        .unwrap();

    let client = SyncClient::new(url, "device-a".into());
    let result = client.sync(&local).await.unwrap();

    assert_eq!(result.pulled, 0, "empty server has nothing to pull");
    assert_eq!(result.pushed, 2, "both local events should be pushed");
}

/// Idempotency: a second sync with no new activity must be a no-op.
/// This only passes if `sync_state.last_sync_timestamp` was persisted and
/// parsed back correctly across the two calls.
#[tokio::test]
async fn sync_is_idempotent_when_nothing_changed() {
    let (url, _h) = start_server().await;
    let local = device_db().await;
    let store = SurrealEventStore::new(local.clone());

    store
        .append(sample_event("device-a", "note-1"))
        .await
        .unwrap();

    let client = SyncClient::new(url, "device-a".into());
    let first = client.sync(&local).await.unwrap();
    assert_eq!(first.pushed, 1);
    assert_eq!(first.pulled, 0);

    let second = client.sync(&local).await.unwrap();
    assert_eq!(second.pulled, 0, "no remote events since last sync");
    assert_eq!(
        second.pushed, 0,
        "no new local events — sync_state must have persisted"
    );
}

/// Pull side: when the server holds events from another device, SyncClient
/// should fetch them and write them into the local store with preserved IDs.
#[tokio::test]
async fn sync_pulls_remote_events_into_local_store() {
    let (url, _h) = start_server().await;
    let http = reqwest::Client::new();

    http.post(format!("{url}/sync/push"))
        .json(&PushRequest {
            device_id: "device-b".into(),
            events: vec![sample_event("device-b", "note-remote")],
        })
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let local = device_db().await;
    let client = SyncClient::new(url, "device-a".into());
    let result = client.sync(&local).await.unwrap();

    assert_eq!(result.pulled, 1, "should pull device-b's event");
    assert_eq!(result.pushed, 0, "device-a has nothing to push");

    let store = SurrealEventStore::new(local);
    let local_events = store.get_by_aggregate("note-remote").await.unwrap();
    assert_eq!(local_events.len(), 1);
    assert_eq!(local_events[0].device_id, "device-b");
    assert_eq!(local_events[0].event_type, "note_created");
}

/// Device filter on push: events pulled from other devices must NOT be
/// re-pushed back to the server on the next sync. Without this guarantee,
/// every sync would loop every event through every device.
#[tokio::test]
async fn sync_does_not_re_push_pulled_events() {
    let (url, _h) = start_server().await;
    let http = reqwest::Client::new();

    // Initial empty sync on device-a to establish its last_sync_timestamp.
    let local = device_db().await;
    let client = SyncClient::new(url.clone(), "device-a".into());
    let initial = client.sync(&local).await.unwrap();
    assert_eq!(initial.pushed, 0);
    assert_eq!(initial.pulled, 0);

    // Device B publishes an event via raw HTTP AFTER device-a's initial sync —
    // so this event will be pulled into device-a's local store on the next sync.
    http.post(format!("{url}/sync/push"))
        .json(&PushRequest {
            device_id: "device-b".into(),
            events: vec![sample_event("device-b", "note-from-b")],
        })
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // Device A creates one of its own events locally.
    let store = SurrealEventStore::new(local.clone());
    store
        .append(sample_event("device-a", "note-from-a"))
        .await
        .unwrap();

    // Second sync: pulls device-b's event, then pushes — but push must
    // ONLY contain device-a's own event, not the freshly pulled device-b one.
    let result = client.sync(&local).await.unwrap();
    assert_eq!(result.pulled, 1, "pulled device-b's event");
    assert_eq!(result.pushed, 1, "pushed device-a's event");

    let epoch = DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let pull_resp: PullResponse = http
        .post(format!("{url}/sync/pull"))
        .json(&PullRequest {
            device_id: "device-c".into(),
            since: epoch,
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(
        pull_resp.events.len(),
        2,
        "Only 2 events should exist on server"
    );
    assert!(
        pull_resp
            .events
            .iter()
            .any(|event| event.device_id == "device-a")
    );
    assert!(
        pull_resp
            .events
            .iter()
            .any(|event| event.device_id == "device-b")
    );
}
