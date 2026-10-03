//! `POST /wipe/preview` and `POST /wipe/confirm` end to end.
//!
//! The engine's tests prove what a scoped wipe does to a log. What they cannot
//! prove is the pair of gates, and the gates are where the irreversible part
//! lives: that a preview's ticket authorises exactly one confirm, that a confirm
//! may narrow the previewed features and never widen them, and that a wipe
//! addressed to the wrong deployment is refused before anything is deleted.
//!
//! ⚠️ Every assertion goes over a real socket against the production router,
//! because the ticket and the declared instance live in server state — an
//! in-process call to `purge_features` reaches neither.

mod common;

use omni_me_core::db;
use omni_me_core::events::{EventStore, NewEvent, SurrealEventStore};

/// Put one event of each named type straight into the server's own log.
async fn seed(db: &db::Database, types: &[&str]) {
    let store = SurrealEventStore::new(db.clone());
    for (i, event_type) in types.iter().enumerate() {
        store
            .append(NewEvent {
                id: None,
                event_type: (*event_type).to_string(),
                aggregate_id: format!("a{i}"),
                timestamp: chrono::Utc::now(),
                device_id: "test-device".to_string(),
                payload: serde_json::json!({}),
            })
            .await
            .expect("seed");
    }
}

async fn event_types(db: &db::Database) -> Vec<String> {
    let store = SurrealEventStore::new(db.clone());
    let mut found: Vec<String> = store
        .get_since(chrono::Utc::now() - chrono::Duration::hours(1), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    found.sort();
    found
}

/// The separation, over the wire and end to end: the ledger goes, the rest stays,
/// and the wipe leaves its own record behind.
#[tokio::test]
async fn a_previewed_wipe_takes_the_ledger_and_leaves_everything_else() {
    let (url, db, _h) = common::start_dev_server().await;
    seed(
        &db,
        &[
            "transaction_recorded",
            "transaction_categorized",
            "journal_entry_created",
            "document_archived",
            "config_set",
        ],
    )
    .await;
    let client = reqwest::Client::new();

    let preview: serde_json::Value = client
        .post(format!("{url}/wipe/preview"))
        .json(&serde_json::json!({ "features": ["finances"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(preview["total"], 2);
    assert_eq!(preview["events_by_type"]["transaction_recorded"], 1);
    assert_eq!(preview["events_by_type"]["transaction_categorized"], 1);
    // ⛔ The preview writes nothing. A person who looks and walks away has an
    // unchanged log.
    assert_eq!(event_types(&db).await.len(), 5);
    assert_eq!(preview["instance"], "dev");

    let report: serde_json::Value = client
        .post(format!("{url}/wipe/confirm"))
        .json(&serde_json::json!({
            "token": preview["token"],
            "features": ["finances"],
            "instance": "dev",
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(report["events_removed"], 2);
    assert_eq!(report["projections_rebuilt"], true);
    // The snapshot was taken first, and it still holds what the wipe took.
    let snapshot = std::fs::read_to_string(report["snapshot"].as_str().unwrap()).unwrap();
    assert!(
        snapshot.contains("transaction_recorded"),
        "the wiped events are in the snapshot"
    );
    std::fs::remove_file(report["snapshot"].as_str().unwrap()).ok();

    let left = event_types(&db).await;
    // The three survivors, plus the wipe's own audit record.
    assert_eq!(
        left,
        vec![
            "config_set",
            "data_wiped",
            "document_archived",
            "journal_entry_created",
        ],
        "a finances wipe must take the ledger and nothing else"
    );
}

/// A ticket authorises one confirm. The second is a 409, not a second wipe.
#[tokio::test]
async fn a_wipe_ticket_cannot_be_replayed() {
    let (url, db, _h) = common::start_dev_server().await;
    seed(&db, &["transaction_recorded"]).await;
    let client = reqwest::Client::new();

    let preview: serde_json::Value = client
        .post(format!("{url}/wipe/preview"))
        .json(&serde_json::json!({ "features": ["finances"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let confirm = serde_json::json!({
        "token": preview["token"],
        "features": ["finances"],
        "instance": "dev",
    });

    let first = client
        .post(format!("{url}/wipe/confirm"))
        .json(&confirm)
        .send()
        .await
        .unwrap();
    assert!(first.status().is_success());

    let replay = client
        .post(format!("{url}/wipe/confirm"))
        .json(&confirm)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), reqwest::StatusCode::CONFLICT);
}

/// ⛔ A confirm may shrink what was previewed and never widen it. Previewing
/// finances and confirming finances *plus auto-import* would destroy events
/// nobody counted.
#[tokio::test]
async fn a_confirm_may_narrow_the_previewed_features_but_never_widen_them() {
    let (url, db, _h) = common::start_dev_server().await;
    seed(
        &db,
        &["transaction_recorded", "auto_import_batch_committed"],
    )
    .await;
    let client = reqwest::Client::new();

    let preview: serde_json::Value = client
        .post(format!("{url}/wipe/preview"))
        .json(&serde_json::json!({ "features": ["finances"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let widened = client
        .post(format!("{url}/wipe/confirm"))
        .json(&serde_json::json!({
            "token": preview["token"],
            "features": ["finances", "auto_import"],
            "instance": "dev",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(widened.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = widened.text().await.unwrap();
    assert!(body.contains("auto_import"), "{body}");

    // And nothing was taken on the way to refusing.
    assert_eq!(event_types(&db).await.len(), 2);
}

/// A wipe addressed to another deployment is refused before anything is deleted.
/// This is the guard against a shell pointed at the wrong host.
#[tokio::test]
async fn a_wipe_addressed_to_production_is_refused_by_the_dev_server() {
    let (url, db, _h) = common::start_dev_server().await;
    seed(&db, &["transaction_recorded"]).await;
    let client = reqwest::Client::new();

    let preview: serde_json::Value = client
        .post(format!("{url}/wipe/preview"))
        .json(&serde_json::json!({ "features": ["finances"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let wrong = client
        .post(format!("{url}/wipe/confirm"))
        .json(&serde_json::json!({
            "token": preview["token"],
            "features": ["finances"],
            "instance": "production",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), reqwest::StatusCode::CONFLICT);
    assert_eq!(event_types(&db).await, vec!["transaction_recorded"]);
}

/// ⛔ Documents are refused through this route: a document event names a blob, and
/// only the purge path's refcount knows whether anything else still points at it.
#[tokio::test]
async fn documents_are_refused_and_the_route_says_why() {
    let (url, db, _h) = common::start_dev_server().await;
    seed(&db, &["document_archived"]).await;
    let client = reqwest::Client::new();

    let listed: serde_json::Value = client
        .get(format!("{url}/wipe/features"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let documents = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["feature"] == "documents")
        .expect("documents must be listed, with its refusal");
    assert_eq!(documents["refused"], true);
    assert!(
        documents["refused_because"]
            .as_str()
            .unwrap()
            .contains("purge"),
        "the refusal has to name what to use instead"
    );

    let preview: serde_json::Value = client
        .post(format!("{url}/wipe/preview"))
        .json(&serde_json::json!({ "features": ["documents"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // ⚠️ The preview is allowed: counting is not destroying, and seeing the number
    // is how someone learns the refusal applies to something real.
    assert_eq!(preview["total"], 1);

    let refused = client
        .post(format!("{url}/wipe/confirm"))
        .json(&serde_json::json!({
            "token": preview["token"],
            "features": ["documents"],
            "instance": "dev",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(event_types(&db).await, vec!["document_archived"]);
}

/// An unknown feature name lists the ones that exist rather than wiping nothing
/// and reporting success.
#[tokio::test]
async fn an_unknown_feature_is_a_bad_request_naming_the_known_ones() {
    let (url, _db, _h) = common::start_dev_server().await;
    let resp = reqwest::Client::new()
        .post(format!("{url}/wipe/preview"))
        .json(&serde_json::json!({ "features": ["ledger"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(resp.text().await.unwrap().contains("finances"));
}

/// A device follows a server's wipe when it pulls the record (user, 2026-10-03):
/// its pre-wipe finance events go, and its journal and its own post-wipe work stay.
#[tokio::test]
async fn a_device_that_pulls_the_wipe_clears_its_own_copy() {
    let (url, _db, _h) = common::start_dev_server().await;
    let device = common::device_db().await;
    let local = SurrealEventStore::new(device.clone());
    let event = |event_type: &str, aggregate: &str| NewEvent {
        id: None,
        event_type: event_type.into(),
        aggregate_id: aggregate.into(),
        timestamp: chrono::Utc::now(),
        device_id: "phone".into(),
        payload: serde_json::json!({}),
    };
    local
        .append(event("transaction_recorded", "old"))
        .await
        .unwrap();
    local
        .append(event("journal_entry_created", "j"))
        .await
        .unwrap();
    let sync = omni_me_core::sync::SyncClient::new(url.clone(), "phone".into());
    sync.sync(&device).await.unwrap();

    let client = reqwest::Client::new();
    let preview: serde_json::Value = client
        .post(format!("{url}/wipe/preview"))
        .json(&serde_json::json!({ "features": ["finances"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let report: serde_json::Value = client
        .post(format!("{url}/wipe/confirm"))
        .json(&serde_json::json!({
            "token": preview["token"], "features": ["finances"], "instance": "dev",
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    std::fs::remove_file(report["snapshot"].as_str().unwrap()).ok();

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    local
        .append(event("transaction_recorded", "new"))
        .await
        .unwrap();
    let pulled = sync.pull_only(&device).await.unwrap();
    assert_eq!(pulled.wiped, 1, "only the pre-wipe transaction goes");

    let mut left: Vec<String> = local
        .get_since(chrono::Utc::now() - chrono::Duration::hours(1), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| format!("{} {}", e.event_type, e.aggregate_id))
        .filter(|e| !e.starts_with("data_wiped"))
        .collect();
    left.sort();
    assert_eq!(
        left,
        ["journal_entry_created j", "transaction_recorded new"]
    );
}
