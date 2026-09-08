//! omni-me agent — a headless device.
//!
//! Own process, own database, own device id, syncing over HTTP exactly as a
//! phone does. The rationale for the device shape (rather than a module inside
//! the server) is in `docs/src/assistant.md` § "The assistant is a device, not a
//! feature"; the short version is that the embedded store is opened per process,
//! and the shape that forces is also the one worth wanting — contained failure,
//! inherited machinery, proposals that sync for free.
//!
//! **This phase registers no verbs and calls no model.** What it proves is that a
//! second binary can backfill the log, run projections, and author an event that
//! reaches the other devices — and that it inherits the feature guard and the
//! non-production posture rather than re-implementing either.

use std::path::PathBuf;
use std::sync::Arc;

use omni_me_core::config::ResolvedConfig;
use omni_me_core::db::{self, Database};
use omni_me_core::events::{
    EventStore, EventWriter, ProjectionRunner, SurrealEventStore, load_persisted, registry,
};
use omni_me_core::runtime::{self, ServerUrlPolicy};
use omni_me_core::sync::{
    self, NetworkMonitor, PullScheduler, PushDebouncer, RetryEngine, SyncClient, wire_accelerator,
    wire_puller_network,
};

const DB_NAME: &str = "agent.db";
const DEVICE_ID_FILE: &str = "device_id";
const SERVER_URL_FILE: &str = "server_url";
const SERVER_TOKEN_FILE: &str = "server_token";

/// Runtime override for the agent's data root.
///
/// Setting it also puts the agent in a **non-production posture** — see
/// `omni_me_core::runtime`. Deliberately a different variable from the client's
/// `OMNI_DATA_DIR`: the two must never share a data root, because pointing two
/// processes at one embedded database is the corruption case the device shape
/// exists to avoid.
const DATA_DIR_ENV: &str = "OMNI_AGENT_DATA";
const SERVER_URL_ENV: &str = "OMNI_AGENT_SERVER_URL";

/// Fresh-install default sync target, overridable at build time like the
/// client's. Unset → localhost, so a zero-config agent talks to a local hub
/// rather than anything real.
const DEFAULT_SERVER_URL: &str = match option_env!("OMNI_AGENT_DEFAULT_SERVER_URL") {
    Some(url) => url,
    None => "http://localhost:3000",
};
/// Where a non-production run points when the env var is unset. Never
/// `DEFAULT_SERVER_URL` — see `runtime::ServerUrlPolicy`.
const NON_PRODUCTION_SERVER_URL: &str = "http://localhost:3000";

/// How the agent was asked to run.
struct Args {
    /// Refuse to construct an `EventWriter` at all, so the process is
    /// structurally incapable of authoring an event.
    ///
    /// A zero-verb agent authors nothing anyway, which makes a backfill
    /// rehearsal against real data safe *by accident*. This turns it into a
    /// property: the writer does not exist, so no later code path can quietly
    /// start writing. It is what makes a full-scale timing run against the live
    /// box something other than a leap of faith.
    read_only: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args { read_only: false };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--read-only" => args.read_only = true,
            other => return Err(format!("unrecognised argument: {other}")),
        }
    }
    Ok(args)
}

fn default_data_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("omni-me-agent")
}

fn server_url_policy() -> ServerUrlPolicy<'static> {
    ServerUrlPolicy {
        env_var: SERVER_URL_ENV,
        url_file: SERVER_URL_FILE,
        build_default: DEFAULT_SERVER_URL,
        non_production_default: NON_PRODUCTION_SERVER_URL,
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "omni_me_agent=info,omni_me_core=info".into()),
        )
        .init();

    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}\n\nusage: omni-me-agent [--read-only]");
            std::process::exit(2);
        }
    };

    if let Err(e) = run(args).await {
        tracing::error!(error = %e, "agent failed to start");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), String> {
    let (data_dir, non_production) = runtime::resolve_app_data(default_data_dir(), DATA_DIR_ENV);
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| format!("could not create {data_dir:?}: {e}"))?;

    let db_path = data_dir.join(DB_NAME);
    let db = db::connect(db_path.to_str().ok_or("data dir is not valid UTF-8")?)
        .await
        .map_err(|e| format!("could not open {db_path:?}: {e}"))?;

    let device_id =
        runtime::load_or_create(&data_dir, DEVICE_ID_FILE, || ulid::Ulid::new().to_string());
    let server_url = server_url_policy().resolve(&data_dir, non_production);
    let server_token = runtime::load_or_create(&data_dir, SERVER_TOKEN_FILE, String::new);

    // `has_server_token`, never the token itself.
    tracing::info!(
        device_id = %device_id,
        server_url = %server_url,
        has_server_token = !server_token.trim().is_empty(),
        data_dir = %data_dir.display(),
        non_production,
        read_only = args.read_only,
        "agent initialized"
    );

    let sync_client =
        SyncClient::new(server_url.clone(), device_id.clone()).with_token(&server_token);

    // Learn the config BEFORE deciding which projections to run.
    //
    // The shared config layer is itself a projection, so on a cold start it does
    // not exist yet and every feature falls back to its default — which is `on`.
    // That default is right for a client, where a person installed the app and
    // expects it to work; it is wrong for a deployed process, which would spend
    // its first boot materializing features nobody asked for. A client gets away
    // with it because a fresh install is rare. For an agent — new container, new
    // volume — a cold start is the *normal* case.
    //
    // So: pull the raw log, fold it through the never-gated projections only,
    // then re-read the config and build the real list against the answer. The
    // cost is that a cold start reads the log twice out of the local database.
    let config = learn_config(&db, &sync_client).await?;

    // `JournalFile` is excluded structurally rather than left to the finances
    // switch — see `registry::build_projections_headless` for why the config
    // filter alone cannot be trusted on this path.
    let registered = registry::build_projections_headless(&config);
    tracing::info!(
        projections = ?registered.iter().map(|p| p.name()).collect::<Vec<_>>(),
        features = ?omni_me_core::config::ALL_FEATURES
            .iter()
            .filter(|f| config.enabled(**f))
            .map(|f| f.label())
            .collect::<Vec<_>>(),
        "registered projections"
    );
    let projections = ProjectionRunner::new(db.clone(), registered);
    // `init_all` catches each projection up from its own watermark, so the
    // projections built here replay the log the pull above just landed.
    projections
        .init_all()
        .await
        .map_err(|e| format!("could not initialize projections: {e}"))?;

    let (push_debouncer, _pusher) = PushDebouncer::spawn(sync_client.clone(), db.clone());
    let (retry_engine, _retry) =
        RetryEngine::spawn(sync_client.clone(), db.clone(), &push_debouncer);
    let (network_monitor, _net) = NetworkMonitor::spawn(sync::probe_target(&server_url));
    let _accel = wire_accelerator(&network_monitor, retry_engine.clone());

    // MUST stay below `push_debouncer`: a writer built before it exists appends
    // without waking the pusher, and `pusher::run_loop` has no interval fallback
    // — that is a no-sync bug, not a slow-sync one.
    let _writer: Option<Arc<EventWriter>> = if args.read_only {
        tracing::warn!("--read-only: no writer constructed; this process cannot author events");
        None
    } else {
        Some(Arc::new(
            EventWriter::from_config(
                Arc::new(SurrealEventStore::new(db.clone())) as Arc<dyn EventStore>,
                projections.clone(),
                &config,
                device_id.clone(),
            )
            .with_push_debouncer(push_debouncer.clone()),
        ))
    };

    // Inbound half: startup backfill, then interval and network-online pulls. On
    // a cold start this batch is the entire log arriving.
    let (pull_scheduler, _pull) =
        PullScheduler::spawn(sync_client, db.clone(), projections.clone());
    let _pull_net = wire_puller_network(&network_monitor, pull_scheduler.clone());

    // Diagnostic only, and non-fatal: warns on the orphan signature (local events
    // authored under a non-bound id with no successful pull).
    match omni_me_core::sync::audit_device_ids(&db, &device_id).await {
        Ok(audit) => tracing::info!(?audit, "device id audit"),
        Err(e) => tracing::warn!(error = %e, "device_id audit failed"),
    }

    tracing::info!("agent running; no verbs registered");
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("could not listen for shutdown: {e}"))?;
    tracing::info!("shutting down");
    Ok(())
}

/// Resolve the config this agent should run under, pulling first if it has to.
///
/// Reads the materialized `app_config` table. If that is empty — a cold start —
/// it pulls the log and folds it through the never-gated projections, which is
/// the minimum needed for `ConfigProjection` to materialize the shared layer,
/// then reads again.
///
/// Every failure here degrades to "use what we have" rather than refusing to
/// boot. An agent that will not start because the box is unreachable is worse
/// than one that starts with default features and corrects itself on the next
/// tick — the pull scheduler runs regardless.
async fn learn_config(db: &Database, client: &SyncClient) -> Result<ResolvedConfig, String> {
    let resolve = |global| ResolvedConfig::new(global, Default::default());

    let persisted = load_persisted(db).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "could not read shared config; using defaults");
        Default::default()
    });
    if !persisted.is_empty() {
        return Ok(resolve(persisted));
    }

    tracing::info!("no shared config yet — pulling before choosing projections");
    match client.pull_only(db).await {
        Ok(outcome) => tracing::info!(pulled = outcome.pulled, "cold-start pull complete"),
        Err(e) => {
            // Not fatal: with no config we fall back to defaults, and the pull
            // scheduler will retry. Loud, because it means this boot is running
            // on defaults rather than on the user's real settings.
            tracing::warn!(error = %e, "cold-start pull failed; booting on default features");
            return Ok(resolve(Default::default()));
        }
    }

    let config_only = ProjectionRunner::new(
        db.clone(),
        registry::build_projections_headless(&resolve(Default::default()))
            .into_iter()
            .filter(|p| registry::NEVER_GATED.contains(&p.name()))
            .collect(),
    );
    config_only
        .init_all()
        .await
        .map_err(|e| format!("could not fold the config pre-pass: {e}"))?;

    let global = load_persisted(db).await.unwrap_or_default();
    if global.is_empty() {
        tracing::info!("log carries no config events; default features apply");
    }
    Ok(resolve(global))
}
