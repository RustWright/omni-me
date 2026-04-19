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
use omni_me_core::llm::GeminiClient;
use omni_me_server::{AppState, routes};
use tower_http::cors::CorsLayer;

/// Spin up a real Axum server on a random port with its own temp SurrealDB.
/// Returns (server_url, join_handle). The tempdir is leaked intentionally —
/// it must outlive the running server.
pub async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.db");
    let server_db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);

    let state = AppState {
        db: Arc::new(server_db),
        llm_client: Arc::new(GeminiClient::new("test-key-unused".into())),
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

/// Create a temp SurrealDB instance — simulates a device's local DB.
pub async fn device_db() -> db::Database {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("device.db");
    let db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);
    db
}
