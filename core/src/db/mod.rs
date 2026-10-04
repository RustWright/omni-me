mod error;
pub mod queries;

pub use error::DbError;

use surrealdb::Surreal;
use surrealdb::engine::local::{Db, SurrealKv};

/// Re-exported database handle type. Consumers use this instead of importing surrealdb directly.
pub type Database = Surreal<Db>;

const NAMESPACE: &str = "omni";
const DATABASE: &str = "main";

/// Connect to an embedded SurrealDB instance at the given path.
/// Creates the database file if it doesn't exist, selects namespace/db,
/// and initializes the schema.
pub async fn connect(path: &str) -> Result<Surreal<Db>, DbError> {
    let db = Surreal::new::<SurrealKv>(path)
        .await
        .map_err(DbError::Connection)?;

    db.use_ns(NAMESPACE)
        .use_db(DATABASE)
        .await
        .map_err(DbError::Connection)?;

    init_schema(&db).await?;

    Ok(db)
}

async fn init_schema(db: &Surreal<Db>) -> Result<(), DbError> {
    db.query(
        "
        DEFINE TABLE IF NOT EXISTS events SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS event_type ON events TYPE string;
        DEFINE FIELD IF NOT EXISTS aggregate_id ON events TYPE string;
        DEFINE FIELD IF NOT EXISTS timestamp ON events TYPE datetime;
        DEFINE FIELD IF NOT EXISTS device_id ON events TYPE string;
        DEFINE FIELD IF NOT EXISTS payload ON events TYPE object FLEXIBLE;
        -- When THIS node stored the event, as opposed to `timestamp`, which is
        -- when the authoring device *wrote* it. Sync cursors key on this: a
        -- cursor handed out by one node has to be compared against a clock that
        -- node owns, or events silently fall below it forever. `option<>` so
        -- rows written before this field existed still load; the backfill below
        -- fills them in.
        DEFINE FIELD IF NOT EXISTS received_at ON events TYPE option<datetime>;
        DEFINE INDEX IF NOT EXISTS idx_events_timestamp ON events FIELDS timestamp;
        DEFINE INDEX IF NOT EXISTS idx_events_received_at ON events FIELDS received_at;
        DEFINE INDEX IF NOT EXISTS idx_events_aggregate ON events FIELDS aggregate_id;
        DEFINE INDEX IF NOT EXISTS idx_events_device ON events FIELDS device_id;

        DEFINE TABLE IF NOT EXISTS sync_state SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS device_id ON sync_state TYPE string;
        DEFINE FIELD IF NOT EXISTS last_sync_timestamp ON sync_state TYPE datetime;
        DEFINE INDEX IF NOT EXISTS idx_sync_device ON sync_state FIELDS device_id UNIQUE;
        -- Push watermark, kept separate from `last_sync_timestamp` (the pull
        -- cursor). They advance on different clocks and conflating them meant
        -- the background pusher used a post-pull *server* cursor as its push
        -- `since`, skipping local work at or below it.
        DEFINE FIELD IF NOT EXISTS last_push_received_at ON sync_state TYPE option<datetime>;

        -- The one text analyzer, shared by every full-text index in the app.
        --
        -- It lives here rather than in a projection because an index naming a
        -- missing analyzer is a schema error, and projections define their
        -- tables in an order this module does not control. Defining it during
        -- `connect` puts it ahead of all of them.
        --
        -- `class` splits on character class so `2026-03-14` yields its parts;
        -- `blank` splits on whitespace. `lowercase` + `ascii` make matching
        -- case- and accent-insensitive.
        DEFINE ANALYZER IF NOT EXISTS omni_text
            TOKENIZERS class, blank
            FILTERS lowercase, ascii;
        ",
    )
    .await
    .map_err(DbError::Schema)?
    .check()
    .map_err(DbError::Schema)?;

    // One-time backfill for events stored before `received_at` existed. The
    // author timestamp is the only approximation available, and it is the right
    // one: those events were already exchanged under the old author-clock rule,
    // so seeding the new watermark from it keeps them below both cursors instead
    // of stranding them (NONE compares false against any bound, which would mean
    // local events could never push again).
    db.query("UPDATE events SET received_at = timestamp WHERE received_at IS NONE")
        .await
        .map_err(DbError::Schema)?
        .check()
        .map_err(DbError::Schema)?;

    Ok(())
}

/// A fresh, empty database for one test: its own namespace on one instance
/// shared by the test binary. A `connect` per test leaks an engine thread
/// each, which wedged the suite; see `docs/src/testing.md`.
#[cfg(test)]
pub(crate) async fn test_db() -> Database {
    use std::sync::LazyLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    // The engine runs on the runtime that connects it, and a #[tokio::test]
    // runtime ends with its test, so the shared instance gets the one the
    // binaries use, which also carries the stack the engine needs.
    static RUNTIME: LazyLock<tokio::runtime::Runtime> =
        LazyLock::new(|| crate::async_runtime::build().expect("test database runtime"));
    static SHARED: tokio::sync::OnceCell<Database> = tokio::sync::OnceCell::const_new();
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let shared = SHARED
        .get_or_init(|| async {
            RUNTIME
                .spawn(async {
                    let dir = tempfile::tempdir().expect("test database dir").keep();
                    Surreal::new::<SurrealKv>(dir.join("shared.db").to_str().unwrap())
                        .await
                        .expect("test database")
                })
                .await
                .expect("test database task")
        })
        .await;
    let db = shared.clone();
    let ns = format!("test_{}", NEXT.fetch_add(1, Ordering::Relaxed));
    db.use_ns(ns).use_db(DATABASE).await.unwrap();
    init_schema(&db).await.unwrap();
    db
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the shared fixture rests on: a write in one test's namespace is
    /// invisible to another's, including the projection bookmarks.
    #[tokio::test]
    async fn test_databases_do_not_see_each_others_rows() {
        use crate::events::{BudgetProjection, ProjectionRunner};

        let (a, b) = (test_db().await, test_db().await);
        for db in [&a, &b] {
            ProjectionRunner::new(db.clone(), vec![Box::new(BudgetProjection)])
                .init_all()
                .await
                .unwrap();
        }
        a.query(
            "CREATE events CONTENT { event_type: 'x', aggregate_id: 'a', \
             timestamp: time::now(), device_id: 'd', payload: {} };
             CREATE projection_versions CONTENT { name: 'only_in_a', version: 1, last_event_id: '' };",
        )
        .await
        .unwrap()
        .check()
        .unwrap();

        let count = |db: &Database, query: &'static str| {
            let db = db.clone();
            async move {
                let n: Option<usize> = db.query(query).await.unwrap().take("n").unwrap();
                n.unwrap_or(0)
            }
        };
        let events = "SELECT count() AS n FROM events GROUP ALL";
        let bookmark =
            "SELECT count() AS n FROM projection_versions WHERE name = 'only_in_a' GROUP ALL";
        assert_eq!(count(&a, events).await, 1);
        assert_eq!(count(&b, events).await, 0);
        assert_eq!(count(&a, bookmark).await, 1);
        assert_eq!(count(&b, bookmark).await, 0);
    }

    #[tokio::test]
    async fn test_connect_and_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        let db = connect(path.to_str().unwrap()).await.unwrap();

        // Verify we can insert into the events table
        let result: Vec<surrealdb::types::RecordId> = db
            .query(
                "CREATE events CONTENT {
                    event_type: 'test_event',
                    aggregate_id: 'test-123',
                    timestamp: d'2026-03-24T12:00:00Z',
                    device_id: 'device-1',
                    payload: { key: 'value' }
                } RETURN id",
            )
            .await
            .unwrap()
            .take("id")
            .unwrap();

        assert_eq!(result.len(), 1);

        // Verify we can query it back
        let count: Option<usize> = db
            .query("SELECT count() AS total FROM events GROUP ALL")
            .await
            .unwrap()
            .take("total")
            .unwrap();

        assert_eq!(count, Some(1));
    }

    /// The trap every write in this crate has to know about: a statement the
    /// database *rejects* still comes back as `Ok`.
    ///
    /// `await?` only forwards transport and parse failures. A statement error
    /// travels inside the `Response` and reaches the caller solely through
    /// `.check()` or `.take()`. A write that discards the response therefore
    /// cannot fail, which reads in the source exactly like a write that
    /// succeeded — the projections were built on that misreading.
    #[tokio::test]
    async fn a_rejected_statement_still_returns_ok_until_it_is_checked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = connect(path.to_str().unwrap()).await.unwrap();

        // `sync_state.device_id` carries a UNIQUE index, so the second write of
        // the same device is refused. Chosen over a SCHEMAFULL type violation
        // because an index conflict is a statement error in every version, where
        // what SCHEMAFULL does with an undeclared field has moved between them.
        let insert = "CREATE sync_state CONTENT \
                      { device_id: 'dup', last_sync_timestamp: d'2026-01-01T00:00:00Z' }";
        db.query(insert)
            .await
            .unwrap()
            .check()
            .expect("the first write is legal");

        let response = db
            .query(insert)
            .await
            .expect("the transport succeeded, which is all `await?` can report");

        assert!(
            response.check().is_err(),
            "the write was refused, and `check` is the only thing that says so"
        );
    }
}
