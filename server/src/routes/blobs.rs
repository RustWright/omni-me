use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::put,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::AppState;

const MAX_BLOB_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum BlobError {
    #[error("hash must be 64 hex characters (sha-256)")]
    InvalidHashFormat,
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("blob not found")]
    NotFound,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for BlobError {
    fn into_response(self) -> Response {
        let status = match &self {
            BlobError::InvalidHashFormat | BlobError::HashMismatch { .. } => {
                StatusCode::BAD_REQUEST
            }
            BlobError::NotFound => StatusCode::NOT_FOUND,
            BlobError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.to_string()).into_response()
    }
}

pub fn blob_routes() -> Router<AppState> {
    Router::new()
        .route("/blobs/{hash}", put(put_blob_handler).get(get_blob_handler))
        .layer(DefaultBodyLimit::max(MAX_BLOB_BYTES))
}

fn validate_hash_format(hash: &str) -> Result<String, BlobError> {
    let hash = hash.to_lowercase();
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(BlobError::InvalidHashFormat);
    }
    Ok(hash)
}

async fn put_blob_handler(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    body: Bytes,
) -> Result<StatusCode, BlobError> {
    let hash = validate_hash_format(&hash)?;
    let final_path = state.blob_dir.join(&hash);

    if tokio::fs::try_exists(&final_path).await? {
        return Ok(StatusCode::OK);
    }

    let actual = Sha256::digest(&body)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if actual != hash {
        return Err(BlobError::HashMismatch {
            expected: hash,
            actual,
        });
    }

    let tmp = state.blob_dir.join(format!(".tmp-{}", ulid::Ulid::new()));
    tokio::fs::write(&tmp, &body).await?;
    tokio::fs::rename(&tmp, &final_path).await?;
    Ok(StatusCode::CREATED)
}

async fn get_blob_handler(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Response, BlobError> {
    let hash = validate_hash_format(&hash)?;
    let path = state.blob_dir.join(&hash);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(BlobError::NotFound),
        Err(e) => return Err(BlobError::Io(e)),
    };

    let mime = infer::get(&bytes)
        .map(|t| t.mime_type())
        .unwrap_or("application/octet-stream");

    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_LENGTH, &bytes.len().to_string()),
        ],
        bytes,
    )
        .into_response())
}
