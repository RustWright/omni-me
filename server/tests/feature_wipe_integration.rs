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

use std::sync::Arc;

use omni_me_core::db;
use omni_me_core::events::{EventStore, NewEvent, SurrealEventStore};
use omni_me_core::extraction::null::NullExtractor;
use omni_me_core::llm::NullLlmClient;
use omni_me_core::runtime::Instance;
use omni_me_server::AppState;

/// A server that declares itself `dev`.
///
/// ⛔ The shared harness deliberately declares no instance, because an undeclared
/// deployment is what every destructive tool must refuse — so a wipe's happy path
/// cannot be reached through it. This one names `dev` on purpose, which is also
/// the only instance a test should ever claim to be.
async fn dev_server() -> (String, db::Database, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.db");
    let server_db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);

    let blob_dir = tempfile::tempdir().unwrap();
    let blob_path = blob_dir.path().to_path_buf();
    std::mem::forget(blob_dir);

    let db_arc = Arc::new(server_db);
    let event_store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new((*db_arc).clone()));
    let projections = common::server_projections(&db_arc).await;

    let state = AppState {
        db: db_arc.clone(),
        llm_client: Arc::new(NullLlmClient::unconfigured()),
        blob_dir: Arc::new(blob_path),
        extractor: Arc::new(NullExtractor),
        auto_import_registry: Default::default(),
        store: event_store,
        projections,
        device_id: "test-device".to_string(),
        default_interval: std::time::Duration::from_secs(1800),
        secrets: Default::default(),
        instance: Some(Instance::Dev),
        purge_ticket: Default::default(),
        wipe_ticket: Default::default(),
    };

    let app = omni_me_server::build_app(state, None, None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, (*db_arc).clone(), handle)
}

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
    let (url, db, _h) = dev_server().await;
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
    let (url, db, _h) = dev_server().await;
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
    let (url, db, _h) = dev_server().await;
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
    let (url, db, _h) = dev_server().await;
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
    let (url, db, _h) = dev_server().await;
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
    let (url, _db, _h) = dev_server().await;
    let resp = reqwest::Client::new()
        .post(format!("{url}/wipe/preview"))
        .json(&serde_json::json!({ "features": ["ledger"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(resp.text().await.unwrap().contains("finances"));
}
