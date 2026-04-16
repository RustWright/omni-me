#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database connection error: {0}")]
    Connection(surrealdb::Error),
    #[error("database query error: {0}")]
    Query(#[from] surrealdb::Error),
    #[error("schema initialization error: {0}")]
    Schema(surrealdb::Error),
}
