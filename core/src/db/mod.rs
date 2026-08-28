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
        ",
    )
    .await
    .map_err(DbError::Schema)?;

    // One-time backfill for events stored before `received_at` existed. The
    // author timestamp is the only approximation available, and it is the right
    // one: those events were already exchanged under the old author-clock rule,
    // so seeding the new watermark from it keeps them below both cursors instead
    // of stranding them (NONE compares false against any bound, which would mean
    // local events could never push again).
    db.query("UPDATE events SET received_at = timestamp WHERE received_at IS NONE")
        .await
        .map_err(DbError::Schema)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
