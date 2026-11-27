// `POST /calendar/refresh` through the client a device calls it with (ruling 113).

mod common;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use axum::Router;
use omni_me_core::calendar::sync::{Reader, failing, recorded};
use omni_me_core::calendar::{CalendarEvent, CalendarSource, EventTime, Window};
use omni_me_core::db;
use omni_me_core::events::{EventStore, SurrealEventStore};
use omni_me_core::extraction::null::NullExtractor;
use omni_me_core::llm::NullLlmClient;
use omni_me_core::sync::SyncClient;
use omni_me_server::{AppState, routes};

struct Fake(Mutex<Result<Vec<CalendarEvent>, String>>);

// Spelled out as `#[async_trait]` expands it: the server has no async-trait dependency.
impl CalendarSource for Fake {
    fn name(&self) -> &str {
        "fake"
    }
    fn fetch<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _window: &'life1 Window,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<CalendarEvent>, String>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        let result = self.0.lock().unwrap().clone();
        Box::pin(async move { result })
    }
}

async fn serve(calendar: Option<Arc<dyn CalendarSource>>) -> (String, db::Database) {
    let dir = tempfile::tempdir().unwrap();
    let server_db = db::connect(dir.path().join("s.db").to_str().unwrap())
        .await
        .unwrap();
    let blobs = dir.path().join("blobs");
    std::fs::create_dir(&blobs).unwrap();
    std::mem::forget(dir);

    let db_arc = Arc::new(server_db.clone());
    let store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new(server_db.clone()));
    let state = AppState {
        db: db_arc,
        llm_client: Arc::new(NullLlmClient::unconfigured()),
        blob_dir: Arc::new(blobs),
        extractor: Arc::new(NullExtractor),
        auto_import_registry: Default::default(),
        store,
        projections: common::server_projections(&server_db).await,
        device_id: "server".to_string(),
        default_interval: std::time::Duration::from_secs(1800),
        secrets: Default::default(),
        instance: None,
        purge_ticket: Default::default(),
        wipe_ticket: Default::default(),
        calendar: calendar.map(|s| Arc::new(Reader::new(s))),
    };
    let app: Router = routes::planning_routes().with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, server_db)
}

fn lecture() -> CalendarEvent {
    let start = chrono::Utc::now() + chrono::Duration::days(1);
    CalendarEvent {
        uid: "a".into(),
        calendar: "main".into(),
        title: "Lecture".into(),
        location: None,
        when: EventTime::Timed {
            start,
            end: start + chrono::Duration::hours(1),
        },
        series: None,
    }
}

#[tokio::test]
async fn a_refresh_records_the_read_and_a_failure_records_the_status() {
    let source = Arc::new(Fake(Mutex::new(Ok(vec![lecture()]))));
    let (url, server_db) = serve(Some(source.clone())).await;
    let client = SyncClient::new(url, "agent".into());

    assert!(client.refresh_calendar().await.unwrap());
    assert_eq!(recorded(&server_db, None).await.unwrap().len(), 1);
    assert!(failing(&server_db).await.unwrap().is_empty());

    *source.0.lock().unwrap() = Err("invalid_grant".into());
    let err = client.refresh_calendar().await.unwrap_err().to_string();
    assert!(
        err.contains("502") && err.contains("invalid_grant"),
        "{err}"
    );
    assert_eq!(recorded(&server_db, None).await.unwrap().len(), 1, "kept");
    let down = failing(&server_db).await.unwrap();
    assert_eq!(down.len(), 1);
    assert_eq!(down[0].source, "fake");
}

#[tokio::test]
async fn a_server_without_a_source_has_nothing_fresher() {
    let (url, _db) = serve(None).await;
    let client = SyncClient::new(url, "agent".into());
    assert!(!client.refresh_calendar().await.unwrap());
}
