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
use chrono::NaiveDate;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::auto_import_scheduler::{
    AutoImportSource, CursorResetOutcome, DropReason, ImportError, ImportSummary, ImportTally,
};
use crate::db::Database;
use crate::events::{EventStore, ProjectionRunner};
use surrealdb::types::SurrealValue;

use super::imap::{
    ArchiveTarget, FetchCursor, ImapFetcher, ImapHandler, cursor_for_anchor, poll_once,
};

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

/// One `imap_cursors` row. Decoded as a row so a stored `NONE` lands in the
/// `Option` rather than failing the whole read — see the note in `load`.
#[derive(Debug, SurrealValue)]
struct CursorRow {
    uid: i64,
    uid_validity: Option<i64>,
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
        // ⚠️ Through a serde row, not two `take("<field>")` calls. Taking a single
        // nullable field into `Option<i64>` fails outright on a stored `NONE` —
        // "Expected int, got none" — so the row written before `uid_validity`
        // existed would make `load` error, `ImapSource::new` fail, and the account
        // vanish from the registry rather than poll with an unknown validity.
        let rows: Vec<CursorRow> = resp
            .take(0)
            .map_err(|e| ImportError::Upstream(format!("decode cursor: {e}")))?;
        Ok(rows.into_iter().next().map(|r| StoredCursor {
            uid: r.uid as u32,
            uid_validity: r.uid_validity.map(|v| v as u32),
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
    archive: Option<ArchiveConfig>,
}

/// What archiving a fetched message needs.
///
/// A named struct rather than a tuple since the PDF passwords joined it: reading
/// `(PathBuf, String, PdfPasswords)` at a call site tells you nothing about which
/// string is the device.
#[derive(Debug, Clone)]
pub struct ArchiveConfig {
    pub blob_dir: std::path::PathBuf,
    pub device_id: String,
    /// Tried in turn on an encrypted PDF attachment. See
    /// [`crate::credentials::PdfPasswords`].
    pub pdf_passwords: crate::credentials::PdfPasswords,
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
        archive: Option<ArchiveConfig>,
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
        let target = self.archive.as_ref().map(|a| ArchiveTarget {
            blob_dir: a.blob_dir.as_path(),
            device_id: &a.device_id,
            passwords: &a.pdf_passwords,
        });
        let mut outcome = poll_once(
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
            super::duplicates::flag_likely_duplicates(self.projections.db(), &mut outcome.events)
                .await;
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

    async fn reset_cursor(
        &self,
        since: Option<NaiveDate>,
    ) -> Result<CursorResetOutcome, ImportError> {
        // ⛔ Rewound to zero, NOT deleted — and the difference is the whole
        // behaviour. An absent cursor means "never polled" to every fetcher here,
        // and that case deliberately anchors to the mailbox's newest message so a
        // new account does not back-import years of mail. Deleting the row would
        // therefore move the source *forward* to now, which is the opposite of
        // what a rewind promises. `uid_range` holds this line.
        const REWOUND: u32 = 0;

        // A date is located in the mailbox's numbering as it is now, so the validity
        // stored with it is the one just observed, not the one on file.
        let anchor = match since {
            Some(date) => Some(self.fetcher.first_uid_since(date).await?),
            None => None,
        };
        let target = anchor.as_ref().map_or(REWOUND, cursor_for_anchor);
        let observed_validity = anchor.and_then(|a| a.uid_validity);

        // Both halves, and neither alone is enough. The row is what a restart
        // reads; the in-memory cursor is what this process reads *and* what the
        // next pull writes back — so rewinding only the row leaves a running
        // source able to re-save the position it still holds.
        let stored = match &self.cursor_store {
            Some(cs) => {
                let existing = cs.load(&self.name).await?;
                cs.save(
                    &self.name,
                    StoredCursor {
                        uid: target,
                        // Kept: the mailbox has not been renumbered by us asking
                        // to re-read it, and dropping it would make the next tick
                        // treat the validity as unknown.
                        uid_validity: observed_validity.or(existing.and_then(|c| c.uid_validity)),
                    },
                )
                .await?;
                existing
            }
            None => None,
        };

        let previous = {
            let mut guard = self.cursor.lock().await;
            let was = guard.last_seen_uid;
            guard.last_seen_uid = Some(target);
            if observed_validity.is_some() {
                guard.uid_validity = observed_validity;
            }
            was
        };

        // The row wins when both carry a position: it is the one that survives a
        // restart. The in-memory value is what a store-less source (tests, and any
        // future caller passing `None`) has instead.
        let from = stored.map(|c| c.uid).or(previous);

        Ok(match (anchor, from) {
            (Some(_), from) => CursorResetOutcome::MovedToDate {
                from_uid: from,
                to_uid: target,
            },
            // Reported as where it was, not where it now is: "rewound past 4,812"
            // is checkable against the mailbox; "set to 0" is not.
            (None, Some(uid)) if uid != REWOUND => CursorResetOutcome::Rewound { from_uid: uid },
            _ => CursorResetOutcome::AlreadyAtStart,
        })
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

    /// A rewind leaves the cursor at zero, and zero is not the same as absent.
    ///
    /// ⚠️ This deliberately does NOT claim "the next pull re-fetches", which it
    /// cannot: `MockFetcher::fetch_new` ignores the cursor entirely and replays a
    /// script, so a second pull returning the same messages would prove only that
    /// the script agrees with the test. What decides re-fetching is the UID range,
    /// and `uid_range` in `imap_real` is where that is asserted.
    #[tokio::test]
    async fn a_rewind_leaves_the_cursor_at_zero_rather_than_absent() {
        let (_db, store, projections) = test_db_runner().await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        fetcher.push_response(vec![make_msg(101, "x@a.com")], Some(101));
        let cursor_store: Arc<dyn CursorStore> = Arc::new(MemCursorStore::new());
        let handlers: Vec<Box<dyn ImapHandler>> = vec![Box::new(NeedleHandler {
            name: "x".into(),
            needle: "x@a.com".into(),
        })];
        let source = ImapSource::new(
            "gmail",
            fetcher.clone(),
            handlers,
            Some(cursor_store.clone()),
            store.clone(),
            projections.clone(),
            None,
        )
        .await
        .unwrap();

        source.pull().await.unwrap();
        assert_eq!(
            cursor_store.load("gmail").await.unwrap().map(|c| c.uid),
            Some(101),
            "the first pull should have recorded where it got to"
        );

        let outcome = source.reset_cursor(None).await.unwrap();
        assert_eq!(
            outcome,
            CursorResetOutcome::Rewound { from_uid: 101 },
            "the rewind reports where it was, not just success"
        );
        assert_eq!(
            cursor_store.load("gmail").await.unwrap().map(|c| c.uid),
            Some(0),
            "⛔ the row must survive holding 0. Deleted, it reads as never-polled, \
             and never-polled anchors to the newest message — forward, not back"
        );
        assert_eq!(
            source.cursor.lock().await.last_seen_uid,
            Some(0),
            "⚠️ the in-memory cursor is what the next pull writes back — leaving it \
             set lets a running source re-save the position just rewound past"
        );
    }

    /// Rewinding twice is not an error, and the second answer differs from the
    /// first — `AlreadyAtStart` says there was nowhere to rewind from, which is a
    /// different claim from "I rewound past 101".
    #[tokio::test]
    async fn rewinding_a_source_that_never_polled_is_already_at_start() {
        let (_db, store, projections) = test_db_runner().await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        let cursor_store: Arc<dyn CursorStore> = Arc::new(MemCursorStore::new());
        let source = ImapSource::new(
            "never-polled",
            fetcher,
            vec![],
            Some(cursor_store),
            store,
            projections,
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            source.reset_cursor(None).await.unwrap(),
            CursorResetOutcome::AlreadyAtStart
        );
    }

    /// A date rewind lands one below the first match, in both halves, under the
    /// validity the search observed rather than the one on file.
    #[tokio::test]
    async fn a_date_rewind_stops_just_before_the_first_message_since() {
        use super::super::imap::DateAnchor;
        let (_db, store, projections) = test_db_runner().await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        fetcher.set_anchor(DateAnchor {
            first_uid: Some(14_500),
            highest_uid: Some(14_878),
            uid_validity: Some(7),
        });
        let cursor_store: Arc<dyn CursorStore> = Arc::new(MemCursorStore::new());
        cursor_store
            .save(
                "gmail",
                StoredCursor {
                    uid: 400,
                    uid_validity: Some(7),
                },
            )
            .await
            .unwrap();
        let source = ImapSource::new(
            "gmail",
            fetcher,
            vec![],
            Some(cursor_store.clone()),
            store,
            projections,
            None,
        )
        .await
        .unwrap();

        let since = NaiveDate::from_ymd_opt(2026, 8, 28);
        assert_eq!(
            source.reset_cursor(since).await.unwrap(),
            CursorResetOutcome::MovedToDate {
                from_uid: Some(400),
                to_uid: 14_499,
            },
        );
        assert_eq!(
            cursor_store.load("gmail").await.unwrap(),
            Some(StoredCursor {
                uid: 14_499,
                uid_validity: Some(7),
            }),
        );
        assert_eq!(source.cursor.lock().await.last_seen_uid, Some(14_499));
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
