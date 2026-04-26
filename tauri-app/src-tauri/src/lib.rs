mod auto_close_scheduler;
mod commands;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::Manager;

use omni_me_core::db::{self, Database};
use omni_me_core::events::{
    NotesProjection, ProjectionRunner, RoutinesProjection, SurrealEventStore,
};
use omni_me_core::sync::{
    NetworkMonitor, PushDebouncer, RetryEngine, StatusReporter, SyncBuffer, SyncClient,
    wire_accelerator,
};

const DB_NAME: &str = "local.db";
const DEVICE_ID_FILE: &str = "device_id";
const SERVER_URL_FILE: &str = "server_url";
const DEFAULT_SERVER_URL: &str = "http://localhost:3000";
const TIMEZONE_FILE: &str = "timezone";

/// Load a string value from a file, or use a default and persist it.
fn load_or_create(app_data: &Path, filename: &str, default_fn: impl FnOnce() -> String) -> String {
    let path = app_data.join(filename);
    if let Ok(val) = std::fs::read_to_string(&path) {
        let val = val.trim().to_string();
        if !val.is_empty() {
            return val;
        }
    }
    let val = default_fn();
    let _ = std::fs::write(&path, &val);
    val
}

pub struct AppState {
    pub db: Database,
    pub event_store: SurrealEventStore,
    pub projections: ProjectionRunner,
    pub device_id: String,
    pub server_url: tokio::sync::RwLock<String>,
    pub timezone: Arc<tokio::sync::RwLock<String>>,
    pub app_data_dir: std::path::PathBuf,
    pub http: reqwest::Client,
    /// Debounced event buffer — 1s idle flush (see `SyncBuffer`).
    pub sync_buffer: SyncBuffer,
    /// Debounced push orchestrator — 2s idle after buffer flush.
    pub push_debouncer: PushDebouncer,
    /// Retry engine — exponential backoff 1s → 60s.
    pub retry_engine: RetryEngine,
    /// OS network event monitor — edge-triggered Online/Offline hints.
    pub network_monitor: NetworkMonitor,
    /// Aggregated sync status reporter.
    pub status_reporter: StatusReporter,
    /// Canonical root of the most recently scanned vault. `commit_import`
    /// refuses to ingest any path that doesn't sit under this root, so the
    /// frontend can't redirect commit reads to arbitrary files on disk.
    pub last_import_root: tokio::sync::Mutex<Option<PathBuf>>,
}

/// Derive a TCP probe target (`host:port`) from the sync server URL. Used by
/// the Phase 2 `NetworkMonitor` to hint the retry engine when the server
/// becomes reachable again. Falls back to the URL's bare host on parse
/// failures; callers may still wire the monitor to this even if the target
/// is slightly stale — it only drives retry hints, not correctness.
fn probe_target_from_url(url: &str) -> String {
    if let Ok(parsed) = tauri::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("127.0.0.1");
        let port = parsed
            .port_or_known_default()
            .unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
        return format!("{host}:{port}");
    }
    // Last resort — match the default server URL shape.
    "127.0.0.1:3000".to_string()
}

/// Remove stale SurrealKV LOCK file if the owning process is no longer alive.
/// SurrealKV writes the PID to a LOCK file and doesn't clean it up on unclean
/// shutdown (SIGKILL, crash, etc.), which blocks subsequent opens.
fn clear_stale_lock(db_path: &Path) {
    let lock_path = db_path.join("LOCK");
    if let Ok(contents) = std::fs::read_to_string(&lock_path)
        && let Ok(pid) = contents.trim().parse::<u32>()
    {
        let alive = Path::new(&format!("/proc/{}", pid)).exists();
        if !alive {
            tracing::warn!(pid, "Removing stale SurrealKV LOCK (pid not running)");
            let _ = std::fs::remove_file(&lock_path);
        }
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "omni_me_app=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            // Store DB in the OS app data dir (e.g. ~/.local/share/com.omni-me.app/)
            // instead of inside src-tauri/ where Tauri's file watcher would trigger
            // infinite rebuild loops on every LOCK/WAL write.
            let app_data = app.path().app_data_dir()
                .expect("failed to resolve app data directory");
            std::fs::create_dir_all(&app_data).ok();
            let db_path = app_data.join(DB_NAME);

            clear_stale_lock(&db_path);

            let db_path_str = db_path.to_string_lossy().to_string();
            let handle = app.handle().clone();

            // Run async initialization on the Tauri runtime
            tauri::async_runtime::block_on(async move {
                let db = db::connect(&db_path_str)
                    .await
                    .expect("failed to connect to SurrealDB");

                let event_store = SurrealEventStore::new(db.clone());

                let projections = ProjectionRunner::new(
                    db.clone(),
                    vec![Box::new(NotesProjection), Box::new(RoutinesProjection)],
                );

                projections
                    .init_all()
                    .await
                    .expect("failed to initialize projections");

                let device_id = load_or_create(&app_data, DEVICE_ID_FILE, || {
                    ulid::Ulid::new().to_string()
                });
                let server_url = load_or_create(&app_data, SERVER_URL_FILE, || {
                    std::env::var("OMNI_SERVER_URL").unwrap_or(DEFAULT_SERVER_URL.to_string())
                });
                let timezone = load_or_create(&app_data, TIMEZONE_FILE, || {
                    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string())
                });

                tracing::info!(device_id = %device_id, server_url = %server_url, timezone = %timezone, "App initialized");

                let timezone_shared = Arc::new(tokio::sync::RwLock::new(timezone));

                auto_close_scheduler::spawn(
                    db.clone(),
                    event_store.clone(),
                    projections.clone(),
                    device_id.clone(),
                    timezone_shared.clone(),
                );

                // Phase 2 sync pipeline: buffer -> pusher -> retry engine
                // wired together, plus a network monitor feeding hints in.
                let sync_client = SyncClient::new(server_url.clone(), device_id.clone());
                let (sync_buffer, _buffer_task) = SyncBuffer::new(Arc::new(event_store.clone()));
                let (push_debouncer, _pusher_task) =
                    PushDebouncer::spawn(sync_client.clone(), db.clone(), &sync_buffer);
                let (retry_engine, _retry_task) =
                    RetryEngine::spawn(sync_client.clone(), db.clone(), &push_debouncer);
                let probe_target = probe_target_from_url(&server_url);
                let (network_monitor, _net_task) = NetworkMonitor::spawn(probe_target);
                let _accel_task = wire_accelerator(&network_monitor, retry_engine.clone());
                let (status_reporter, _sr_push_task, _sr_retry_task) =
                    StatusReporter::spawn(&push_debouncer, &retry_engine);

                handle.manage(AppState {
                    db,
                    event_store,
                    projections,
                    device_id,
                    server_url: tokio::sync::RwLock::new(server_url),
                    timezone: timezone_shared,
                    app_data_dir: app_data,
                    http: reqwest::Client::new(),
                    sync_buffer,
                    push_debouncer,
                    retry_engine,
                    network_monitor,
                    status_reporter,
                    last_import_root: tokio::sync::Mutex::new(None),
                });
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Journal entries (date-keyed)
            commands::notes::create_journal_entry,
            commands::notes::update_journal_entry,
            commands::notes::close_journal_entry,
            commands::notes::reopen_journal_entry,
            commands::notes::get_journal_by_date,
            commands::notes::list_journal_entries,
            commands::notes::list_journal_dates,
            // Generic notes (id-keyed)
            commands::notes::create_generic_note,
            commands::notes::update_generic_note,
            commands::notes::rename_generic_note,
            commands::notes::get_generic_note,
            commands::notes::list_generic_notes,
            commands::notes::search_generic_notes,
            // LLM
            commands::notes::process_note_llm,
            // Routine groups
            commands::routines::create_routine_group,
            commands::routines::list_routine_groups,
            commands::routines::reorder_routine_groups,
            commands::routines::remove_routine_group,
            // Routine items
            commands::routines::add_routine_item,
            commands::routines::list_routine_items,
            commands::routines::modify_routine_item,
            commands::routines::remove_routine_item,
            // Routine completions
            commands::routines::complete_routine_item,
            commands::routines::undo_completion,
            commands::routines::skip_routine_item,
            commands::routines::undo_skip,
            commands::routines::get_completions_for_date,
            commands::routines::get_routine_history,
            // Meta
            commands::routines::wipe_all_data,
            // Sync
            commands::sync::trigger_sync,
            commands::sync::get_sync_info,
            commands::sync::update_server_url,
            commands::sync::get_sync_status,
            // Timezone
            commands::timezone::get_timezone,
            commands::timezone::update_timezone,
            // Obsidian import/export
            commands::import::preview_import,
            commands::import::commit_import,
            commands::import::export_obsidian,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn mobile_entry_point() {
    run();
}
