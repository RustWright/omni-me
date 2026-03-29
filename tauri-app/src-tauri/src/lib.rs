mod commands;

use std::path::Path;

use tauri::Manager;

use omni_me_core::db::{self, Database};
use omni_me_core::events::{
    NotesProjection, ProjectionRunner, RoutinesProjection, SurrealEventStore,
};

const DB_NAME: &str = "local.db";
const DEVICE_ID_FILE: &str = "device_id";
const SERVER_URL_FILE: &str = "server_url";
const DEFAULT_SERVER_URL: &str = "http://localhost:3000";

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
    pub app_data_dir: std::path::PathBuf,
}

/// Remove stale SurrealKV LOCK file if the owning process is no longer alive.
/// SurrealKV writes the PID to a LOCK file and doesn't clean it up on unclean
/// shutdown (SIGKILL, crash, etc.), which blocks subsequent opens.
fn clear_stale_lock(db_path: &Path) {
    let lock_path = db_path.join("LOCK");
    if let Ok(contents) = std::fs::read_to_string(&lock_path) {
        if let Ok(pid) = contents.trim().parse::<u32>() {
            let alive = Path::new(&format!("/proc/{}", pid)).exists();
            if !alive {
                tracing::warn!(pid, "Removing stale SurrealKV LOCK (pid not running)");
                let _ = std::fs::remove_file(&lock_path);
            }
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

                tracing::info!(device_id = %device_id, server_url = %server_url, "App initialized");

                handle.manage(AppState {
                    db,
                    event_store,
                    projections,
                    device_id,
                    server_url: tokio::sync::RwLock::new(server_url),
                    app_data_dir: app_data,
                });
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Notes
            commands::notes::create_note,
            commands::notes::list_notes,
            commands::notes::get_note,
            commands::notes::update_note,
            commands::notes::search_notes,
            commands::notes::process_note_llm,
            // Routines
            commands::routines::create_routine_group,
            commands::routines::list_routine_groups,
            commands::routines::add_routine_item,
            commands::routines::list_routine_items,
            commands::routines::complete_routine_item,
            commands::routines::skip_routine_item,
            commands::routines::modify_routine_group,
            commands::routines::get_completions_for_date,
            commands::routines::get_routine_history,
            // Sync
            commands::sync::trigger_sync,
            commands::sync::get_sync_info,
            commands::sync::update_server_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn mobile_entry_point() {
    run();
}
