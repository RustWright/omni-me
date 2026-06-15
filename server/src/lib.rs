//! Public `omni-me-server` library — the composition seam for the open-core split.
//!
//! [`run`] performs all of the engine's *generic* startup (SurrealDB, the Gemini
//! document extractor, the blob store, the HTTP routes, graceful shutdown) and
//! delegates the one thing it deliberately does NOT know about — *which*
//! auto-import sources exist — to a caller-supplied [`SourceBuilder`]. The public
//! binary passes a builder that returns zero sources; the private overlay passes
//! one that wires in the real bank adapters. Neither the engine nor this module
//! references any specific bank.

pub mod routes;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use axum::{extract::DefaultBodyLimit, routing::get, Json, Router};
use tokio::signal;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use omni_me_core::auto_import::setup::{spawn_sources, DEFAULT_INTERVAL};
use omni_me_core::auto_import_scheduler::{AutoImportSource, SourceRegistry};
use omni_me_core::credentials::{self, Credentials};
use omni_me_core::db::Database;
use omni_me_core::events::{EventStore, ProjectionRunner, SurrealEventStore};
use omni_me_core::extraction::{gemini::GeminiExtractor, null::NullExtractor, DocumentExtractor};
use omni_me_core::llm::GeminiClient;

const DB_PATH: &str = "surreal_data/server.db";
const LISTEN_ADDR: &str = "0.0.0.0:3000";
const DEFAULT_BLOB_DIR: &str = "blobs";

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub llm_client: Arc<GeminiClient>,
    pub blob_dir: Arc<PathBuf>,
    pub extractor: Arc<dyn DocumentExtractor>,
    /// Status registry for auto-import sources. Empty when the source builder
    /// returns no sources; populated by [`run`] via [`spawn_sources`].
    pub auto_import_registry: SourceRegistry,
}

/// The shared runtime handles [`run`] hands a [`SourceBuilder`] so it can
/// construct concrete sources without `run` knowing their types. The builder
/// reads whatever else it needs (e.g. bank credentials) itself.
pub struct SourceCtx {
    pub db: Arc<Database>,
    pub store: Arc<dyn EventStore>,
    pub projections: ProjectionRunner,
    pub device_id: String,
    pub extractor: Arc<dyn DocumentExtractor>,
}

/// The future a [`SourceBuilder`] returns: boxed + pinned + `Send`, resolving to
/// the constructed sources. Named so the boxed-async type isn't spelled out at
/// every call site (and to keep clippy's `type_complexity` quiet).
pub type SourceFuture = Pin<Box<dyn Future<Output = Vec<Arc<dyn AutoImportSource>>> + Send>>;

/// A caller-supplied factory that builds the auto-import sources from the
/// engine's runtime handles. Boxed so it can live in [`RunConfig`]; `FnOnce`
/// because `run` calls it exactly once and consumes the [`SourceCtx`]; returns a
/// [`SourceFuture`] because building a source is async (an `ImapSource` connects
/// on construction). `Send` so it crosses `run`'s `.await` points.
pub type SourceBuilder = Box<dyn FnOnce(SourceCtx) -> SourceFuture + Send>;

/// Configuration handed to [`run`]. Carries only the source-construction seam;
/// everything else the engine derives itself. The account roster is a *client*
/// concern (it drives the Accounts screen, not the server), so it is
/// intentionally absent here.
pub struct RunConfig {
    pub source_builder: SourceBuilder,
}

/// Boot the public server: run all generic startup, delegate source
/// construction to `cfg.source_builder`, spawn the resulting sources, then
/// serve until SIGTERM / Ctrl-C.
pub async fn run(cfg: RunConfig) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // `omni_me_core=info` ensures auto-import scheduler ticks +
                // warnings surface in default logs.
                "omni_me_server=debug,omni_me_core=info,tower_http=debug".into()
            }),
        )
        .init();

    let db = omni_me_core::db::connect(DB_PATH)
        .await
        .expect("failed to connect to SurrealDB");

    // Gemini key resolution order: GEMINI_API_KEY env var → credentials.toml
    // [gemini].api_key. Env wins so CI/secret-manager flows still work; the
    // credentials fallback lets local dev boot without exporting the key.
    let api_key = match std::env::var("GEMINI_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            let creds_path = credentials::default_path()
                .expect("XDG_CONFIG_HOME / HOME must be set for credentials path");
            credentials::load(&creds_path)
                .ok()
                .and_then(|c| c.gemini.map(|g| g.api_key))
                .filter(|k| !k.is_empty())
                .expect("GEMINI_API_KEY env var unset and no gemini.api_key in credentials.toml")
        }
    };
    let llm_client = Arc::new(GeminiClient::new(api_key));

    let blob_dir: PathBuf = std::env::var("BLOB_DIR")
        .unwrap_or_else(|_| DEFAULT_BLOB_DIR.into())
        .into();
    tokio::fs::create_dir_all(&blob_dir)
        .await
        .expect("failed to create blob dir");
    tracing::info!("blob dir: {}", blob_dir.display());

    let db_arc = Arc::new(db);

    // Document extractor (NullExtractor fallback when no Gemini key) — lifted
    // out of any auto-import conditional so AppState always carries one; the
    // /documents/extract route needs it regardless of auto-import config.
    let creds_path = credentials::default_path()
        .expect("XDG_CONFIG_HOME / HOME must be set for credentials path");
    let extractor: Arc<dyn DocumentExtractor> = match credentials::load(&creds_path).ok() {
        Some(c) => build_extractor(&c),
        None => {
            tracing::warn!("no credentials.toml — documents/extract will use NullExtractor");
            Arc::new(NullExtractor)
        }
    };

    // Shared registry — populated below by spawn_sources, read by the
    // /auto_import/status + /auto_import/tick route handlers via AppState.
    let auto_import_registry = SourceRegistry::new();

    let state = AppState {
        db: db_arc.clone(),
        llm_client,
        blob_dir: Arc::new(blob_dir),
        extractor: extractor.clone(),
        auto_import_registry: auto_import_registry.clone(),
    };

    // Auto-import: the engine owns the store/projections/device_id but not the
    // sources — those come from the caller's builder. Projections vec is empty:
    // the server stores events + syncs them to clients, which run their own
    // projections locally.
    let device_id = std::env::var("OMNI_SERVER_DEVICE_ID")
        .unwrap_or_else(|_| "server-auto-import".to_string());
    let server_projections = ProjectionRunner::new((*db_arc).clone(), Vec::new());
    if let Err(e) = server_projections.init_all().await {
        tracing::warn!(error = %e, "server projection_versions init failed");
    }
    let event_store_arc: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new((*db_arc).clone()));

    // OMNI_AUTO_IMPORT_INTERVAL_SECS override: lets a dev-test shrink the
    // 30-min default so a batch lands during a manual test window. Clamped to
    // [60, 3600] — below 60s hammers upstream APIs, above 3600s is no
    // different from the default for testing.
    let interval = std::env::var("OMNI_AUTO_IMPORT_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|s| s.clamp(60, 3600))
        .map(std::time::Duration::from_secs)
        .unwrap_or(DEFAULT_INTERVAL);

    let ctx = SourceCtx {
        db: db_arc.clone(),
        store: event_store_arc,
        projections: server_projections,
        device_id,
        extractor,
    };
    let sources = (cfg.source_builder)(ctx).await;
    let source_count = sources.len();
    let _handles = spawn_sources(sources, interval, &auto_import_registry).await;
    tracing::info!(
        sources = source_count,
        interval_secs = interval.as_secs(),
        "auto-import scheduler initialized"
    );

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

/// Build the document extractor — real `GeminiExtractor` if a key is present in
/// `credentials.gemini`, else `NullExtractor`. Server-side: this is where
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
