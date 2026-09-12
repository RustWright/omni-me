//! Public `omni-me-server` library — the composition seam for the open-core split.
//!
//! [`run`] performs all of the engine's *generic* startup (SurrealDB, the
//! document extractor, the blob store, the HTTP routes, graceful shutdown) and
//! delegates the one thing it deliberately does NOT know about — *which*
//! auto-import sources exist — to a caller-supplied [`SourceBuilder`]. The public
//! binary passes a builder that returns zero sources; the private overlay passes
//! one that wires in the real bank adapters. Neither the engine nor this module
//! references any specific bank.

pub mod routes;

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{Request, StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use tokio::signal;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use omni_me_core::auto_import::setup::{DEFAULT_INTERVAL, spawn_sources};
use omni_me_core::auto_import_scheduler::{AutoImportSource, SourceRegistry};
use omni_me_core::credentials::{self, Credentials, LlmRole};
use omni_me_core::db::Database;
use omni_me_core::events::{EventStore, ProjectionRunner, SurrealEventStore};
use omni_me_core::extraction::{
    DocumentExtractor, null::NullExtractor, openai_compat::OpenAiCompatExtractor,
};
use omni_me_core::llm::{ClientOptions, LlmClient, build_llm_client};

const DB_PATH: &str = "surreal_data/server.db";
const LISTEN_ADDR: &str = "0.0.0.0:3000";
const DEFAULT_BLOB_DIR: &str = "blobs";

/// Where blobs live, from `BLOB_DIR` or the default.
///
/// ⚠️ **Public because the overlay needs the same answer.** It builds the IMAP
/// sources (handler composition is per-app policy) and those archive every
/// message they fetch — resolving the path independently there would let the
/// two disagree, and a document written to the wrong directory is one the
/// `/blobs/{hash}` route cannot serve.
pub fn blob_dir_from_env() -> PathBuf {
    std::env::var("BLOB_DIR")
        .unwrap_or_else(|_| DEFAULT_BLOB_DIR.into())
        .into()
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    /// Text LLM client — `Arc<dyn LlmClient>` so the endpoint is swappable:
    /// any OpenAI-compatible endpoint `[llm]` selects, or a `NullLlmClient` that
    /// reports the gap when none is configured. Selected once at boot by
    /// [`build_llm_client`].
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
    /// The credentials file's name-keyed `secrets` map, resolved by routes that
    /// take a secret *name* rather than a secret.
    ///
    /// Statement upload is the current user: an encrypted statement needs a
    /// password, but the rule for deriving one is the bank's invention and has
    /// no place in a general engine. The client names a secret, the value is
    /// looked up here, and the engine learns nothing about the institution —
    /// the same indirection `[llm].api_key` and the subprocess helpers use.
    pub secrets: Arc<HashMap<String, String>>,
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
    // so a zero-config public engine still boots — 3.4). Reused for the text-LLM
    // client and the document extractor.
    let creds = credentials::default_path()
        .ok()
        .and_then(|p| credentials::load(&p).ok())
        .unwrap_or_default();

    // HTTP bearer token. Deliberately FAILS OPEN when `[server]` is absent:
    // upgrading the box must not start rejecting devices that haven't been
    // given the token yet, or a routine deploy silently kills sync everywhere
    // and looks like a network fault. The warning below carries a ready-made
    // token so closing the gap is copy-paste, and it keeps shouting on every
    // boot until it is closed.
    let auth_token = creds
        .server
        .as_ref()
        .map(|s| s.auth_token.clone())
        .filter(|t| !t.trim().is_empty());
    if auth_token.is_none() {
        tracing::warn!(
            suggested_token = %credentials::generate_auth_token(),
            "SECURITY: no [server].auth_token configured — every endpoint is \
             unauthenticated to anything that can reach this port. Add \
             `[server]\\nauth_token = \"<token>\"` to credentials.toml and set the \
             same value on each device (Settings → Server token), then restart.",
        );
    }

    // Text LLM client: `[llm]` selects the endpoint. Shared with the agent, which
    // runs the assistant against the same section.
    //
    // `ClientOptions::default()` is deliberate — routing preferences are gateway
    // vocabulary, and production goes direct to the provider. See
    // `core::llm::ClientOptions`.
    let llm_client = build_llm_client(&creds, ClientOptions::default());

    let blob_dir: PathBuf = blob_dir_from_env();
    tokio::fs::create_dir_all(&blob_dir)
        .await
        .expect("failed to create blob dir");
    tracing::info!("blob dir: {}", blob_dir.display());

    let db_arc = Arc::new(db);

    // Document extractor (NullExtractor fallback when no vision endpoint) — lifted
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
    let device_id =
        std::env::var("OMNI_SERVER_DEVICE_ID").unwrap_or_else(|_| "server-auto-import".to_string());
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
        secrets: Arc::new(creds.secrets.clone()),
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
    // Persisted off-switch set (#367). A source the user switched off stays off
    // across restarts — critical for a runaway bank source, where re-arming on
    // reboot would resume the exact hammering the pause was meant to stop.
    // Threaded into `spawn_sources` so a paused source is registered but never
    // spawned (not even one boot tick). Applies uniformly to compiled overlay
    // sources too: everything the builder returns flows through here. A load
    // failure degrades to "nothing paused" rather than failing startup.
    let paused_names = match omni_me_core::auto_import::paused::default_path() {
        Ok(p) => omni_me_core::auto_import::paused::load(&p).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to load persisted paused sources — treating none as paused");
            Default::default()
        }),
        Err(e) => {
            tracing::warn!(error = %e, "paused-sources path lookup failed — treating none as paused");
            Default::default()
        }
    };

    let sources = (cfg.source_builder)(ctx).await;
    let source_count = sources.len();
    let paused_count = paused_names.len();
    spawn_sources(sources, interval, &auto_import_registry, &paused_names).await;

    tracing::info!(
        sources = source_count,
        paused = paused_count,
        interval_secs = interval.as_secs(),
        "auto-import scheduler initialized"
    );

    // App-update hosting (optional, generic): when UPDATES_DIR is set the server
    // serves that directory read-only under /updates so a self-hosted box can
    // hand out signed app artifacts + per-platform manifests to its own devices
    // over the tailnet. Unset → the route is absent (404). Bank-free: any
    // self-hoster can point UPDATES_DIR at their own release dir.
    let updates_dir = std::env::var("UPDATES_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    match &updates_dir {
        Some(d) => tracing::info!("updates dir: {} (serving /updates)", d.display()),
        None => tracing::info!("UPDATES_DIR unset — /updates route disabled"),
    }

    let app = build_app(state, updates_dir, auth_token);

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR)
        .await
        .expect("failed to bind");

    tracing::info!("listening on {LISTEN_ADDR}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

/// Assemble the HTTP router. Split out of [`run`] so tests can build the exact
/// production route set against a test [`AppState`]. `updates_dir`, when `Some`,
/// mounts a read-only static file service at `/updates` (app-update hosting);
/// `None` leaves the route absent.
pub fn build_app(
    state: AppState,
    updates_dir: Option<PathBuf>,
    auth_token: Option<String>,
) -> Router {
    // Everything that reads or writes state sits behind the bearer gate. The
    // exceptions are deliberate: `/health` has to answer the deploy's readiness
    // probe before any device is provisioned, and `/updates` has to stay
    // reachable so a device that has lost its token can still pull an APK and
    // recover — it serves only already-published artifacts.
    let mut protected = Router::new()
        .merge(routes::sync_routes())
        .merge(routes::notes_routes())
        .layer(DefaultBodyLimit::max(256 * 1024))
        .merge(routes::blob_routes())
        .merge(routes::documents_routes())
        .merge(routes::statement_routes())
        .merge(routes::auto_import_routes())
        .merge(routes::feedback_routes())
        .merge(routes::llm_routes());

    if let Some(token) = auth_token {
        // `route_layer`, NOT `layer`: a plain `layer` also wraps the fallback,
        // so every unmatched path would answer 401 instead of 404 — which turns
        // a typo'd URL into an auth failure and makes the box maddening to
        // debug. `route_layer` only runs for paths this router actually has.
        protected = protected.route_layer(middleware::from_fn_with_state(
            Arc::new(token),
            require_token,
        ));
    }

    let mut app = Router::new().route("/health", get(health)).merge(protected);

    if let Some(dir) = updates_dir {
        // ServeDir maps /updates/<rel> -> <dir>/<rel>; missing files → 404. GETs
        // carry no request body, so the 256 KiB DefaultBodyLimit above is
        // irrelevant to serving large artifacts (APK / AppImage).
        app = app.nest_service("/updates", ServeDir::new(dir));
    }

    // No CORS layer. It used to be `CorsLayer::permissive()`, which was both
    // unused and actively harmful: the WASM frontend makes zero direct HTTP
    // calls (everything goes through the Rust client in `commands/*`), while
    // the permissive preflight let any page the user happened to open drive
    // these endpoints cross-origin. With no CORS headers a browser refuses the
    // preflight, and the JSON content-type these routes require means the
    // no-preflight "simple request" path can't reach them either.
    app.layer(TraceLayer::new_for_http()).with_state(state)
}

/// Reject any request that does not carry `Authorization: Bearer <token>`.
///
/// The comparison is constant-time in the length-equal case; an attacker who
/// can measure it is already on the tailnet, but the cost of getting this right
/// is one loop.
async fn require_token(
    State(expected): State<Arc<String>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
        Ok(next.run(req).await)
    } else {
        // No body: nothing to learn from the response beyond "not authorized".
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Build the document extractor — `OpenAiCompatExtractor` when `[llm]` opts in
/// to vision, else `NullExtractor`. Server-side: this is where
/// `feedback_llm_server_side.md` is honored — extraction calls originate here.
///
/// `vision = true` is an explicit assertion that the configured endpoint accepts
/// images, because support varies across OpenAI-compatible servers and a silent
/// POST to one that cannot would surface as a confusing upstream error.
fn build_extractor(creds: &Credentials) -> Arc<dyn DocumentExtractor> {
    // ⚠️ Role C, not `[llm]` directly. The two are genuinely different models:
    // `[llm]`'s leading candidate is chosen on latency and is **text-only**, so
    // reading `[llm]` here meant `vision = true` pointed the extractor at a
    // model that cannot see. `[llm.extractor]` overrides only what differs;
    // absent, `for_role` returns `[llm]` unchanged and behaviour is as before.
    if let Some(cfg) = creds.llm.as_ref().map(|c| c.for_role(LlmRole::Extractor))
        && cfg.provider == "openai_compatible"
        && cfg.vision
    {
        let cfg = &cfg;
        match (cfg.base_url.as_deref(), cfg.model.as_deref()) {
            (Some(base_url), Some(model)) if !base_url.is_empty() && !model.is_empty() => {
                tracing::info!(model = %model, "Document extractor: OpenAI-compatible vision");
                return Arc::new(OpenAiCompatExtractor::new(
                    base_url,
                    model,
                    cfg.api_key.clone().unwrap_or_default(),
                ));
            }
            _ => tracing::warn!("[llm.extractor] vision=true but base_url/model missing"),
        }
    }
    tracing::warn!(
        "no vision-capable extractor configured ([llm.extractor], or [llm] as \
         fallback) — handlers will use NullExtractor (no events)"
    );
    Arc::new(NullExtractor)
}

// The *text* LLM client selector lives in `omni_me_core::llm::build_llm_client`,
// so the server and the agent cannot disagree about which provider a `[llm]`
// section selects. The extractor is built here because only the server has one.

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
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
    use omni_me_core::credentials::LlmProviderConfig;

    fn openai_llm(vision: bool) -> LlmProviderConfig {
        LlmProviderConfig {
            provider: "openai_compatible".into(),
            base_url: Some("http://localhost:11434/v1".into()),
            model: Some("llava".into()),
            api_key: Some("k".into()),
            vision,
            allow_closed_weights: false,
            ..Default::default()
        }
    }

    /// Role C overriding `[llm]`: the case the role split exists for — the
    /// assistant on a text-only model while the extractor reads images.
    fn llm_with_extractor_role(model: &str) -> LlmProviderConfig {
        LlmProviderConfig {
            model: Some("openai/gpt-oss-120b".into()),
            vision: false,
            extractor: Some(omni_me_core::credentials::LlmRoleOverride {
                model: Some(model.into()),
                vision: Some(true),
                ..Default::default()
            }),
            ..openai_llm(false)
        }
    }

    #[test]
    fn the_extractor_role_overrides_the_shared_llm_section() {
        // `[llm]` is text-only with vision off; `[llm.extractor]` turns vision
        // on and names a different model. Reading `[llm]` directly — what this
        // did before 2026-09-11 — would build a NullExtractor here.
        let creds = Credentials {
            llm: Some(llm_with_extractor_role("z-ai/glm-5.3-flash")),
            ..Default::default()
        };
        assert_eq!(build_extractor(&creds).name(), "z-ai/glm-5.3-flash");
    }

    #[test]
    fn an_unset_role_inherits_the_shared_section() {
        // No `[llm.extractor]` at all: behaviour must be exactly as before the
        // role split, or every existing credentials.toml changes meaning.
        let creds = Credentials {
            llm: Some(openai_llm(true)),
            ..Default::default()
        };
        assert_eq!(build_extractor(&creds).name(), "llava");
    }

    #[test]
    fn a_role_inherits_base_url_and_key_it_does_not_restate() {
        let cfg = llm_with_extractor_role("z-ai/glm-5.3-flash")
            .for_role(omni_me_core::credentials::LlmRole::Extractor);
        assert_eq!(cfg.base_url.as_deref(), Some("http://localhost:11434/v1"));
        assert_eq!(cfg.api_key.as_deref(), Some("k"));
        assert_eq!(cfg.provider, "openai_compatible");
    }

    #[test]
    fn build_extractor_uses_vision_only_when_opted_in() {
        // vision opt-in → OpenAI-compatible vision extractor (name == model).
        let creds = Credentials {
            llm: Some(openai_llm(true)),
            ..Default::default()
        };
        assert_eq!(build_extractor(&creds).name(), "llava");

        // Same provider, vision=false → no extractor rather than a silent
        // image POST to an endpoint that may not accept one.
        let creds = Credentials {
            llm: Some(openai_llm(false)),
            ..Default::default()
        };
        assert_eq!(build_extractor(&creds).name(), "null");

        // No [llm] at all → Null.
        assert_eq!(build_extractor(&Credentials::default()).name(), "null");
    }

    /// Selection itself is tested in `core::llm::provider`. What is worth
    /// asserting here is the pairing the server owns: one `[llm]` section feeds
    /// both the text client and the extractor, and `vision` gates only the
    /// second — a text client must not require a vision-capable endpoint.
    #[test]
    fn the_vision_flag_gates_the_extractor_without_touching_the_text_client() {
        let creds = Credentials {
            llm: Some(openai_llm(false)),
            ..Default::default()
        };
        assert_eq!(
            build_llm_client(&creds, ClientOptions::default()).model_name(),
            "llava"
        );
        assert_eq!(build_extractor(&creds).name(), "null");
    }
}
