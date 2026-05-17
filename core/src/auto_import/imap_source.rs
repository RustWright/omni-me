//! `ImapSource` — bridges the IMAP-side trait stack (`ImapFetcher` + handlers)
//! into the scheduler-side trait (`AutoImportSource`).
//!
//! Holds a fetcher + a list of handlers + a persistent cursor. Each `pull()`
//! tick:
//!   1. Loads cursor from store (else in-memory fallback for tests)
//!   2. Calls `poll_once(fetcher, handlers, cursor)` (Phase 2.11)
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

use crate::auto_import_scheduler::{AutoImportSource, ImportError, ImportSummary};
use crate::db::Database;
use crate::events::{EventStore, ProjectionRunner};

use super::imap::{poll_once, FetchCursor, ImapFetcher, ImapHandler};

#[async_trait]
pub trait CursorStore: Send + Sync {
    async fn load(&self, account_name: &str) -> Result<Option<u32>, ImportError>;
    async fn save(&self, account_name: &str, uid: u32) -> Result<(), ImportError>;
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
                 DEFINE FIELD IF NOT EXISTS updated_at ON imap_cursors TYPE datetime;",
            )
            .await
            .map_err(|e| ImportError::Upstream(format!("init imap_cursors: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl CursorStore for SurrealCursorStore {
    async fn load(&self, account_name: &str) -> Result<Option<u32>, ImportError> {
        let mut resp = self
            .db
            .query("SELECT uid FROM type::record('imap_cursors', $name)")
            .bind(("name", account_name.to_string()))
            .await
            .map_err(|e| ImportError::Upstream(format!("load cursor: {e}")))?;
        let uid: Option<i64> = resp
            .take("uid")
            .map_err(|e| ImportError::Upstream(format!("decode cursor: {e}")))?;
        Ok(uid.map(|n| n as u32))
    }

    async fn save(&self, account_name: &str, uid: u32) -> Result<(), ImportError> {
        let ts = chrono::Utc::now().to_rfc3339();
        self.db
            .query(
                "UPSERT type::record('imap_cursors', $name) CONTENT {
                    uid: $uid,
                    updated_at: type::datetime($ts)
                 }",
            )
            .bind(("name", account_name.to_string()))
            .bind(("uid", uid as i64))
            .bind(("ts", ts))
            .await
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
                last_seen_uid: initial,
            }),
            cursor_store,
            store,
            projections,
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
        let (events, next_cursor) =
            poll_once(self.fetcher.as_ref(), &self.handlers, &cursor_snapshot).await?;

        let mut appended_count = 0usize;
        if !events.is_empty() {
            let appended = self
                .store
                .append_batch(events)
                .await
                .map_err(|e| ImportError::Upstream(format!("append batch: {e}")))?;
            appended_count = appended.len();
            self.projections
                .apply_events(&appended)
                .await
                .map_err(|e| ImportError::Upstream(format!("project: {e}")))?;
        }

        // Advance the cursor in memory + persistent storage.
        *self.cursor.lock().await = next_cursor.clone();
        if let (Some(cs), Some(uid)) = (&self.cursor_store, next_cursor.last_seen_uid) {
            cs.save(&self.name, uid).await?;
        }

        Ok(ImportSummary {
            events_appended: appended_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auto_import::imap::mock::{MockFetcher, NeedleHandler};
    use crate::auto_import::imap::ImapMessage;
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
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(crate::events::BudgetProjection)],
        );
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    /// In-memory cursor store for unit tests — no SurrealDB round-trip.
    struct MemCursorStore {
        loaded: StdMutex<std::collections::HashMap<String, u32>>,
    }
    impl MemCursorStore {
        fn new() -> Self {
            Self {
                loaded: StdMutex::new(std::collections::HashMap::new()),
            }
        }
        fn with(name: &str, uid: u32) -> Self {
            let s = Self::new();
            s.loaded.lock().unwrap().insert(name.into(), uid);
            s
        }
    }
    #[async_trait]
    impl CursorStore for MemCursorStore {
        async fn load(&self, account_name: &str) -> Result<Option<u32>, ImportError> {
            Ok(self.loaded.lock().unwrap().get(account_name).copied())
        }
        async fn save(&self, account_name: &str, uid: u32) -> Result<(), ImportError> {
            self.loaded.lock().unwrap().insert(account_name.into(), uid);
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
        )
        .await
        .unwrap();

        let summary = source.pull().await.unwrap();
        assert_eq!(summary.events_appended, 2);
        assert_eq!(cursor_store.load("gmail").await.unwrap(), Some(102));
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
        cs.save("gmail_personal", 12345).await.unwrap();
        let loaded = cs.load("gmail_personal").await.unwrap();
        assert_eq!(loaded, Some(12345));
        // Overwrite via UPSERT
        cs.save("gmail_personal", 67890).await.unwrap();
        assert_eq!(cs.load("gmail_personal").await.unwrap(), Some(67890));
        // Missing account → None, no error
        assert_eq!(cs.load("never_seen").await.unwrap(), None);
    }
}
