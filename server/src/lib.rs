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
use std::time::Duration;

use axum::{extract::DefaultBodyLimit, routing::get, Json, Router};
use tokio::signal;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use omni_me_core::auto_import::setup::{spawn_sources, DEFAULT_INTERVAL};
use omni_me_core::auto_import_scheduler::{AutoImportSource, SourceRegistry};
use omni_me_core::credentials::{self, Credentials};
use omni_me_core::db::Database;
use omni_me_core::events::{EventStore, ProjectionRunner, SurrealEventStore};
use omni_me_core::extraction::{
    gemini::GeminiExtractor, null::NullExtractor, openai_compat::OpenAiCompatExtractor,
    DocumentExtractor,
};
use omni_me_core::llm::{GeminiClient, LlmClient, OpenAiCompatClient};

const DB_PATH: &str = "surreal_data/server.db";
const LISTEN_ADDR: &str = "0.0.0.0:3000";
const DEFAULT_BLOB_DIR: &str = "blobs";

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    /// Text LLM client — `Arc<dyn LlmClient>` so the provider is swappable
    /// (3.8): Gemini by default, or any OpenAI-compatible endpoint when `[llm]`
    /// selects it. Selected once at boot by [`build_llm_client`].
    pub llm_client: Arc<dyn LlmClient>,
    pub blob_dir: Arc<PathBuf>,
    pub extractor: Arc<dyn DocumentExtractor>,
    /// Status registry for auto-import sources. Empty when the source builder
    /// returns no sources; populated by [`run`] via [`spawn_sources`].
    pub auto_import_registry: SourceRegistry,
    /// The handles needed to *build* an auto-import source live — the in-app
    /// add-source endpoint (3.7 fast-follow) constructs a source from these and
    /// spawns it straight into `auto_import_registry`, no restart required.
    /// `default_interval` is what a freshly-added source inherits unless it
    /// declares its own `schedule_secs`.
    pub store: Arc<dyn EventStore>,
    pub projections: ProjectionRunner,
    pub device_id: String,
    pub default_interval: Duration,
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

    // Load server credentials once (graceful: missing/unreadable → default-empty,
    // so a zero-config public engine still boots — 3.4). Reused for the Gemini
    // key, the text-LLM provider swap (3.8), and the document extractor.
    let creds = credentials::default_path()
        .ok()
        .and_then(|p| credentials::load(&p).ok())
        .unwrap_or_default();

    // Gemini key resolution order: GEMINI_API_KEY env var → credentials.toml
    // [gemini].api_key. Env wins so CI/secret-manager flows still work; the
    // credentials fallback lets local dev boot without exporting the key. Both
    // OPTIONAL — when absent the Gemini client carries an empty key and LLM
    // routes error gracefully at call time rather than crashing boot (3.4).
    let gemini_key = std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| {
            creds
                .gemini
                .as_ref()
                .map(|g| g.api_key.clone())
                .filter(|k| !k.is_empty())
        });

    // Text LLM client (3.8 provider-swap): `[llm]` selects the provider; the
    // default is Gemini with the resolved key.
    let llm_client = build_llm_client(&creds, gemini_key);

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
    // /documents/extract route needs it regardless of auto-import config. Built
    // from the already-loaded `creds`; a missing key degrades to NullExtractor
    // rather than panicking — part of the zero-config boot guarantee (3.4). Its
    // provider-swap (OpenAI-compatible vision) is a deferred fast-follow that
    // will read the same `[llm]` section.
    let extractor: Arc<dyn DocumentExtractor> = build_extractor(&creds);

    // Shared registry — populated below by spawn_sources, read by the
    // /auto_import/status + /auto_import/tick route handlers via AppState.
    let auto_import_registry = SourceRegistry::new();

    // Auto-import build handles. Built before AppState so the state can carry
    // *clones* (the in-app add-source endpoint constructs + spawns a source live
    // from them) while the boot-time `SourceCtx` builder consumes the originals.
    // Projections vec is empty: the server stores events + syncs them to clients,
    // which run their own projections locally.
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
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_INTERVAL);

    let state = AppState {
        db: db_arc.clone(),
        llm_client,
        blob_dir: Arc::new(blob_dir),
        extractor: extractor.clone(),
        auto_import_registry: auto_import_registry.clone(),
        store: event_store_arc.clone(),
        projections: server_projections.clone(),
        device_id: device_id.clone(),
        default_interval: interval,
    };

    // Auto-import: the engine owns the store/projections/device_id but not the
    // sources — those come from the caller's builder.
    let ctx = SourceCtx {
        db: db_arc.clone(),
        store: event_store_arc,
        projections: server_projections,
        device_id,
        extractor,
    };
    let sources = (cfg.source_builder)(ctx).await;
    let source_count = sources.len();
    spawn_sources(sources, interval, &auto_import_registry).await;
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
        .merge(routes::llm_routes())
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
    // 3.8a opt-in: route the document extractor through the OpenAI-compatible
    // endpoint's vision API. Gated on `[llm] vision = true` (+ provider +
    // base_url + model) so we never silently send images to an endpoint without
    // vision; otherwise fall through to the Gemini/Null default below.
    if let Some(cfg) = &creds.llm
        && cfg.provider == "openai_compatible"
        && cfg.vision
    {
        match (cfg.base_url.as_deref(), cfg.model.as_deref()) {
            (Some(base_url), Some(model)) if !base_url.is_empty() && !model.is_empty() => {
                tracing::info!(model = %model, "Document extractor: OpenAI-compatible vision");
                return Arc::new(OpenAiCompatExtractor::new(
                    base_url,
                    model,
                    cfg.api_key.clone().unwrap_or_default(),
                ));
            }
            _ => tracing::warn!(
                "[llm] vision=true but base_url/model missing — falling back to Gemini/Null extractor"
            ),
        }
    }
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

/// Build the *text* LLM client (3.8 provider-swap). `[llm].provider ==
/// "openai_compatible"` (with a non-empty `base_url` + `model`) selects the
/// generic OpenAI-compatible client; anything else — including an absent `[llm]`
/// section — uses Gemini keyed by `gemini_key`. A missing key never crashes
/// boot: the Gemini client carries an empty key and LLM routes error at call
/// time (3.4). The document extractor is built separately and stays on Gemini.
fn build_llm_client(creds: &Credentials, gemini_key: Option<String>) -> Arc<dyn LlmClient> {
    if let Some(cfg) = &creds.llm
        && cfg.provider == "openai_compatible"
    {
        match (cfg.base_url.as_deref(), cfg.model.as_deref()) {
            (Some(base_url), Some(model)) if !base_url.is_empty() && !model.is_empty() => {
                tracing::info!(model = %model, "LLM client: OpenAI-compatible");
                return Arc::new(OpenAiCompatClient::new(
                    base_url,
                    model,
                    cfg.api_key.clone().unwrap_or_default(),
                ));
            }
            _ => tracing::warn!(
                "[llm] provider=openai_compatible but base_url/model missing — \
                 falling back to Gemini"
            ),
        }
    }
    if gemini_key.is_none() {
        tracing::warn!(
            "no LLM provider configured (no [llm] + no Gemini key) — note-processing \
             will error at call time"
        );
    }
    Arc::new(GeminiClient::new(gemini_key.unwrap_or_default()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use omni_me_core::credentials::{GeminiCredentials, LlmProviderConfig};

    fn openai_llm(vision: bool) -> LlmProviderConfig {
        LlmProviderConfig {
            provider: "openai_compatible".into(),
            base_url: Some("http://localhost:11434/v1".into()),
            model: Some("llava".into()),
            api_key: Some("k".into()),
            vision,
        }
    }

    #[test]
    fn build_extractor_uses_vision_only_when_opted_in() {
        // vision opt-in → OpenAI-compatible vision extractor (name == model).
        let creds = Credentials {
            llm: Some(openai_llm(true)),
            ..Default::default()
        };
        assert_eq!(build_extractor(&creds).name(), "llava");

        // Same provider, vision=false, no Gemini key → falls through to Null.
        let creds = Credentials {
            llm: Some(openai_llm(false)),
            ..Default::default()
        };
        assert_eq!(build_extractor(&creds).name(), "null");

        // Gemini key present, no vision opt-in → Gemini extractor (unchanged default).
        let creds = Credentials {
            gemini: Some(GeminiCredentials {
                api_key: "g".into(),
            }),
            ..Default::default()
        };
        assert!(build_extractor(&creds).name().contains("gemini"));
    }

    #[test]
    fn build_llm_client_selects_openai_compatible_text() {
        // Text client swaps independently of the vision flag.
        let creds = Credentials {
            llm: Some(openai_llm(false)),
            ..Default::default()
        };
        assert_eq!(build_llm_client(&creds, None).model_name(), "llava");

        // No [llm] → Gemini default keyed by the passed key.
        let creds = Credentials::default();
        assert_ne!(
            build_llm_client(&creds, Some("g".into())).model_name(),
            "llava"
        );
    }
}
