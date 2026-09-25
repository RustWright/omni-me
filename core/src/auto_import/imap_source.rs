//! `ImapSource` — bridges the IMAP-side trait stack (`ImapFetcher` + handlers)
//! into the scheduler-side trait (`AutoImportSource`).
//!
//! Holds a fetcher + a list of handlers + a persistent cursor. Each `pull()`
//! tick:
//!   1. Loads cursor from store (else in-memory fallback for tests)
//!   2. Calls `poll_once(fetcher, handlers, cursor, archive)`
//!   3. Appends emitted events via `EventStore::append_batch`
//!   4. Runs projections on the batch
//!   5. Persists the advanced cursor
//!
//! Cursor persistence avoids re-processing the entire inbox on every server
//! restart. The `CursorStore` trait is async + object-safe so the production
//! `SurrealCursorStore` and test-side mocks share the same surface.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::auto_import_scheduler::{
    AutoImportSource, DropReason, ImportError, ImportSummary, ImportTally,
};
use crate::db::Database;
use crate::events::{EventStore, ProjectionRunner};

use super::imap::{ArchiveTarget, FetchCursor, ImapFetcher, ImapHandler, poll_once};

#[async_trait]
pub trait CursorStore: Send + Sync {
    /// The stored cursor, or `None` when this account has never polled.
    ///
    /// ⚠️ The UID alone is not a cursor — see [`FetchCursor::uid_validity`]. A row
    /// written before the validity was recorded loads it as `None`, which reads as
    /// "unknown" and never as a mismatch, so an upgrade does not reset a mailbox.
    async fn load(&self, account_name: &str) -> Result<Option<StoredCursor>, ImportError>;
    async fn save(&self, account_name: &str, cursor: StoredCursor) -> Result<(), ImportError>;
}

/// A cursor as it sits on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredCursor {
    pub uid: u32,
    pub uid_validity: Option<u32>,
}

/// SurrealDB-backed cursor store. Uses a dedicated `imap_cursors` table
/// keyed by account name (the same name the credentials.toml uses, e.g.
/// `gmail_personal`).
pub struct SurrealCursorStore {
    db: Database,
}

impl SurrealCursorStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Idempotent schema initialisation. Call once at startup.
    pub async fn init_schema(&self) -> Result<(), ImportError> {
        self.db
            .query(
                "DEFINE TABLE IF NOT EXISTS imap_cursors SCHEMAFULL;
                 DEFINE FIELD IF NOT EXISTS uid ON imap_cursors TYPE int;
                 -- `option<>` so rows written before this existed still load, and
                 -- load as unknown rather than as a mismatch that would reset the
                 -- mailbox on the first tick after an upgrade.
                 DEFINE FIELD IF NOT EXISTS uid_validity ON imap_cursors TYPE option<int>;
                 DEFINE FIELD IF NOT EXISTS updated_at ON imap_cursors TYPE datetime;",
            )
            .await
            .map_err(|e| ImportError::Upstream(format!("init imap_cursors: {e}")))?
            .check()
            .map_err(|e| ImportError::Upstream(format!("init imap_cursors: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl CursorStore for SurrealCursorStore {
    async fn load(&self, account_name: &str) -> Result<Option<StoredCursor>, ImportError> {
        let mut resp = self
            .db
            .query("SELECT uid, uid_validity FROM type::record('imap_cursors', $name)")
            .bind(("name", account_name.to_string()))
            .await
            .map_err(|e| ImportError::Upstream(format!("load cursor: {e}")))?;
        let uid: Option<i64> = resp
            .take("uid")
            .map_err(|e| ImportError::Upstream(format!("decode cursor: {e}")))?;
        let uid_validity: Option<i64> = resp
            .take("uid_validity")
            .map_err(|e| ImportError::Upstream(format!("decode cursor validity: {e}")))?;
        Ok(uid.map(|n| StoredCursor {
            uid: n as u32,
            uid_validity: uid_validity.map(|v| v as u32),
        }))
    }

    async fn save(&self, account_name: &str, cursor: StoredCursor) -> Result<(), ImportError> {
        let ts = chrono::Utc::now().to_rfc3339();
        self.db
            .query(
                "UPSERT type::record('imap_cursors', $name) CONTENT {
                    uid: $uid,
                    uid_validity: $uid_validity,
                    updated_at: type::datetime($ts)
                 }",
            )
            .bind(("name", account_name.to_string()))
            .bind(("uid", cursor.uid as i64))
            .bind(("uid_validity", cursor.uid_validity.map(|v| v as i64)))
            .bind(("ts", ts))
            .await
            .map_err(|e| ImportError::Upstream(format!("save cursor: {e}")))?
            // A refused cursor write used to read as a saved one, which is the
            // shape that re-fetches a whole mailbox on the next tick.
            .check()
            .map_err(|e| ImportError::Upstream(format!("save cursor: {e}")))?;
        Ok(())
    }
}

pub struct ImapSource {
    name: String,
    fetcher: Arc<dyn ImapFetcher>,
    handlers: Vec<Box<dyn ImapHandler>>,
    /// In-memory fallback cursor — primed from the persistent store at
    /// construction time, refreshed after each successful tick.
    cursor: Mutex<FetchCursor>,
    cursor_store: Option<Arc<dyn CursorStore>>,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    /// Where fetched messages are archived, and as whose device.
    ///
    /// ⚠️ `Option` because a caller with no blob store legitimately cannot
    /// archive — ⛔ not because archiving is optional policy. Production passes
    /// it; leaving it `None` files every statement email as transactions only,
    /// with the message itself gone.
    archive: Option<(std::path::PathBuf, String)>,
}

impl ImapSource {
    /// Construct + prime the cursor from persistent storage if available.
    pub async fn new(
        name: impl Into<String>,
        fetcher: Arc<dyn ImapFetcher>,
        handlers: Vec<Box<dyn ImapHandler>>,
        cursor_store: Option<Arc<dyn CursorStore>>,
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        archive: Option<(std::path::PathBuf, String)>,
    ) -> Result<Self, ImportError> {
        let name = name.into();
        let initial = if let Some(cs) = &cursor_store {
            cs.load(&name).await?
        } else {
            None
        };
        Ok(Self {
            name,
            fetcher,
            handlers,
            cursor: Mutex::new(FetchCursor {
                last_seen_uid: initial.map(|c| c.uid),
                uid_validity: initial.and_then(|c| c.uid_validity),
            }),
            cursor_store,
            store,
            projections,
            archive,
        })
    }
}

#[async_trait]
impl AutoImportSource for ImapSource {
    fn name(&self) -> &str {
        &self.name
    }

    async fn pull(&self) -> Result<ImportSummary, ImportError> {
        let cursor_snapshot = self.cursor.lock().await.clone();
        let target = self.archive.as_ref().map(|(dir, device_id)| ArchiveTarget {
            blob_dir: dir.as_path(),
            device_id,
        });
        let outcome = poll_once(
            self.fetcher.as_ref(),
            &self.handlers,
            &cursor_snapshot,
            target.as_ref(),
        )
        .await?;
        let next_cursor = outcome.next_cursor.clone();

        // The accounting unit here is the **message**, not the event: one
        // statement email can yield many transactions, so counting events would
        // make the identity unbalanceable against what the mailbox handed us.
        let mut tally = ImportTally::new(outcome.messages_seen);
        for uid in &outcome.unrouted {
            tally.dropped(format!("uid {uid}"), DropReason::Ignored);
        }
        for (uid, err) in &outcome.failed {
            tally.failed(format!("uid {uid}"), err.clone());
        }
        // A handler that succeeds but yields no events still *processed* its
        // message, so it counts here. The claim being made is "this message was
        // handled as intended", which is exactly what the identity needs.
        tally.appended(outcome.messages_seen - outcome.unrouted.len() - outcome.failed.len());

        if !outcome.events.is_empty() {
            let appended = self
                .store
                .append_batch(outcome.events)
                .await
                .map_err(|e| ImportError::Upstream(format!("append batch: {e}")))?;
            // Best-effort once stored: failing here skips the cursor save below, so the
            // next tick would re-archive the same messages as new documents.
            let failed = self.projections.apply_events_resilient(&appended).await;
            if failed > 0 {
                tracing::warn!(
                    source = %self.name,
                    failed,
                    "archived mail stored but not all projected"
                );
            }
        }

        // Advance the cursor in memory + persistent storage.
        *self.cursor.lock().await = next_cursor.clone();
        if let (Some(cs), Some(uid)) = (&self.cursor_store, next_cursor.last_seen_uid) {
            cs.save(
                &self.name,
                StoredCursor {
                    uid,
                    uid_validity: next_cursor.uid_validity,
                },
            )
            .await?;
        }

        tally.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auto_import::imap::ImapMessage;
    use crate::auto_import::imap::mock::{MockFetcher, NeedleHandler};
    use chrono::{TimeZone, Utc};
    use std::sync::Mutex as StdMutex;

    fn make_msg(uid: u32, from: &str) -> ImapMessage {
        ImapMessage {
            uid,
            from: from.into(),
            subject: "x".into(),
            date: Utc.with_ymd_and_hms(2026, 5, 16, 0, 0, 0).unwrap(),
            body: Vec::new(),
        }
    }

    async fn test_db_runner() -> (Database, Arc<dyn EventStore>, ProjectionRunner) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let store: Arc<dyn EventStore> =
            Arc::new(crate::events::SurrealEventStore::new(db.clone()));
        let runner =
            ProjectionRunner::new(db.clone(), vec![Box::new(crate::events::BudgetProjection)]);
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    /// In-memory cursor store for unit tests — no SurrealDB round-trip.
    struct MemCursorStore {
        loaded: StdMutex<std::collections::HashMap<String, StoredCursor>>,
    }
    impl MemCursorStore {
        fn new() -> Self {
            Self {
                loaded: StdMutex::new(std::collections::HashMap::new()),
            }
        }
        fn with(name: &str, uid: u32) -> Self {
            let s = Self::new();
            s.loaded.lock().unwrap().insert(
                name.into(),
                StoredCursor {
                    uid,
                    uid_validity: None,
                },
            );
            s
        }
    }
    #[async_trait]
    impl CursorStore for MemCursorStore {
        async fn load(&self, account_name: &str) -> Result<Option<StoredCursor>, ImportError> {
            Ok(self.loaded.lock().unwrap().get(account_name).copied())
        }
        async fn save(&self, account_name: &str, cursor: StoredCursor) -> Result<(), ImportError> {
            self.loaded
                .lock()
                .unwrap()
                .insert(account_name.into(), cursor);
            Ok(())
        }
    }

    #[tokio::test]
    async fn pull_advances_cursor_in_persistent_store() {
        let (_db, store, projections) = test_db_runner().await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        fetcher.push_response(
            vec![make_msg(101, "x@a.com"), make_msg(102, "y@a.com")],
            Some(102),
        );
        let cursor_store: Arc<dyn CursorStore> = Arc::new(MemCursorStore::new());
        let handlers: Vec<Box<dyn ImapHandler>> = vec![Box::new(NeedleHandler {
            name: "x".into(),
            needle: "@a.com".into(),
        })];
        let source = ImapSource::new(
            "gmail",
            fetcher,
            handlers,
            Some(cursor_store.clone()),
            store,
            projections,
            None,
        )
        .await
        .unwrap();

        let summary = source.pull().await.unwrap();
        assert_eq!(summary.appended, 2, "two messages handled");
        assert_eq!(summary.lost(), 0);
        assert_eq!(
            cursor_store.load("gmail").await.unwrap().map(|c| c.uid),
            Some(102)
        );
    }

    #[tokio::test]
    async fn primes_cursor_from_persistent_store_at_construction() {
        let (_db, store, projections) = test_db_runner().await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        let cursor_store: Arc<dyn CursorStore> = Arc::new(MemCursorStore::with("gmail", 500));
        let source = ImapSource::new(
            "gmail",
            fetcher,
            vec![],
            Some(cursor_store),
            store,
            projections,
            None,
        )
        .await
        .unwrap();
        assert_eq!(source.cursor.lock().await.last_seen_uid, Some(500));
    }

    #[tokio::test]
    async fn surreal_cursor_store_round_trips_uid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let cs = SurrealCursorStore::new(db);
        cs.init_schema().await.unwrap();
        let first = StoredCursor {
            uid: 12345,
            uid_validity: Some(7),
        };
        cs.save("gmail_personal", first).await.unwrap();
        assert_eq!(cs.load("gmail_personal").await.unwrap(), Some(first));
        // Overwrite via UPSERT
        let second = StoredCursor {
            uid: 67890,
            uid_validity: Some(8),
        };
        cs.save("gmail_personal", second).await.unwrap();
        assert_eq!(cs.load("gmail_personal").await.unwrap(), Some(second));
        // Missing account → None, no error
        assert_eq!(cs.load("never_seen").await.unwrap(), None);
    }

    /// A row from before the validity column existed has to keep loading, or the
    /// first tick after an upgrade reads as a renumbering and resets the mailbox.
    #[tokio::test]
    async fn a_cursor_stored_without_a_validity_loads_as_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let cs = SurrealCursorStore::new(db);
        cs.init_schema().await.unwrap();
        cs.save(
            "legacy",
            StoredCursor {
                uid: 99,
                uid_validity: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            cs.load("legacy").await.unwrap(),
            Some(StoredCursor {
                uid: 99,
                uid_validity: None
            })
        );
    }
}
