//! omni-me agent — a headless device.
//!
//! Own process, own database, own device id, syncing over HTTP exactly as a
//! phone does. The rationale for the device shape (rather than a module inside
//! the server) is in `docs/src/assistant.md` § "The assistant is a device, not a
//! feature"; the short version is that the embedded store is opened per process,
//! and the shape that forces is also the one worth wanting — contained failure,
//! inherited machinery, proposals that sync for free.
//!
//! **It answers questions.** A question authored on any device arrives here the
//! ordinary way — pulled, projected — and the answer goes back the same way. That
//! is not an implementation detail: `surrealkv` holds an exclusive lock on its
//! directory, so a resident agent's database cannot be opened by a second process
//! to hand it work. Carrying the question as an event is what makes *resident*
//! and *askable* the same process. The loop itself is in [`responder`].
//!
//! Two flags stand for the two postures, and they are opposites: `--read-only`
//! refuses to build a writer at all, `--probe` authors one note through the one
//! it built. Both answer a question about a *host* rather than about the code,
//! which is why they live here and not in a test.

mod ask;
mod bench;
mod responder;
mod retrieval_bench;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use omni_me_core::config::{ConfigKey, ConfigMap, ConfigValue, ResolvedConfig};
use omni_me_core::db::{self, Database};
use omni_me_core::events::{
    AssistantQuestionAskedPayload, EventStore, EventWriter, NewEvent, ProjectionRunner,
    SurrealEventStore, load_persisted, registry,
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

/// Where to read `[llm]` from, overriding the XDG default.
///
/// The server has no such override because it runs where its config lives. The
/// agent is developed against a checked-out repo whose gitignored
/// `secrets/credentials.toml` holds the key, so it needs one.
const CREDENTIALS_ENV: &str = "OMNI_AGENT_CREDENTIALS";

/// Provider-specific request fields, as raw JSON.
///
/// Exists for gateways that pick an upstream for you: OpenRouter needs
/// `{"provider":{"order":["deepinfra"],"allow_fallbacks":false}}` or a silent
/// reroute makes a benchmark unattributable — you learn that *something*
/// answered, not whose serving stack did, and the constrained-decoding result
/// this phase measures is a fact about a stack.
const LLM_EXTRA_BODY_ENV: &str = "OMNI_AGENT_LLM_EXTRA_BODY";

/// Minimum milliseconds between model requests.
///
/// ⚠️ On a rate-capped endpoint this is a **correctness** setting, not politeness.
/// One question makes several calls and `--bench` makes dozens; without spacing,
/// a free tier's cap returns rate-limit errors that a scorecard would then report
/// as the model failing. For a ten-per-minute tier, 7000 leaves retry headroom.
const LLM_MIN_INTERVAL_ENV: &str = "OMNI_AGENT_LLM_MIN_INTERVAL_MS";

/// Point the agent at a different endpoint without editing `credentials.toml`.
///
/// `--bench` has to run against more than one endpoint — the constraint tax can
/// only be read off a stack that can serve both halves — and the alternative is
/// hand-editing the credentials file per run and remembering to change it back.
/// Leaving it pointed at a benchmark endpoint afterwards is the failure mode
/// these exist to remove.
const LLM_BASE_URL_ENV: &str = "OMNI_AGENT_LLM_BASE_URL";
const LLM_MODEL_ENV: &str = "OMNI_AGENT_LLM_MODEL";
const LLM_API_KEY_ENV: &str = "OMNI_AGENT_LLM_API_KEY";

/// Override the assistant's turn budget for one run.
///
/// The quality-against-cost balance is found by trying, and a tuning loop
/// runs from here rather than from a phone's settings screen. Injected as the
/// **device layer** of the resolved config (see [`learn_config`]) so it lands
/// in the same resolution and the same clamp as a value set from Settings —
/// an override that bypassed `int_of` could set a budget the app itself would
/// refuse.
const MAX_TURNS_ENV: &str = "OMNI_AGENT_MAX_TURNS";

/// How long the agent waits between sync pulls, in milliseconds.
///
/// ⚠️ Deliberately **not** [`sync::DEFAULT_PULL_INTERVAL`]'s 20s. That default is
/// sized for a phone: a radio, a battery, and a person who does not notice a
/// twenty-second lag on a note edit. This process sits on the same host as the
/// server it polls, on mains power, and its pull latency is a limb of the
/// interactive path — a question waits here before anything else can happen to
/// it. See [`DEFAULT_PULL_INTERVAL_MS`].
const PULL_INTERVAL_ENV: &str = "OMNI_AGENT_PULL_INTERVAL_MS";

/// Minutes after which an unanswered question is closed without a model call.
///
/// An operational safety valve, not a preference — which is why it is an env var
/// rather than a `ConfigKey`. The `assistant.*` config keys are things a person
/// tunes from the settings screen (which embedder, whether to rerank); nobody
/// adjusts a staleness horizon from a phone, and making it the first `Int` key
/// would drag an `int_of` accessor and a numeric settings widget in behind it.
const ANSWER_HORIZON_ENV: &str = "OMNI_AGENT_ANSWER_HORIZON_MINS";

/// 3 seconds: fast enough that the agent's pull is not the dominant term in
/// ask-to-answer latency, cheap because the request is loopback to a process on
/// the same host.
const DEFAULT_PULL_INTERVAL_MS: u64 = 3_000;

/// 60 minutes. Long enough that a brief restart answers everything it missed,
/// short enough that an overnight outage does not wake up and pay for a day of
/// questions the user has already given up on.
const DEFAULT_ANSWER_HORIZON_MINS: u64 = 60;

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

    /// Author one throwaway note through the real writer, then keep running.
    ///
    /// The mirror of [`Args::read_only`]: that one proves this process *cannot*
    /// write, this one proves it *can*. A cold start that backfills correctly
    /// says nothing about the write half, and the write half is the one whose
    /// wiring fails silently — a writer built above the push debouncer appends
    /// happily and never syncs, because `pusher::run_loop` has no interval
    /// fallback to cover for the missing nudge.
    ///
    /// ⚠️ This writes a real note into whatever log it is pointed at. That is a
    /// throwaway hub, never a real one.
    probe: bool,

    /// Put one question through the assistant, print the trace, and exit.
    ///
    /// ⚠️ **Test scaffolding, not ops surface** — unlike [`Args::read_only`] and
    /// [`Args::probe`], which answer questions about a *host* and are permanent.
    /// This exists because the verbs have no real interface yet; the chat surface
    /// that replaces it is separate, later work. Do not build tooling on it.
    ask: Option<String>,

    /// Score verb selection over fixed cases, free-form and schema-constrained,
    /// and report the difference — the **constraint tax**, as a measured number
    /// rather than an assumption.
    ///
    /// ⚠️ Test scaffolding, on the same terms as [`Args::ask`].
    bench: bool,

    /// Re-embed every record, even ones whose text has not changed.
    ///
    /// The sweep is content-hashed, so the only way to rebuild after changing the
    /// embedding model — or after a corrupted index — is to clear it first. Not a
    /// routine operation: on a real corpus it re-runs the model over everything.
    reindex: bool,

    /// Run [`Args::ask`] under grammar-constrained decoding.
    ///
    /// Exists so the constrained half can be tried **once** before `--bench`
    /// tries it a hundred times. Two things can only be settled against a real
    /// endpoint: whether it accepts the verb-call schema at all, and whether a
    /// constrained reply can reach the `answer` exit rather than re-calling
    /// until the turn budget runs out. Both have already produced a scorecard
    /// that measured the harness, and both cost one request to rule out.
    constrained: bool,

    /// Score retrieval itself — keyword, fused, and fused-plus-reranked — over a
    /// fixture corpus with known right answers, and report cost per model.
    ///
    /// Separate from [`Args::bench`] rather than a mode of it, because the two
    /// measure different machines. That one scores an LLM over a network and
    /// needs credentials, a rate limit and a chosen endpoint; this one is local,
    /// deterministic and needs none of them. Folding them together would make the
    /// retrieval number cost tokens to obtain.
    ///
    /// ⚠️ Test scaffolding, on the same terms as [`Args::ask`].
    bench_retrieval: bool,

    /// Author a question event, then exit — a stand-in for the client.
    ///
    /// Distinct from [`Args::ask`] in the thing that matters: that one calls the
    /// model in this process, this one writes an event and lets whichever agent
    /// is *resident* pick it up. It is how the ask/answer path gets exercised
    /// end-to-end before the Assistant tab exists, and it exercises the real
    /// path rather than a shortcut through it — two processes, separate data
    /// roots, one hub, exactly as a phone and the box relate.
    ///
    /// ⚠️ Writes a real event into whatever log it is pointed at.
    ask_event: Option<String>,

    /// Continue an existing thread instead of starting one. Only meaningful
    /// with [`Args::ask_event`].
    thread: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        read_only: false,
        probe: false,
        ask: None,
        bench: false,
        constrained: false,
        reindex: false,
        bench_retrieval: false,
        ask_event: None,
        thread: None,
    };
    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--read-only" => args.read_only = true,
            "--probe" => args.probe = true,
            "--bench" => args.bench = true,
            "--bench-retrieval" => args.bench_retrieval = true,
            "--constrained" => args.constrained = true,
            "--reindex" => args.reindex = true,
            "--ask" => {
                let question = argv
                    .next()
                    .ok_or_else(|| "--ask needs a question".to_string())?;
                if question.trim().is_empty() {
                    return Err("--ask needs a non-empty question".to_string());
                }
                args.ask = Some(question);
            }
            "--ask-event" => {
                let question = argv
                    .next()
                    .ok_or_else(|| "--ask-event needs a question".to_string())?;
                if question.trim().is_empty() {
                    return Err("--ask-event needs a non-empty question".to_string());
                }
                args.ask_event = Some(question);
            }
            "--thread" => {
                args.thread = Some(
                    argv.next()
                        .ok_or_else(|| "--thread needs a thread id".to_string())?,
                );
            }
            other => return Err(format!("unrecognised argument: {other}")),
        }
    }
    // A contradiction rather than a preference, so it is refused rather than
    // resolved: read-only builds no writer, and probing needs one.
    if args.probe && args.read_only {
        return Err("--probe needs a writer; --read-only refuses to build one".to_string());
    }
    if args.ask.is_some() && args.bench {
        return Err("--ask and --bench are separate runs; pick one".to_string());
    }
    // Each one-shot mode exits when it is done, so a second would never run. A
    // refusal beats silently honouring whichever the branch happens to test first.
    if args.bench_retrieval && (args.bench || args.ask.is_some()) {
        return Err("--bench-retrieval is its own run; pick one".to_string());
    }
    // With `--bench` this means **bench the constrained arm only**, and it is
    // deliberate rather than a mistake: some endpoints offer `response_format`
    // and no `tools` parameter at all, so the free-form arm cannot be run there
    // and a both-arms bench aborts in pre-flight instead of measuring anything.
    // DeepInfra's Llama-4 endpoints are the case that forced this.
    //
    // This combination used to be refused, on the grounds that it would silently
    // halve the measurement. That concern was right and is now answered by the
    // scorecard rather than by the ban: a constrained-only run says so in its
    // header and reports the tax as NOT APPLICABLE, naming the omission as
    // chosen. Silence was the problem, not the halving.
    if args.constrained && args.ask.is_none() && !args.bench {
        return Err("--constrained applies to --ask or --bench".to_string());
    }
    // Same contradiction as `--probe --read-only`, and refused rather than
    // resolved for the same reason: authoring needs a writer.
    if args.ask_event.is_some() && args.read_only {
        return Err("--ask-event needs a writer; --read-only refuses to build one".to_string());
    }
    // `--ask` answers in this process; `--ask-event` hands the question to
    // whichever agent is resident. Running both would ask the same thing twice
    // and pay twice.
    if args.ask_event.is_some() && (args.ask.is_some() || args.bench || args.bench_retrieval) {
        return Err("--ask-event is its own run; pick one".to_string());
    }
    if args.thread.is_some() && args.ask_event.is_none() {
        return Err("--thread applies to --ask-event".to_string());
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
            eprintln!(
                "{e}\n\nusage: omni-me-agent [--read-only | --probe]\n       \
                 omni-me-agent [--reindex]\n       \
                 omni-me-agent --ask-event \"<question>\" [--thread <id>]\n       \
                 omni-me-agent --ask \"<question>\" [--constrained]   (test scaffolding)\n       \
                 omni-me-agent --bench                              (test scaffolding)\n       \
                 omni-me-agent --bench-retrieval                    (test scaffolding)"
            );
            std::process::exit(2);
        }
    };

    // ⚠️ **Above `run`, deliberately.** `run` connects to the real database and
    // sweeps the real corpus into the vector index before it reaches any
    // one-shot branch. For every other mode that is what you want; for this one
    // it would embed the whole journal to answer a question about a fixture, and
    // do it on a machine chosen for having spare memory rather than for holding
    // the data. The retrieval bench needs a model cache and nothing else.
    if args.bench_retrieval {
        retrieval_bench::run(model_cache_dir()).await;
        return;
    }

    if let Err(e) = run(args).await {
        tracing::error!(error = %e, "agent failed to start");
        std::process::exit(1);
    }
}

/// Build the assistant's LLM client from `[llm]`.
///
/// Goes through `omni_me_core::llm::build_llm_client` rather than constructing a
/// client here, so the agent and the server cannot disagree about which provider
/// a config selects.
fn build_assistant_llm() -> Result<std::sync::Arc<dyn omni_me_core::llm::LlmClient>, String> {
    let path = match std::env::var(CREDENTIALS_ENV) {
        Ok(p) => PathBuf::from(p),
        Err(_) => omni_me_core::credentials::default_path()
            .map_err(|e| format!("could not locate credentials: {e}"))?,
    };
    let mut creds = omni_me_core::credentials::load(&path)
        .map_err(|e| format!("could not read credentials at {}: {e}", path.display()))?;

    // Env overrides win over the file. Setting any of them means the caller is
    // deliberately pointing this run somewhere else, so an override also selects
    // the OpenAI-compatible provider — otherwise a base URL would be set and
    // silently ignored because `[llm]` still named another provider.
    let overrides = [LLM_BASE_URL_ENV, LLM_MODEL_ENV, LLM_API_KEY_ENV]
        .iter()
        .any(|k| std::env::var(k).is_ok());
    if overrides {
        let llm = creds
            .llm
            .get_or_insert_with(|| omni_me_core::credentials::LlmProviderConfig {
                provider: "openai_compatible".to_string(),
                base_url: None,
                model: None,
                api_key: None,
                vision: false,
                allow_closed_weights: false,
                // Per-role overrides are never synthesised from env: these env
                // vars configure the agent's own endpoint (role A), and a role
                // table invented here would silently outrank the credentials
                // file the user actually wrote.
                ..Default::default()
            });
        llm.provider = "openai_compatible".to_string();
        if let Ok(v) = std::env::var(LLM_BASE_URL_ENV) {
            llm.base_url = Some(v);
        }
        if let Ok(v) = std::env::var(LLM_MODEL_ENV) {
            llm.model = Some(v);
        }
        if let Ok(v) = std::env::var(LLM_API_KEY_ENV) {
            llm.api_key = Some(v);
        }
        // The model, never the key or the URL — a base URL can carry a key.
        tracing::info!(model = ?llm.model, "LLM endpoint overridden by environment");
    }

    let options = omni_me_core::llm::ClientOptions {
        extra_body: match std::env::var(LLM_EXTRA_BODY_ENV) {
            Ok(raw) => Some(
                serde_json::from_str(&raw)
                    .map_err(|e| format!("{LLM_EXTRA_BODY_ENV} is not valid JSON: {e}"))?,
            ),
            Err(_) => None,
        },
        min_interval: match std::env::var(LLM_MIN_INTERVAL_ENV) {
            Ok(raw) => Some(std::time::Duration::from_millis(
                raw.parse()
                    .map_err(|e| format!("{LLM_MIN_INTERVAL_ENV} is not a number: {e}"))?,
            )),
            Err(_) => None,
        },
    };

    Ok(omni_me_core::llm::build_llm_client(&creds, options))
}

/// Load the embedding model and define the vector index, or explain why not.
///
/// Returns `None` rather than failing the boot. A model that will not load leaves
/// the agent answering with keyword search — worse, but available — and the warning
/// says which mode it is in. Refusing to start would make a missing 133 MB download
/// look like a broken agent.
/// Where downloaded ONNX weights live.
///
/// Beside the data, not beside the binary: the cache is state, and on a deployed
/// host `.` is not writable while the data volume is. Both models share it, so a
/// re-deploy re-downloads neither.
fn model_cache_dir() -> PathBuf {
    std::env::var("FASTEMBED_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            runtime::resolve_app_data(default_data_dir(), DATA_DIR_ENV)
                .0
                .join("models")
        })
}

async fn build_embedder(
    db: &Database,
    config: &ResolvedConfig,
) -> Option<omni_me_core::assistant::Embedder> {
    use omni_me_core::assistant::Embedder;

    let cache = model_cache_dir();

    // Blocking: model load reads hundreds of MB and, on a cold cache, downloads it.
    let model = config.text_of(ConfigKey::AssistantEmbedModel);
    let loaded = tokio::task::spawn_blocking(move || Embedder::load(&model, cache))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));

    let embedder = match loaded {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "no embedding model; search will be keyword-only");
            return None;
        }
    };

    if let Err(e) = omni_me_core::assistant::vector_store::init_schema(db, embedder.dim()).await {
        tracing::warn!(error = %e, "could not define the vector index; keyword-only");
        return None;
    }
    tracing::info!(
        model = embedder.name(),
        dim = embedder.dim(),
        "embedder ready"
    );
    Some(embedder)
}

/// Load the cross-encoder, if this host is configured to run one.
///
/// ⚠️ **`None` here is two different situations and the log must say which.** Off
/// by config is a choice; failed to load is a degradation. Both leave retrieval
/// working, which is exactly why the second one would otherwise go unnoticed.
async fn build_reranker(config: &ResolvedConfig) -> Option<omni_me_core::assistant::Reranker> {
    use omni_me_core::assistant::Reranker;

    if !config.bool_of(ConfigKey::AssistantRerank) {
        tracing::info!("reranking is off; results keep their fused order");
        return None;
    }

    let cache = model_cache_dir();
    let model = config.text_of(ConfigKey::AssistantRerankModel);
    let loaded = tokio::task::spawn_blocking(move || Reranker::load(&model, cache))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));

    match loaded {
        Ok(r) => {
            tracing::info!(model = r.name(), "reranker ready");
            Some(r)
        }
        Err(e) => {
            tracing::warn!(error = %e, "no reranking model; results keep their fused order");
            None
        }
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

    // Retrieval is built AFTER `init_all`, never before: the sweep reads the
    // materialized record tables, so running it against a log that has not been
    // folded yet would index an empty corpus and report success.
    let embedder = build_embedder(&db, &config).await;
    let reranker = build_reranker(&config).await;

    if let Some(embedder) = embedder.as_ref() {
        if args.reindex {
            match omni_me_core::assistant::vector_store::clear(&db).await {
                Ok(n) => tracing::info!(removed = n, "--reindex: cleared the vector index"),
                Err(e) => tracing::warn!(error = %e, "could not clear the vector index"),
            }
        }
        match omni_me_core::assistant::vector_store::sweep(&db, &config, embedder).await {
            Ok(report) => tracing::info!(?report, "vector index up to date"),
            // Non-fatal by design: an agent that answers with keyword search only
            // is degraded, and one that refuses to boot is unavailable. The report
            // above is what says which of the two happened.
            Err(e) => tracing::warn!(error = %e, "sweep failed; keyword retrieval only"),
        }
    }

    let (push_debouncer, _pusher) = PushDebouncer::spawn(sync_client.clone(), db.clone());
    let (retry_engine, _retry) =
        RetryEngine::spawn(sync_client.clone(), db.clone(), &push_debouncer);
    let (network_monitor, _net) = NetworkMonitor::spawn(sync::probe_target(&server_url));
    let _accel = wire_accelerator(&network_monitor, retry_engine.clone());

    // MUST stay below `push_debouncer`: a writer built before it exists appends
    // without waking the pusher, and `pusher::run_loop` has no interval fallback
    // — that is a no-sync bug, not a slow-sync one.
    let writer: Option<Arc<EventWriter>> = if args.read_only {
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

    // `parse_args` refuses `--probe --read-only`, so the writer is always `Some`
    // here when probing.
    if let (true, Some(writer)) = (args.probe, writer.as_ref()) {
        probe_once(writer).await;
    }

    // Authoring a question is a client's job, so it exits like a client would —
    // before any of the answering machinery below starts. The resident agent is
    // a different process against a different data root.
    if let Some(question) = &args.ask_event {
        let writer = writer.as_ref().ok_or("--ask-event needs a writer")?;
        ask_event_once(writer, question, args.thread.as_deref()).await?;
        // Nudged, then given the debounce window: `run` returning drops the
        // pusher, and a question that never left the process is not a question.
        tokio::time::sleep(omni_me_core::sync::DEFAULT_PUSH_DELAY + Duration::from_secs(3)).await;
        return Ok(());
    }

    // One-shot diagnostics run against the log as it stands and then exit,
    // rather than starting the schedulers. They pull once first: asking about
    // records this device has not seen would answer "not there" for data that
    // exists, which looks like a retrieval failure and is not one.
    if args.ask.is_some() || args.bench {
        match sync_client.pull_only(&db).await {
            Ok(outcome) => {
                tracing::info!(pulled = outcome.pulled, "pull before diagnostics");
                if let Err(e) = projections.init_all().await {
                    tracing::warn!(error = %e, "could not project the pulled events");
                }
            }
            // Not fatal: a local log may already hold what the question is about,
            // and refusing to answer at all would be worse than answering from
            // slightly stale data — as long as it is said out loud.
            Err(e) => tracing::warn!(error = %e, "pull failed; answering from local data only"),
        }

        let llm = build_assistant_llm()?;
        tracing::info!(model = llm.model_name(), "assistant model");
        // Built here rather than in a helper: `Retrievers` borrows both services,
        // so a function returning one would be returning references to its own
        // locals. The concrete values have to live in the scope that uses them.
        let search = embedder
            .as_ref()
            .map(|e| omni_me_core::assistant::VectorSearch {
                db: &db,
                config: &config,
                embedder: e,
            });
        let rerank_service = reranker
            .as_ref()
            .map(|r| omni_me_core::assistant::RerankService { reranker: r });
        let retrievers = omni_me_core::assistant::Retrievers {
            semantic: search
                .as_ref()
                .map(|s| s as &dyn omni_me_core::assistant::SemanticSearch),
            reranker: rerank_service
                .as_ref()
                .map(|s| s as &dyn omni_me_core::assistant::Rerank),
        };
        if let Some(question) = &args.ask {
            ask::run(
                &db,
                &config,
                llm.as_ref(),
                question,
                args.constrained,
                retrievers,
            )
            .await;
        } else {
            bench::run(&db, &config, llm.as_ref(), args.constrained).await;
        }
        return Ok(());
    }

    // Inbound half: startup backfill, then interval and network-online pulls. On
    // a cold start this batch is the entire log arriving.
    //
    // ⚠️ `spawn_with`, not `spawn`: the default 20s interval is a phone's, and
    // here it would sit in the middle of the ask-to-answer path. See
    // `PULL_INTERVAL_ENV`.
    let pull_interval = Duration::from_millis(env_u64(PULL_INTERVAL_ENV, DEFAULT_PULL_INTERVAL_MS));
    let (pull_scheduler, _pull) = PullScheduler::spawn_with(
        sync_client,
        db.clone(),
        projections.clone(),
        pull_interval,
        sync::DEFAULT_PULL_WARMUP,
    );
    let _pull_net = wire_puller_network(&network_monitor, pull_scheduler.clone());

    // Diagnostic only, and non-fatal: warns on the orphan signature (local events
    // authored under a non-bound id with no successful pull).
    match omni_me_core::sync::audit_device_ids(&db, &device_id).await {
        Ok(audit) => tracing::info!(?audit, "device id audit"),
        Err(e) => tracing::warn!(error = %e, "device_id audit failed"),
    }

    // A read-only agent has nothing to answer *with*: an answer is an event, and
    // it cannot author one. Say so once, plainly, rather than letting it look
    // like a working assistant that never replies.
    let Some(writer) = writer.as_ref() else {
        tracing::warn!(
            "--read-only: questions will not be answered; this process cannot author events"
        );
        tokio::signal::ctrl_c()
            .await
            .map_err(|e| format!("could not listen for shutdown: {e}"))?;
        tracing::info!("shutting down");
        return Ok(());
    };

    let llm = build_assistant_llm()?;
    // Built here rather than in a helper for the same reason the diagnostics
    // branch does it: `Retrievers` borrows both services, so a function returning
    // one would be returning references to its own locals.
    let search = embedder
        .as_ref()
        .map(|e| omni_me_core::assistant::VectorSearch {
            db: &db,
            config: &config,
            embedder: e,
        });
    let rerank_service = reranker
        .as_ref()
        .map(|r| omni_me_core::assistant::RerankService { reranker: r });
    let retrievers = omni_me_core::assistant::Retrievers {
        semantic: search
            .as_ref()
            .map(|s| s as &dyn omni_me_core::assistant::SemanticSearch),
        reranker: rerank_service
            .as_ref()
            .map(|s| s as &dyn omni_me_core::assistant::Rerank),
    };
    let horizon = Duration::from_secs(
        env_u64(ANSWER_HORIZON_ENV, DEFAULT_ANSWER_HORIZON_MINS).saturating_mul(60),
    );

    tracing::info!(
        model = llm.model_name(),
        pull_interval_ms = pull_interval.as_millis(),
        horizon_mins = horizon.as_secs() / 60,
        semantic = retrievers.semantic.is_some(),
        reranked = retrievers.reranker.is_some(),
        "agent running; answering questions"
    );

    let responder = responder::Responder {
        db: &db,
        config: &config,
        llm: llm.as_ref(),
        writer,
        retrievers,
        horizon,
    };
    match responder.run(&pull_scheduler).await {
        responder::Stopped::Interrupted => Ok(()),
    }
}

/// Read a positive integer out of the environment, or fall back.
///
/// A malformed value warns and falls back rather than refusing to boot. These
/// are deployment knobs; an agent that will not start because a compose file has
/// a typo in a poll interval is worse than one that starts on the default and
/// says so.
fn env_u64(var: &str, default: u64) -> u64 {
    match std::env::var(var) {
        Err(_) => default,
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(v) if v > 0 => v,
            _ => {
                tracing::warn!(var, value = %raw, default, "unusable value; using the default");
                default
            }
        },
    }
}

/// Author one question event and report where it landed.
///
/// The client's half of the interface, standing in for the Assistant tab. It
/// mints a thread id when none is given, because a bare question is the start of
/// a conversation — continuing one is the deliberate act, which is what `--thread`
/// is for.
async fn ask_event_once(
    writer: &EventWriter,
    question: &str,
    thread: Option<&str>,
) -> Result<(), String> {
    let thread_id = thread
        .map(str::to_string)
        .unwrap_or_else(|| ulid::Ulid::new().to_string());
    let message_id = ulid::Ulid::new().to_string();
    let payload = AssistantQuestionAskedPayload {
        thread_id: thread_id.clone(),
        message_id: message_id.clone(),
        text: question.to_string(),
        // Only a thread's first question titles it. A `--thread` continuation is
        // by definition not the first.
        title: thread.is_none().then(|| title_from(question)),
        // `--ask` is a person at a terminal, which is the opposite of scheduled
        // however headless the binary is.
        scheduled: false,
    };
    let event = NewEvent::assistant_question_asked(writer.device_id(), &payload)
        .map_err(|e| format!("could not build the question event: {e}"))?;

    let stored = writer
        .append_new(event)
        .await
        .map_err(|e| format!("question refused: {e}"))?;
    tracing::info!(
        thread = %thread_id,
        message_id = %message_id,
        event_id = %stored.id,
        "question authored; a resident agent should answer it"
    );
    println!("thread {thread_id}");
    Ok(())
}

/// A thread's title: the question, trimmed to something a list can show.
///
/// Cut on a word boundary rather than mid-word — a list of threads is scanned,
/// and a truncated word reads as corruption before it reads as an ellipsis.
fn title_from(question: &str) -> String {
    const MAX: usize = 60;
    let q = question.trim();
    if q.chars().count() <= MAX {
        return q.to_string();
    }
    let head: String = q.chars().take(MAX).collect();
    let cut = head.rfind(char::is_whitespace).unwrap_or(head.len());
    format!("{}…", head[..cut].trim_end())
}

/// Author one note through the real writer and report what happened.
///
/// Goes through [`EventWriter`] rather than the store on purpose: the point is
/// to exercise the guard, the projection fold and the push nudge in the order
/// *this process* wires them, which is the half no unit test can reach — a test
/// constructs its own writer and so cannot catch a mistake in `run`'s ordering.
///
/// Non-fatal either way, because a refusal is a **result**, not a failure: with
/// the feature that owns notes switched off, being refused here is the correct
/// outcome and the one worth confirming.
async fn probe_once(writer: &EventWriter) {
    let note_id = ulid::Ulid::new().to_string();
    let event = NewEvent::generic_note_created(
        writer.device_id(),
        &note_id,
        "agent probe",
        "Written by `omni-me-agent --probe` to prove this host's write path.",
        None,
    );

    match writer.append_new(event).await {
        Ok(stored) => tracing::info!(
            event_id = %stored.id,
            device_id = %stored.device_id,
            note_id = %note_id,
            "probe authored; the pusher should report it shortly"
        ),
        Err(e) => tracing::warn!(error = %e, "probe refused"),
    }
}

/// Env overrides for this run, as the config's device layer.
///
/// ⚠️ An unparseable value is **named, not ignored**. Silently falling back to
/// the configured budget during a tuning run would attribute the resulting
/// numbers to a setting that was never applied.
fn device_overrides() -> ConfigMap {
    let mut device = ConfigMap::new();
    if let Ok(raw) = std::env::var(MAX_TURNS_ENV) {
        match raw.trim().parse::<i64>() {
            Ok(n) => {
                device.insert(ConfigKey::AssistantMaxTurns, ConfigValue::Int(n));
                tracing::info!(turns = n, "turn budget overridden from the environment");
            }
            Err(_) => tracing::warn!(
                value = %raw,
                "{MAX_TURNS_ENV} is not a whole number; using the configured budget"
            ),
        }
    }
    device
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
    let resolve = |global| ResolvedConfig::new(global, device_overrides());

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
