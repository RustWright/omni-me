// Shared fixtures for server integration tests.
//
// Located at tests/common/mod.rs (not tests/common.rs) so Cargo treats it as a
// submodule of each test binary rather than compiling it as its own test crate.

// Each test binary only uses a subset of these helpers; the unused ones
// trigger dead_code warnings per-binary. Allow them since the module is
// shared.
#![allow(dead_code)]

use std::sync::Arc;

use axum::{Json, Router, routing::get};
use omni_me_core::db;
use omni_me_core::events::{EventStore, ProjectionRunner, SurrealEventStore};
use omni_me_core::extraction::null::NullExtractor;
use omni_me_core::llm::GeminiClient;
use omni_me_server::{AppState, routes};

/// Spin up a real Axum server on a random port with its own temp SurrealDB.
/// Returns (server_url, join_handle). The tempdir is leaked intentionally —
/// it must outlive the running server.
pub async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.db");
    let server_db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);

    let blob_dir = tempfile::tempdir().unwrap();
    let blob_path = blob_dir.path().to_path_buf();
    std::mem::forget(blob_dir);

    let db_arc = Arc::new(server_db);
    let event_store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new((*db_arc).clone()));
    let projections = ProjectionRunner::new((*db_arc).clone(), Vec::new());

    let state = AppState {
        db: db_arc.clone(),
        llm_client: Arc::new(GeminiClient::new("test-key-unused".into())),
        blob_dir: Arc::new(blob_path),
        extractor: Arc::new(NullExtractor),
        auto_import_registry: Default::default(),
        store: event_store,
        projections,
        device_id: "test-device".to_string(),
        default_interval: std::time::Duration::from_secs(1800),
        secrets: Default::default(),
    };

    let app = Router::new()
        .route("/health", get(health))
        .merge(routes::sync_routes())
        // No CORS layer — production dropped it (security review High #4); the test
        // harness must mirror the real router or it would hide the difference.
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

/// Boot the *full* production router (via `omni_me_server::build_app`) on a random
/// port, with an optional `/updates` static dir. Returns (server_url, handle).
/// Used by the updates-route test; `start_server` above stays minimal (sync only).
pub async fn start_full_server(
    updates_dir: Option<std::path::PathBuf>,
) -> (String, tokio::task::JoinHandle<()>) {
    start_full_server_with_auth(updates_dir, None).await
}

/// As [`start_full_server`], but with an optional bearer token enforced on
/// every route except `/health` and `/updates`.
pub async fn start_full_server_with_auth(
    updates_dir: Option<std::path::PathBuf>,
    auth_token: Option<String>,
) -> (String, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.db");
    let server_db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);

    let blob_dir = tempfile::tempdir().unwrap();
    let blob_path = blob_dir.path().to_path_buf();
    std::mem::forget(blob_dir);

    let db_arc = Arc::new(server_db);
    let event_store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new((*db_arc).clone()));
    let projections = ProjectionRunner::new((*db_arc).clone(), Vec::new());

    let state = AppState {
        db: db_arc.clone(),
        llm_client: Arc::new(GeminiClient::new("test-key-unused".into())),
        blob_dir: Arc::new(blob_path),
        extractor: Arc::new(NullExtractor),
        auto_import_registry: Default::default(),
        store: event_store,
        projections,
        device_id: "test-device".to_string(),
        default_interval: std::time::Duration::from_secs(1800),
        secrets: Default::default(),
    };

    let app = omni_me_server::build_app(state, updates_dir, auth_token);

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

/// Create a temp SurrealDB instance — simulates a device's local DB.
pub async fn device_db() -> db::Database {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device.db");
    let db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);
    db
}
