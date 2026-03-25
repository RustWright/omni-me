use std::fmt;

#[derive(Debug)]
pub enum DbError {
    Connection(String),
    Query(String),
    Schema(String),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Connection(msg) => write!(f, "database connection error: {msg}"),
            DbError::Query(msg) => write!(f, "database query error: {msg}"),
            DbError::Schema(msg) => write!(f, "schema initialization error: {msg}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<surrealdb::Error> for DbError {
    fn from(err: surrealdb::Error) -> Self {
        DbError::Query(err.to_string())
    }
}
