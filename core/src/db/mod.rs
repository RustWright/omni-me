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
        DEFINE INDEX IF NOT EXISTS idx_events_timestamp ON events FIELDS timestamp;
        DEFINE INDEX IF NOT EXISTS idx_events_aggregate ON events FIELDS aggregate_id;
        DEFINE INDEX IF NOT EXISTS idx_events_device ON events FIELDS device_id;

        DEFINE TABLE IF NOT EXISTS sync_state SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS device_id ON sync_state TYPE string;
        DEFINE FIELD IF NOT EXISTS last_sync_timestamp ON sync_state TYPE datetime;
        DEFINE INDEX IF NOT EXISTS idx_sync_device ON sync_state FIELDS device_id UNIQUE;
        ",
    )
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
