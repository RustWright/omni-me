//! Generic subprocess auto-import source — the engine side of the
//! `SUBPROCESS_SOURCE_CONTRACT.md` boundary.
//!
//! A [`SubprocessSource`] spawns a configured helper executable, sends it a
//! one-line JSON [`HelperRequest`] on stdin, reads a one-line JSON
//! [`HelperResponse`] on stdout, and (for a successful `pull`) wraps the
//! returned drafts into an `AutoImportBatchProposed` event via
//! [`crate::auto_import::to_proposed_event`] — exactly the generic tail that
//! used to live inside each bank adapter. The helper owns everything
//! bank-specific (its credentials, its upstream, its account mapping); the
//! engine never sees a secret.
//!
//! The `HelperRequest` / `HelperResponse` / `HelperStatus` types ARE the
//! contract in code: the engine deserializes responses with them and Rust
//! helpers serialize with them, so any drift is a compile error. The prose
//! companion (for non-Rust plugin authors) is `SUBPROCESS_SOURCE_CONTRACT.md`.

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::auto_import::to_proposed_event;
use crate::auto_import_scheduler::{
    AutoImportSource, ImportError, ImportSummary, ReauthOutcome,
};
use crate::events::{DraftTransaction, EventStore, ProjectionRunner};

/// Engine → helper request, sent as one JSON line on the helper's stdin.
/// Tagged by `verb` so the wire form is `{"verb":"pull"}` /
/// `{"verb":"reauth","otp":"…"}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum HelperRequest {
    /// Normal scheduled tick: fetch whatever is new, return drafts.
    Pull,
    /// Interactive re-auth with a single-use code. Defined now so the contract
    /// is frozen; the `/reauth` route + helper handling land next session.
    Reauth { otp: String },
}

/// The `status` field of a [`HelperResponse`] — the outcome the helper reports.
/// Wires as snake_case (`needs_reauth`, …) to match the contract doc's table.
/// `Serialize` is derived alongside `Deserialize` so a Rust helper can construct
/// and emit one too (single source of truth on both ends of the pipe).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HelperStatus {
    /// Success. `drafts` may be empty — "no new data" is not a failure.
    Ok,
    /// Stored credential expired/invalid; the helper did NOT loop on login.
    /// Engine degrades this source instead of hammering the upstream.
    NeedsReauth,
    /// `reauth` succeeded; credential refreshed + persisted. (Next session.)
    ReauthOk,
    /// `reauth` ran but the supplied code was wrong. (Next session.)
    InvalidOtp,
    /// Anything unexpected; `message` carries detail. Engine treats it as a
    /// transient failure → exponential backoff.
    Error,
}

/// Helper → engine response: one JSON line on the helper's stdout.
///
/// `#[serde(default)]` on every field but `status` means a terse helper can emit
/// `{"status":"ok"}` and still deserialize — the forgiving contract the doc
/// promises third-party plugin authors. The engine ignores unknown fields, so a
/// helper may carry extra keys without breaking older engines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelperResponse {
    pub status: HelperStatus,
    /// Fully-built drafts (real accounts + any `Unmatched` mirror). The engine
    /// wraps these verbatim — it never reasons about banks or balancing.
    #[serde(default)]
    pub drafts: Vec<DraftTransaction>,
    /// Per-batch idempotency token. When `None`, the engine falls back to
    /// `"{source-name}-{unix_millis}"`.
    #[serde(default)]
    pub dedup_key: Option<String>,
    /// Opaque JSON the review screen can render; engine stores but never reads.
    #[serde(default)]
    pub source_metadata: Option<serde_json::Value>,
    /// Human-readable detail; required when `status == Error`.
    #[serde(default)]
    pub message: Option<String>,
}

/// A source whose work is delegated to an external helper executable over the
/// `SUBPROCESS_SOURCE_CONTRACT.md` boundary. Holds the engine handles the helper
/// can't have across a process boundary (the event store + projections) and the
/// command to spawn; everything bank-specific lives in the helper.
pub struct SubprocessSource {
    name: String,
    /// The helper executable. Resolved + existence-checked before spawn when it
    /// looks like a path (see `run_helper`), so a stale/missing helper path
    /// surfaces as a clear `NotConfigured` rather than a raw OS spawn error.
    command: PathBuf,
    args: Vec<String>,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    device_id: String,
}

impl SubprocessSource {
    pub fn new(
        name: impl Into<String>,
        command: impl Into<PathBuf>,
        args: Vec<String>,
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        device_id: String,
    ) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args,
            store,
            projections,
            device_id,
        }
    }

    /// Spawn the helper, send `request` as one JSON line on stdin, and parse the
    /// one JSON line it writes to stdout. Separated from `pull` so a future
    /// `reauth` path reuses the exact same transport.
    async fn run_helper(&self, request: &HelperRequest) -> Result<HelperResponse, ImportError> {
        // Hardening (Step 2a/E): if the command is a path (not a bare PATH
        // name), verify it exists first — turns a stale helper path into a
        // precise error instead of an opaque "No such file or directory".
        let looks_like_path = self.command.components().count() > 1;
        if looks_like_path && !self.command.exists() {
            return Err(ImportError::NotConfigured(format!(
                "helper command not found: {}",
                self.command.display()
            )));
        }

        let request_json = serde_json::to_string(request)
            .map_err(|e| ImportError::Parse(format!("serialize request: {e}")))?;

        let mut child = Command::new(&self.command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                ImportError::Io(format!("spawn helper {}: {e}", self.command.display()))
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(request_json.as_bytes())
                .await
                .map_err(|e| ImportError::Io(format!("write stdin: {e}")))?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|e| ImportError::Io(format!("write stdin newline: {e}")))?;
            // Drop stdin → EOF, so the helper's stdin read completes.
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ImportError::Io(format!("wait helper: {e}")))?;

        // Non-zero exit = the helper crashed or never produced JSON (per the
        // contract, even `needs_reauth` exits 0). Treat as transient → backoff.
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ImportError::Upstream(format!(
                "helper {} exited {} stderr: {stderr}",
                self.name, output.status
            )));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| ImportError::Parse(format!("stdout not utf-8: {e}")))?;
        serde_json::from_str(stdout.trim())
            .map_err(|e| ImportError::Parse(format!("helper response: {e}")))
    }

    /// Wrap helper-supplied drafts into a single `AutoImportBatchProposed` event
    /// and project it — the generic tail that used to live in each bank adapter.
    async fn ingest(&self, response: HelperResponse) -> Result<ImportSummary, ImportError> {
        if response.drafts.is_empty() {
            return Ok(ImportSummary { events_appended: 0 });
        }

        // Helper-supplied dedup_key wins; else a per-tick timestamp (matches the
        // old WS polling behavior — row-level dedup still rides each draft's
        // stable external_id).
        let dedup_key = response
            .dedup_key
            .unwrap_or_else(|| format!("{}-{}", self.name, Utc::now().timestamp_millis()));

        let proposed = to_proposed_event(
            &self.name,
            dedup_key,
            response.drafts,
            response.source_metadata,
            self.device_id.clone(),
        );

        let appended = self
            .store
            .append_batch(vec![proposed])
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

#[async_trait]
impl AutoImportSource for SubprocessSource {
    fn name(&self) -> &str {
        &self.name
    }

    async fn pull(&self) -> Result<ImportSummary, ImportError> {
        let response = self.run_helper(&HelperRequest::Pull).await?;
        match response.status {
            HelperStatus::Ok => self.ingest(response).await,
            // A needs-reauth signal rides its own ImportError variant so the
            // registry can flip the source to AuthState::NeedsReauth (3.5a).
            // The scheduler still backs off on it — harmless, since the helper
            // returns fast without looping on login (no lockout risk).
            HelperStatus::NeedsReauth => Err(ImportError::NeedsReauth(
                response.message.unwrap_or_else(|| {
                    format!("{} session expired — reconnect required", self.name)
                }),
            )),
            HelperStatus::Error => Err(ImportError::Upstream(
                response
                    .message
                    .unwrap_or_else(|| format!("{} reported an error", self.name)),
            )),
            // reauth-only outcomes can't answer a pull request.
            HelperStatus::ReauthOk | HelperStatus::InvalidOtp => Err(ImportError::Parse(format!(
                "{} returned a reauth status to a pull request",
                self.name
            ))),
        }
    }

    fn reauth_capable(&self) -> bool {
        // Any subprocess source can be sent a `reauth` verb; whether the helper
        // actually does anything useful with it is the helper's business. The
        // engine offers the affordance uniformly.
        true
    }

    async fn reauth(&self, otp: &str) -> ReauthOutcome {
        let response = match self
            .run_helper(&HelperRequest::Reauth {
                otp: otp.to_string(),
            })
            .await
        {
            Ok(r) => r,
            // Spawn/transport failure (helper missing, crashed, bad JSON) —
            // report as an error outcome the route can render, not a panic.
            Err(e) => return ReauthOutcome::Error { message: e.to_string() },
        };
        match response.status {
            HelperStatus::ReauthOk => ReauthOutcome::Active,
            HelperStatus::InvalidOtp => ReauthOutcome::InvalidOtp,
            HelperStatus::Error => ReauthOutcome::Error {
                message: response
                    .message
                    .unwrap_or_else(|| format!("{} reauth error", self.name)),
            },
            // A helper that answers a reauth with a pull status is misbehaving.
            HelperStatus::Ok | HelperStatus::NeedsReauth => ReauthOutcome::Error {
                message: format!("{} returned a pull status to a reauth request", self.name),
            },
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Write an executable shell script standing in for a real helper. Returns
    /// the TempDir (kept alive for the script's lifetime) + the script path.
    fn write_shell_helper(content: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("helper.sh");
        std::fs::write(&path, content).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, path)
    }

    /// A helper that consumes stdin and emits a fixed response line.
    fn emit_helper(response_json: &str) -> (tempfile::TempDir, PathBuf) {
        write_shell_helper(&format!(
            "#!/bin/bash\ncat > /dev/null\ncat <<'EOF'\n{response_json}\nEOF\n"
        ))
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
            vec![
                Box::new(crate::events::BudgetProjection),
                Box::new(crate::events::AutoImportProjection),
            ],
        );
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    fn source_with(
        command: PathBuf,
        args: Vec<String>,
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
    ) -> SubprocessSource {
        SubprocessSource::new("test-source", command, args, store, projections, "device-1".into())
    }

    const ONE_DRAFT_OK: &str = r#"{"status":"ok","drafts":[{"external_id":"ws-t1","date":"2026-06-15","description":"Loblaws","postings":[{"account":"Assets:Wealthsimple:Cash","commodity":"CAD","amount":"-87.42"},{"account":"Unmatched","commodity":"CAD","amount":"87.42"}]}]}"#;

    #[tokio::test]
    async fn pull_projects_drafts_from_helper() {
        let (_helper_dir, helper) = emit_helper(ONE_DRAFT_OK);
        let (db, store, projections) = test_db_and_runner().await;
        let source = source_with(helper, vec![], store, projections);

        let summary = source.pull().await.unwrap();
        assert_eq!(summary.events_appended, 1);

        // The drafts landed as one pending batch with the postings intact —
        // proving the generic ingest tail wraps helper output identically to
        // the old in-process adapter.
        let mut resp = db
            .query("SELECT source, status, draft_postings FROM pending_auto_import_batches")
            .await
            .unwrap();
        let sources: Vec<String> = resp.take("source").unwrap();
        let statuses: Vec<String> = resp.take("status").unwrap();
        let draft_postings: Vec<serde_json::Value> = resp.take("draft_postings").unwrap();
        assert_eq!(sources, vec!["test-source".to_string()]);
        assert_eq!(statuses, vec!["pending".to_string()]);
        let drafts = draft_postings[0].as_array().unwrap();
        assert_eq!(drafts.len(), 1);
        let postings = drafts[0]["postings"].as_array().unwrap();
        assert_eq!(postings[0]["account"], "Assets:Wealthsimple:Cash");
        assert_eq!(postings[1]["account"], "Unmatched");
    }

    #[tokio::test]
    async fn pull_with_terse_ok_response_appends_nothing() {
        // The forgiving-contract case: a helper that emits only `{"status":"ok"}`
        // (no drafts) must parse and count as a clean zero-event tick.
        let (_helper_dir, helper) = emit_helper(r#"{"status":"ok"}"#);
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(helper, vec![], store, projections);

        let summary = source.pull().await.unwrap();
        assert_eq!(summary.events_appended, 0);
    }

    #[tokio::test]
    async fn pull_needs_reauth_status_yields_needs_reauth_error() {
        let (_helper_dir, helper) = emit_helper(r#"{"status":"needs_reauth"}"#);
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(helper, vec![], store, projections);

        let err = source.pull().await.unwrap_err();
        // The distinct variant is what lets the registry flip AuthState — a
        // generic Upstream error wouldn't.
        match err {
            ImportError::NeedsReauth(msg) => assert!(msg.contains("reconnect"), "msg: {msg}"),
            other => panic!("expected NeedsReauth, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reauth_ok_status_yields_active() {
        let (_helper_dir, helper) = emit_helper(r#"{"status":"reauth_ok"}"#);
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(helper, vec![], store, projections);

        assert_eq!(source.reauth("123456").await, ReauthOutcome::Active);
    }

    #[tokio::test]
    async fn reauth_invalid_otp_status_yields_invalid_otp() {
        let (_helper_dir, helper) = emit_helper(r#"{"status":"invalid_otp"}"#);
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(helper, vec![], store, projections);

        assert_eq!(source.reauth("000000").await, ReauthOutcome::InvalidOtp);
    }

    #[tokio::test]
    async fn reauth_error_status_carries_message() {
        let (_helper_dir, helper) =
            emit_helper(r#"{"status":"error","message":"driver crashed"}"#);
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(helper, vec![], store, projections);

        match source.reauth("123456").await {
            ReauthOutcome::Error { message } => assert!(message.contains("driver crashed")),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reauth_sends_reauth_verb_with_otp_on_stdin() {
        // Capture stdin to prove the OTP rides the wire in the tagged form.
        let (_helper_dir, helper) = write_shell_helper(
            "#!/bin/bash\ncat > \"$1\"\necho '{\"status\":\"reauth_ok\"}'\n",
        );
        let captured = _helper_dir.path().join("stdin.txt");
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(
            helper,
            vec![captured.to_string_lossy().to_string()],
            store,
            projections,
        );

        source.reauth("987654").await;
        let sent = std::fs::read_to_string(&captured).unwrap();
        assert_eq!(sent.trim(), r#"{"verb":"reauth","otp":"987654"}"#);
    }

    #[tokio::test]
    async fn pull_maps_error_status_to_upstream_message() {
        let (_helper_dir, helper) = emit_helper(r#"{"status":"error","message":"upstream 503"}"#);
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(helper, vec![], store, projections);

        let err = source.pull().await.unwrap_err();
        match err {
            ImportError::Upstream(msg) => assert!(msg.contains("upstream 503"), "msg: {msg}"),
            other => panic!("expected Upstream, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_helper_sends_pull_verb_on_stdin() {
        // Helper writes whatever it reads on stdin to the file named by $1, then
        // emits a valid response. Lets us assert the request wire form.
        let (_helper_dir, helper) = write_shell_helper(
            "#!/bin/bash\ncat > \"$1\"\necho '{\"status\":\"ok\"}'\n",
        );
        let captured = _helper_dir.path().join("stdin.txt");
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(
            helper,
            vec![captured.to_string_lossy().to_string()],
            store,
            projections,
        );

        source.pull().await.unwrap();
        let sent = std::fs::read_to_string(&captured).unwrap();
        assert_eq!(sent.trim(), r#"{"verb":"pull"}"#);
    }

    #[tokio::test]
    async fn missing_command_is_not_configured() {
        let (_db, store, projections) = test_db_and_runner().await;
        let source = source_with(
            Path::new("/nonexistent/dir/ws-helper").to_path_buf(),
            vec![],
            store,
            projections,
        );

        let err = source.pull().await.unwrap_err();
        assert!(
            matches!(err, ImportError::NotConfigured(_)),
            "expected NotConfigured, got {err:?}"
        );
    }
}
