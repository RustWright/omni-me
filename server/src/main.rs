use axum::{Router, Json, routing::get, extract::DefaultBodyLimit};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tokio::signal;

use omni_me_core::auto_import::imap::ImapHandler;
use omni_me_core::auto_import::imap_real::AsyncImapFetcher;
use omni_me_core::auto_import::imap_source::{CursorStore, ImapSource, SurrealCursorStore};
use omni_me_core::auto_import::receipts::ReceiptHandler;
use omni_me_core::auto_import::sc_ngn::ScNgnHandler;
use omni_me_core::auto_import::setup::{setup_from_credentials, SourceConfig};
use omni_me_core::auto_import_scheduler::SourceRegistry;
use omni_me_core::credentials::{self, Credentials};
use omni_me_core::events::{EventStore, ProjectionRunner, SurrealEventStore};
use omni_me_core::extraction::{gemini::GeminiExtractor, null::NullExtractor, DocumentExtractor};
use omni_me_server::{AppState, routes};

const DB_PATH: &str = "surreal_data/server.db";
const LISTEN_ADDR: &str = "0.0.0.0:3000";
const DEFAULT_BLOB_DIR: &str = "blobs";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // `omni_me_core=info` ensures auto-import scheduler ticks +
                // warnings (e.g. WS re-auth needed) surface in default logs.
                // Without this, `./omni-me-server` runs blind.
                "omni_me_server=debug,omni_me_core=info,tower_http=debug".into()
            }),
        )
        .init();

    let db = omni_me_core::db::connect(DB_PATH)
        .await
        .expect("failed to connect to SurrealDB");

    let api_key = std::env::var("GEMINI_API_KEY")
        .expect("GEMINI_API_KEY must be set");
    let llm_client = Arc::new(omni_me_core::llm::GeminiClient::new(api_key));

    let blob_dir: PathBuf = std::env::var("BLOB_DIR")
        .unwrap_or_else(|_| DEFAULT_BLOB_DIR.into())
        .into();
    tokio::fs::create_dir_all(&blob_dir)
        .await
        .expect("failed to create blob dir");
    tracing::info!("blob dir: {}", blob_dir.display());

    let db_arc = Arc::new(db);

    // Build the document extractor up-front (NullExtractor fallback if no
    // Gemini key). Lifted out of the credentials conditional so AppState
    // always carries one — the /documents/extract route needs it whether or
    // not auto-import is configured.
    let creds_path = credentials::default_path()
        .expect("XDG_CONFIG_HOME / HOME must be set for credentials path");
    let creds_opt = credentials::load(&creds_path).ok();
    let extractor: Arc<dyn DocumentExtractor> = match &creds_opt {
        Some(c) => build_extractor(c),
        None => {
            tracing::warn!(
                "no credentials.toml — documents/extract will use NullExtractor"
            );
            Arc::new(NullExtractor)
        }
    };

    // Shared registry — populated below by setup_from_credentials, read by
    // the /auto_import/status + /auto_import/tick route handlers via AppState.
    let auto_import_registry = SourceRegistry::new();

    let state = AppState {
        db: db_arc.clone(),
        llm_client,
        blob_dir: Arc::new(blob_dir),
        extractor: extractor.clone(),
        auto_import_registry: auto_import_registry.clone(),
    };

    // Auto-import scheduler — Wise + WS + IMAP spin up from credentials.toml.
    // Projections vec is empty: server stores events and syncs them to clients;
    // clients run their own projections locally. Auto-import sources append
    // events into the event store; sync pipeline handles propagation.
    let device_id = std::env::var("OMNI_SERVER_DEVICE_ID")
        .unwrap_or_else(|_| "server-auto-import".to_string());
    let server_projections = ProjectionRunner::new((*db_arc).clone(), Vec::new());
    if let Err(e) = server_projections.init_all().await {
        tracing::warn!(error = %e, "server projection_versions init failed");
    }
    let event_store_arc: Arc<dyn EventStore> =
        Arc::new(SurrealEventStore::new((*db_arc).clone()));

    match creds_opt {
        Some(creds) => {
            let cursor_store_arc: Arc<SurrealCursorStore> =
                Arc::new(SurrealCursorStore::new((*db_arc).clone()));
            if let Err(e) = cursor_store_arc.init_schema().await {
                tracing::warn!(error = %e, "imap_cursors schema init failed");
            }
            let imap_sources = build_imap_sources(
                &creds,
                extractor.clone(),
                cursor_store_arc.clone(),
                event_store_arc.clone(),
                server_projections.clone(),
                device_id.clone(),
            )
            .await;
            let config = SourceConfig {
                ws_driver_script: creds
                    .wealthsimple_python
                    .as_ref()
                    .and_then(|w| w.driver_script.clone()),
                imap_sources,
                ..SourceConfig::default()
            };
            let _handles = setup_from_credentials(
                &creds,
                &config,
                event_store_arc,
                server_projections,
                device_id,
                &auto_import_registry,
            )
            .await;
            tracing::info!(path = %creds_path.display(), "auto-import scheduler initialized");
        }
        None => {
            tracing::info!(
                path = %creds_path.display(),
                "no credentials.toml — auto-import skipped"
            );
        }
    }

    let app = Router::new()
        .route("/health", get(health))
        .merge(routes::sync_routes())
        .merge(routes::notes_routes())
        .layer(DefaultBodyLimit::max(256 * 1024))
        .merge(routes::blob_routes())
        .merge(routes::documents_routes())
        .merge(routes::auto_import_routes())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR)
        .await
        .expect("failed to bind");

    tracing::info!("listening on {LISTEN_ADDR}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Build the document extractor — real `GeminiExtractor` if a key is present
/// in credentials.gemini, else `NullExtractor`. Server-side: this is where
/// `feedback_llm_server_side.md` is honored — Gemini calls originate here.
fn build_extractor(creds: &Credentials) -> Arc<dyn DocumentExtractor> {
    match &creds.gemini {
        Some(g) if !g.api_key.is_empty() => {
            tracing::info!("Gemini extractor wired");
            Arc::new(GeminiExtractor::new(g.api_key.clone()))
        }
        _ => {
            tracing::warn!(
                "no Gemini API key in credentials — handlers will use NullExtractor (no events)"
            );
            Arc::new(NullExtractor)
        }
    }
}

const RECEIPT_SENDER_PATTERNS: &[&str] = &[
    "audible",
    "oxio",
    "amazon",
    "walmart",
    "netflix",
    "spotify",
    "manitoba",
    "remitly",
    "greenhouse",
    "peggo",
    "@wise.com",
];
const RECEIPT_SENDER_EXCLUSIONS: &[&str] = &["@sc.com"];

async fn build_imap_sources(
    creds: &Credentials,
    extractor: Arc<dyn DocumentExtractor>,
    cursor_store: Arc<SurrealCursorStore>,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    device_id: String,
) -> Vec<Arc<ImapSource>> {
    let mut sources = Vec::new();
    let cs_dyn: Arc<dyn CursorStore> = cursor_store.clone();
    for (account_name, imap_creds) in &creds.imap {
        let fetcher: Arc<AsyncImapFetcher> =
            Arc::new(AsyncImapFetcher::new(account_name.clone(), imap_creds.clone()));
        let mut handlers: Vec<Box<dyn ImapHandler>> = Vec::new();
        for sc in &creds.sc_accounts {
            handlers.push(Box::new(ScNgnHandler::new(
                format!("sc_{}", sc.commodity.to_lowercase()),
                sc.account_number.clone(),
                sc.hledger_account.clone(),
                sc.commodity.clone(),
                device_id.clone(),
                extractor.clone(),
            )));
        }
        handlers.push(Box::new(
            ReceiptHandler::new(
                "receipts",
                RECEIPT_SENDER_PATTERNS.iter().map(|s| s.to_string()).collect(),
                device_id.clone(),
                extractor.clone(),
            )
            .with_excluded(
                RECEIPT_SENDER_EXCLUSIONS.iter().map(|s| s.to_string()).collect(),
            ),
        ));
        match ImapSource::new(
            account_name.clone(),
            fetcher,
            handlers,
            Some(cs_dyn.clone()),
            store.clone(),
            projections.clone(),
        )
        .await
        {
            Ok(s) => {
                tracing::info!(account = account_name, "ImapSource built");
                sources.push(Arc::new(s));
            }
            Err(e) => {
                tracing::warn!(account = account_name, error = %e, "ImapSource construction failed; skipping");
            }
        }
    }
    sources
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, starting graceful shutdown");
}
