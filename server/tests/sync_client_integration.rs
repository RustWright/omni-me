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
        event_type: "journal_entry_created".into(),
        aggregate_id: aggregate_id.into(),
        timestamp: Utc::now(),
        device_id: device_id.into(),
        payload: serde_json::json!({
            "journal_id": aggregate_id,
            "date": "2026-04-18",
            "raw_text": "sync client test"
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
    assert_eq!(local_events[0].event_type, "journal_entry_created");
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

/// A device that was offline for days must have its backlog delivered to a peer
/// whose cursor is already *ahead* of those events' author timestamps.
///
/// End-to-end regression for the mixed-clock cursor. The pull cursor is issued
/// by the server, but the pull filter used to compare it against the authoring
/// device's clock, which `append_batch` preserves verbatim. So: device B's
/// cursor advances to "now" after a normal sync; device A comes back online and
/// pushes events it authored days ago; B's next pull asks for "everything after
/// now" and those events sit permanently below it. Not delayed — never
/// delivered, and silently, because nothing is aware a gap exists. Routine clock
/// skew between two devices reproduced it without anyone going offline.
///
/// Note the third device: B's cursor only moves when a pull actually returns
/// something, so without C there is no "cursor ahead" state and the test passes
/// against the broken code too. That is exactly what the first draft of this
/// test did.
///
/// This is also the roadmap step 7–8 shape (bring imports current → wipe box →
/// clean re-import), which is a bulk import of months-old dated data.
#[tokio::test]
async fn offline_backlog_reaches_a_peer_whose_cursor_is_already_ahead() {
    let (url, _h) = start_server().await;

    // Device C publishes something current, so B's cursor advances to ~now.
    let db_c = device_db().await;
    let store_c = SurrealEventStore::new(db_c.clone());
    store_c.append(sample_event("device-c", "current")).await.unwrap();
    SyncClient::new(url.clone(), "device-c".into())
        .sync(&db_c)
        .await
        .unwrap();

    let db_b = device_db().await;
    let client_b = SyncClient::new(url.clone(), "device-b".into());
    let warmup = client_b.sync(&db_b).await.unwrap();
    assert_eq!(warmup.pulled, 1, "B should pull C's event and move its cursor");

    // Device A pushes work it authored days ago while offline — all of it
    // stamped *before* B's cursor.
    let db_a = device_db().await;
    let store_a = SurrealEventStore::new(db_a.clone());
    for (i, days_ago) in [4_i64, 3, 2, 1].iter().enumerate() {
        let mut ev = sample_event("device-a", &format!("backlog-{i}"));
        ev.timestamp = Utc::now() - chrono::Duration::days(*days_ago);
        store_a.append(ev).await.unwrap();
    }
    let pushed = SyncClient::new(url.clone(), "device-a".into())
        .sync(&db_a)
        .await
        .unwrap();
    assert_eq!(pushed.pushed, 4, "device A should push its whole backlog");

    // B pulls again. Every backlogged event must arrive despite its author
    // timestamp predating B's cursor.
    let result = client_b.sync(&db_b).await.unwrap();
    assert_eq!(
        result.pulled, 4,
        "backlog authored before the peer's cursor was dropped — the mixed-clock bug"
    );

    let store_b = SurrealEventStore::new(db_b.clone());
    for i in 0..4 {
        let got = store_b
            .get_by_aggregate(&format!("backlog-{i}"))
            .await
            .unwrap();
        assert_eq!(got.len(), 1, "backlog-{i} missing on the peer");
    }

    // And a further sync with nothing new stays a no-op.
    let quiet = client_b.sync(&db_b).await.unwrap();
    assert_eq!(quiet.pulled, 0, "cursor failed to advance past the backlog");
}
