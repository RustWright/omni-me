#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database connection error: {0}")]
    Connection(surrealdb::Error),
    #[error("database query error: {0}")]
    Query(#[from] surrealdb::Error),
    #[error("schema initialization error: {0}")]
    Schema(surrealdb::Error),
}

impl DbError {
    /// A write conflict SurrealKV reports as safe to retry. The same statement
    /// run again can succeed; anything else will fail the same way.
    pub fn is_retryable_conflict(&self) -> bool {
        matches!(
            self,
            DbError::Query(e)
                if e.query_details() == Some(&surrealdb::types::QueryError::TransactionConflict)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use surrealdb::types::QueryError;

    #[test]
    fn only_a_transaction_conflict_is_retryable() {
        let conflict = surrealdb::Error::query("busy".into(), QueryError::TransactionConflict);
        assert!(DbError::Query(conflict).is_retryable_conflict());
        let other = surrealdb::Error::query("bad".into(), None::<QueryError>);
        assert!(!DbError::Query(other).is_retryable_conflict());
        let internal = surrealdb::Error::internal("table missing".into());
        assert!(!DbError::Query(internal).is_retryable_conflict());
    }
}
