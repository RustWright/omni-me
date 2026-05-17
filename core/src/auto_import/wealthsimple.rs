//! WealthSimple auto-import via subprocess wrapper around `gboudreau/ws-api-python`.
//!
//! Per project_ws_api_strategy.md (2026-05-16 spike resolution): SnapTrade
//! coverage is broker-only (WealthSimple Trade); the user's 95% volume lives
//! in Cash + Chequing + Crypto which only the unofficial library reaches.
//!
//! ## Subprocess contract
//!
//! `omni-me` spawns the configured `python_path` running a *user-provided
//! driver script* (we do NOT bundle the ws-api-python library itself — the
//! user manages that install). The driver script:
//!
//! 1. Reads one line of JSON from stdin: `{"email": "...", "password": "..."}`.
//! 2. Logs into WealthSimple via ws-api-python.
//! 3. Pulls accounts + transactions.
//! 4. Emits one line of JSON to stdout matching `WsImportEnvelope` (see below).
//! 5. Exits 0 on success, non-zero on failure (stderr is captured for logs).
//!
//! Decoupling via "user-provided driver script" insulates omni-me from
//! breaking changes in ws-api-python's Python API surface — when ws-api-python
//! changes, the user updates *their* driver, not us.
//!
//! ## Idempotency
//!
//! Each WS transaction has a stable `external_id`. We map it to a
//! deterministic omni-me transaction id (`format!("ws-{external_id}")`) so a
//! second run of the same script doesn't double-record anything — the
//! `transactions` projection's CREATE on a duplicate id silently fails
//! (acceptable — the event already exists in the log).

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::accounts::{make_unmatched_mirror, UNMATCHED_ACCOUNT};
use crate::auto_import_scheduler::{AutoImportSource, ImportError, ImportSummary};
use crate::credentials::WealthSimplePythonCredentials;
use crate::events::{
    EventStore, EventType, NewEvent, Posting, ProjectionRunner, TransactionRecordedPayload,
};

#[derive(Debug, Clone, Deserialize)]
pub struct WsImportEnvelope {
    pub accounts: Vec<WsAccount>,
    pub transactions: Vec<WsTransaction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WsAccount {
    pub id: String,
    pub name: String,
    pub currency: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WsTransaction {
    /// Stable WS-side identifier. Used to derive a deterministic omni-me
    /// txn id so re-runs of the driver don't duplicate events.
    pub external_id: String,
    pub account_id: String,
    pub date: NaiveDate,
    pub description: String,
    /// Signed amount in the account's commodity. Negative = outflow.
    /// Wire format must be a string for `rust_decimal::Decimal`.
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    pub commodity: String,
}

pub struct WealthSimpleSource {
    creds: WealthSimplePythonCredentials,
    driver_script: PathBuf,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    device_id: String,
    /// Account-name mapping: WS account_id → omni-me account name. The driver
    /// emits WS-side ids; this map translates to hledger accounts. Built
    /// from the user's `accounts.toml` (or similar) — for now, callers pass
    /// it in.
    account_map: std::collections::HashMap<String, String>,
}

impl WealthSimpleSource {
    pub fn new(
        creds: WealthSimplePythonCredentials,
        driver_script: PathBuf,
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        device_id: String,
        account_map: std::collections::HashMap<String, String>,
    ) -> Self {
        Self {
            creds,
            driver_script,
            store,
            projections,
            device_id,
            account_map,
        }
    }

    /// Pure helper — runs the configured driver subprocess and parses its
    /// stdout. Separated from `pull()` so tests can drive it with a shell
    /// script that emits canned JSON.
    async fn run_driver(&self) -> Result<WsImportEnvelope, ImportError> {
        let session_path = self
            .creds
            .session_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/tmp/ws-omni-session.json".to_string());
        let creds_json = serde_json::json!({
            "email": self.creds.email,
            "password": self.creds.password,
            "otp": serde_json::Value::Null,
            "session_path": session_path,
        })
        .to_string();

        let mut child = Command::new(&self.creds.python_path)
            .arg(&self.driver_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ImportError::Io(format!("spawn python: {e}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(creds_json.as_bytes())
                .await
                .map_err(|e| ImportError::Io(format!("write stdin: {e}")))?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|e| ImportError::Io(format!("write stdin newline: {e}")))?;
            // Drop stdin to signal EOF to the child.
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ImportError::Io(format!("wait child: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ImportError::Upstream(format!(
                "driver exited {} stderr: {stderr}",
                output.status
            )));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| ImportError::Parse(format!("stdout not utf-8: {e}")))?;

        let envelope: WsImportEnvelope = serde_json::from_str(stdout.trim())
            .map_err(|e| ImportError::Parse(format!("envelope: {e}")))?;
        Ok(envelope)
    }

    /// Convert one WS transaction into a `TransactionRecorded` event payload.
    /// Real-account posting + Unmatched mirror.
    fn build_event(&self, txn: &WsTransaction) -> Option<NewEvent> {
        let account = self.account_map.get(&txn.account_id)?.clone();
        let real_posting = Posting {
            account,
            commodity: txn.commodity.clone(),
            amount: txn.amount,
            fx_rate: None,
            tags: vec![],
        };
        let mirror = make_unmatched_mirror(&real_posting);

        let txn_id = format!("ws-{}", txn.external_id);
        let payload = TransactionRecordedPayload {
            txn_id: txn_id.clone(),
            date: txn.date,
            description: txn.description.clone(),
            postings: vec![real_posting, mirror],
            attachment: None,
        };
        let payload_json = serde_json::to_value(&payload).ok()?;

        Some(NewEvent {
            id: Some(txn_id.clone()),
            event_type: EventType::TransactionRecorded.to_string(),
            aggregate_id: txn_id,
            timestamp: Utc::now(),
            device_id: self.device_id.clone(),
            payload: payload_json,
        })
    }
}

#[async_trait]
impl AutoImportSource for WealthSimpleSource {
    fn name(&self) -> &str {
        "wealthsimple-python"
    }

    async fn pull(&self) -> Result<ImportSummary, ImportError> {
        let envelope = self.run_driver().await?;

        let mut to_append: Vec<NewEvent> = Vec::new();
        let mut skipped_unmapped = 0usize;
        for txn in &envelope.transactions {
            match self.build_event(txn) {
                Some(e) => to_append.push(e),
                None => skipped_unmapped += 1,
            }
        }
        if skipped_unmapped > 0 {
            tracing::warn!(
                count = skipped_unmapped,
                "ws auto-import: skipped txns with unmapped account_id",
            );
        }

        if to_append.is_empty() {
            return Ok(ImportSummary { events_appended: 0 });
        }

        // Append + apply atomically across the batch. Duplicates (re-run of
        // the same external_id) will be rejected by the event store's id
        // uniqueness constraint; that's the dedup mechanism.
        let appended = self
            .store
            .append_batch(to_append)
            .await
            .map_err(|e| ImportError::Upstream(format!("append batch: {e}")))?;

        self.projections
            .apply_events(&appended)
            .await
            .map_err(|e| ImportError::Upstream(format!("project: {e}")))?;

        Ok(ImportSummary {
            events_appended: appended.len(),
        })
    }
}

/// Helper that emits `Unmatched`-tagged constant for callers that need it
/// alongside the source (e.g., UI account-list filters).
pub fn unmatched_account_name() -> &'static str {
    UNMATCHED_ACCOUNT
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn cred(python: &str) -> WealthSimplePythonCredentials {
        WealthSimplePythonCredentials {
            email: "user@example.com".into(),
            password: "fake-password".into(),
            python_path: python.into(),
            driver_script: None,
            session_path: None,
        }
    }

    fn map_one(ws_id: &str, hledger_account: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert(ws_id.into(), hledger_account.into());
        m
    }

    fn write_shell_driver(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("driver.sh");
        std::fs::write(&path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        (dir, path)
    }

    async fn test_db_and_runner() -> (
        crate::db::Database,
        Arc<dyn EventStore>,
        ProjectionRunner,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let store: Arc<dyn EventStore> = Arc::new(crate::events::SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(crate::events::BudgetProjection)],
        );
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn run_driver_parses_envelope_from_shell_subprocess() {
        // Shell script ignores stdin, emits one line of canned WS envelope.
        let (_dir, driver) = write_shell_driver(
            "#!/bin/bash\ncat > /dev/null\ncat <<'EOF'\n\
            {\n\
                \"accounts\": [{\"id\": \"ws-cash\", \"name\": \"Cash\", \"currency\": \"CAD\"}],\n\
                \"transactions\": [\n\
                    {\"external_id\": \"t1\", \"account_id\": \"ws-cash\", \"date\": \"2026-05-16\", \"description\": \"Loblaws\", \"amount\": \"-87.42\", \"commodity\": \"CAD\"}\n\
                ]\n\
            }\n\
EOF\n",
        );

        let (_db, store, projections) = test_db_and_runner().await;
        let source = WealthSimpleSource::new(
            cred("/bin/bash"),
            driver,
            store,
            projections,
            "device-1".into(),
            map_one("ws-cash", "Assets:WealthSimple:Cash"),
        );

        let env = source.run_driver().await.unwrap();
        assert_eq!(env.accounts.len(), 1);
        assert_eq!(env.transactions.len(), 1);
        assert_eq!(
            env.transactions[0].amount,
            Decimal::from_str("-87.42").unwrap()
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn run_driver_surfaces_nonzero_exit_as_upstream_error() {
        let (_dir, driver) = write_shell_driver("#!/bin/bash\necho 'oh no' >&2\nexit 7\n");
        let (_db, store, projections) = test_db_and_runner().await;
        let source = WealthSimpleSource::new(
            cred("/bin/bash"),
            driver,
            store,
            projections,
            "device-1".into(),
            HashMap::new(),
        );

        let err = source.run_driver().await.unwrap_err();
        match err {
            ImportError::Upstream(msg) => assert!(msg.contains("oh no"), "stderr: {msg}"),
            other => panic!("expected Upstream, got {other:?}"),
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn pull_writes_transaction_recorded_with_unmatched_mirror() {
        let (_dir, driver) = write_shell_driver(
            "#!/bin/bash\ncat > /dev/null\ncat <<'EOF'\n\
            {\n\
                \"accounts\": [{\"id\": \"ws-cash\", \"name\": \"Cash\", \"currency\": \"CAD\"}],\n\
                \"transactions\": [\n\
                    {\"external_id\": \"t1\", \"account_id\": \"ws-cash\", \"date\": \"2026-05-16\", \"description\": \"Loblaws\", \"amount\": \"-87.42\", \"commodity\": \"CAD\"}\n\
                ]\n\
            }\n\
EOF\n",
        );

        let (db, store, projections) = test_db_and_runner().await;
        let source = WealthSimpleSource::new(
            cred("/bin/bash"),
            driver,
            store,
            projections,
            "device-1".into(),
            map_one("ws-cash", "Assets:WealthSimple:Cash"),
        );

        let summary = source.pull().await.unwrap();
        assert_eq!(summary.events_appended, 1);

        // Verify the projection got it with the mirror posting.
        let mut resp = db
            .query("SELECT description FROM type::record('transactions', 'ws-t1')")
            .await
            .unwrap();
        let desc: Option<String> = resp.take("description").unwrap();
        assert_eq!(desc.as_deref(), Some("Loblaws"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn pull_skips_transactions_with_unmapped_account_id() {
        let (_dir, driver) = write_shell_driver(
            "#!/bin/bash\ncat > /dev/null\ncat <<'EOF'\n\
            {\n\
                \"accounts\": [],\n\
                \"transactions\": [\n\
                    {\"external_id\": \"t1\", \"account_id\": \"ws-unknown\", \"date\": \"2026-05-16\", \"description\": \"x\", \"amount\": \"-1.00\", \"commodity\": \"CAD\"}\n\
                ]\n\
            }\n\
EOF\n",
        );

        let (_db, store, projections) = test_db_and_runner().await;
        let source = WealthSimpleSource::new(
            cred("/bin/bash"),
            driver,
            store,
            projections,
            "device-1".into(),
            HashMap::new(), // empty map → all skipped
        );

        let summary = source.pull().await.unwrap();
        assert_eq!(summary.events_appended, 0);
    }

    #[test]
    fn build_event_uses_external_id_for_deterministic_dedup() {
        let (_dir, driver) = write_shell_driver("#!/bin/bash\nexit 0\n");
        // Sync construction is fine since build_event doesn't touch async.
        let creds = cred("/bin/bash");
        let map = map_one("ws-cash", "Assets:WealthSimple:Cash");

        // We need a source — but we need an EventStore + ProjectionRunner for
        // construction. Use rt-block-on to keep this test sync.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (_db, store, projections) = rt.block_on(test_db_and_runner());
        let source = WealthSimpleSource::new(
            creds,
            driver,
            store,
            projections,
            "device-1".into(),
            map,
        );

        let txn = WsTransaction {
            external_id: "txn-abc-123".into(),
            account_id: "ws-cash".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Loblaws".into(),
            amount: Decimal::from_str("-87.42").unwrap(),
            commodity: "CAD".into(),
        };
        let event = source.build_event(&txn).unwrap();
        assert_eq!(event.id.as_deref(), Some("ws-txn-abc-123"));
        assert_eq!(event.aggregate_id, "ws-txn-abc-123");
    }
}
