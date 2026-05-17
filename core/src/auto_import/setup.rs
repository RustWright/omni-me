//! `setup_from_credentials` — read `credentials.toml`, instantiate each
//! configured `AutoImportSource`, spawn scheduler tasks. Called once at
//! server startup; returns the set of `JoinHandle`s so the caller can
//! optionally abort them on shutdown.
//!
//! Scope today:
//! - Wise (REST) sources spin up when `[wise]` is present.
//! - WealthSimple subprocess sources spin up when `[wealthsimple_python]`
//!   is present AND a `driver_script` path is configured by the caller.
//! - IMAP wiring needs the real `AsyncImapFetcher` impl + per-account
//!   handler-config glue, deferred to a follow-up — `setup_imap_accounts`
//!   skeleton is here so the call shape is stable.
//!
//! Interval defaults to 30 minutes — chosen as a reasonable balance for
//! per-day-tx-volume use. Override per-source via `SourceConfig::interval`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use crate::auto_import_scheduler::{spawn, AutoImportSource};
use crate::credentials::Credentials;
use crate::events::{EventStore, ProjectionRunner};

use super::imap_source::ImapSource;
use super::wealthsimple::WealthSimpleSource;
use super::wise::WiseSource;

/// Per-source spawn configuration the caller decides. Account-name maps
/// can't sensibly default; the caller knows which hledger accounts each
/// integration corresponds to.
#[derive(Default)]
pub struct SourceConfig {
    /// Override the default 30-minute poll interval.
    pub interval: Option<Duration>,
    /// Wise balance currency → hledger account (e.g. `"CAD" → "Assets:Wise:CAD"`).
    pub wise_account_map: HashMap<String, String>,
    /// WealthSimple ws-api account_id → hledger account.
    pub ws_account_map: HashMap<String, String>,
    /// Filesystem path to the Python driver script (Phase 2.9 contract).
    pub ws_driver_script: Option<PathBuf>,
    /// Pre-constructed IMAP sources — caller builds these because handler
    /// composition (which ScNgnHandler / ReceiptHandler attaches to which
    /// account, with what sender patterns + account mappings) is a per-app
    /// policy decision rather than something derivable from credentials alone.
    /// Pass an empty Vec to skip IMAP entirely.
    pub imap_sources: Vec<Arc<ImapSource>>,
}

const DEFAULT_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Spawn one task per configured source. Returns the JoinHandles so the
/// caller can wait on / abort them at shutdown. An empty return value
/// means no sources were configured — startup succeeds (graceful).
pub fn setup_from_credentials(
    creds: &Credentials,
    config: &SourceConfig,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    device_id: String,
) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::new();
    let interval = config.interval.unwrap_or(DEFAULT_INTERVAL);

    if let Some(wise) = &creds.wise {
        let source: Arc<dyn AutoImportSource> = Arc::new(WiseSource::new(
            wise.clone(),
            store.clone(),
            projections.clone(),
            device_id.clone(),
            config.wise_account_map.clone(),
        ));
        tracing::info!(source = source.name(), interval_secs = interval.as_secs(), "spawning auto-import");
        handles.push(spawn(source, interval));
    }

    if let (Some(ws), Some(driver)) =
        (&creds.wealthsimple_python, &config.ws_driver_script)
    {
        let source: Arc<dyn AutoImportSource> = Arc::new(WealthSimpleSource::new(
            ws.clone(),
            driver.clone(),
            store.clone(),
            projections.clone(),
            device_id.clone(),
            config.ws_account_map.clone(),
        ));
        tracing::info!(source = source.name(), interval_secs = interval.as_secs(), "spawning auto-import");
        handles.push(spawn(source, interval));
    } else if creds.wealthsimple_python.is_some() && config.ws_driver_script.is_none() {
        tracing::warn!(
            "wealthsimple_python configured but no driver script path provided — skipping spawn"
        );
    }

    // IMAP: spawn each pre-built source the caller passed in. We deliberately
    // don't auto-construct sources from `creds.imap` here because handler
    // composition is a per-app policy decision.
    for source in &config.imap_sources {
        let s: Arc<dyn AutoImportSource> = source.clone();
        tracing::info!(source = s.name(), interval_secs = interval.as_secs(), "spawning auto-import");
        handles.push(spawn(s, interval));
    }
    if !creds.imap.is_empty() && config.imap_sources.is_empty() {
        tracing::warn!(
            count = creds.imap.len(),
            "IMAP accounts configured in credentials but no ImapSources passed to setup — none spawned"
        );
    }

    handles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::{Credentials, WealthSimplePythonCredentials, WiseCredentials};

    async fn test_runner() -> (
        crate::db::Database,
        Arc<dyn EventStore>,
        ProjectionRunner,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let store: Arc<dyn EventStore> =
            Arc::new(crate::events::SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(crate::events::BudgetProjection)],
        );
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    #[tokio::test]
    async fn no_credentials_means_no_handles() {
        let (_db, store, projections) = test_runner().await;
        let handles = setup_from_credentials(
            &Credentials::default(),
            &SourceConfig::default(),
            store,
            projections,
            "device-1".into(),
        );
        assert_eq!(handles.len(), 0);
    }

    #[tokio::test]
    async fn wise_creds_spawn_one_source() {
        let (_db, store, projections) = test_runner().await;
        let creds = Credentials {
            wise: Some(WiseCredentials {
                api_token: "test-token".into(),
                profile_id: None,
            }),
            ..Credentials::default()
        };
        let handles = setup_from_credentials(
            &creds,
            &SourceConfig::default(),
            store,
            projections,
            "device-1".into(),
        );
        assert_eq!(handles.len(), 1);
        // Abort so the spawned task doesn't outlive the test.
        for h in handles {
            h.abort();
        }
    }

    #[tokio::test]
    async fn ws_creds_with_driver_path_spawn_one_source() {
        let (_db, store, projections) = test_runner().await;
        let creds = Credentials {
            wealthsimple_python: Some(WealthSimplePythonCredentials {
                email: "x@y".into(),
                password: "p".into(),
                python_path: "/usr/bin/python3".into(),
            }),
            ..Credentials::default()
        };
        let config = SourceConfig {
            ws_driver_script: Some(PathBuf::from("/nonexistent/driver.py")),
            ..SourceConfig::default()
        };
        let handles = setup_from_credentials(
            &creds,
            &config,
            store,
            projections,
            "device-1".into(),
        );
        assert_eq!(handles.len(), 1);
        for h in handles {
            h.abort();
        }
    }

    #[tokio::test]
    async fn ws_creds_without_driver_path_does_not_spawn() {
        let (_db, store, projections) = test_runner().await;
        let creds = Credentials {
            wealthsimple_python: Some(WealthSimplePythonCredentials {
                email: "x@y".into(),
                password: "p".into(),
                python_path: "/usr/bin/python3".into(),
            }),
            ..Credentials::default()
        };
        let handles = setup_from_credentials(
            &creds,
            &SourceConfig::default(),
            store,
            projections,
            "device-1".into(),
        );
        assert_eq!(handles.len(), 0, "no driver path → no spawn (with warning)");
    }

    #[tokio::test]
    async fn both_wise_and_ws_spawn_two_sources() {
        let (_db, store, projections) = test_runner().await;
        let creds = Credentials {
            wise: Some(WiseCredentials {
                api_token: "tok".into(),
                profile_id: None,
            }),
            wealthsimple_python: Some(WealthSimplePythonCredentials {
                email: "x".into(),
                password: "p".into(),
                python_path: "/usr/bin/python3".into(),
            }),
            ..Credentials::default()
        };
        let config = SourceConfig {
            ws_driver_script: Some(PathBuf::from("/nonexistent/driver.py")),
            ..SourceConfig::default()
        };
        let handles = setup_from_credentials(
            &creds,
            &config,
            store,
            projections,
            "device-1".into(),
        );
        assert_eq!(handles.len(), 2);
        for h in handles {
            h.abort();
        }
    }
}
