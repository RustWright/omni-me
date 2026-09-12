use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::put,
};
use omni_me_core::blob;
use thiserror::Error;

use crate::AppState;

const MAX_BLOB_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum BlobError {
    #[error("hash must be 64 hex characters (sha-256)")]
    InvalidHashFormat,
    #[error("blob not found")]
    NotFound,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Blob(#[from] blob::BlobError),
}

impl IntoResponse for BlobError {
    fn into_response(self) -> Response {
        let status = match &self {
            BlobError::InvalidHashFormat => StatusCode::BAD_REQUEST,
            BlobError::NotFound => StatusCode::NOT_FOUND,
            BlobError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // The store's own errors keep the same split this enum already
            // makes: a caller's bad hash is a 400, a failed write is a 500.
            BlobError::Blob(blob::BlobError::InvalidHash)
            | BlobError::Blob(blob::BlobError::HashMismatch { .. }) => StatusCode::BAD_REQUEST,
            BlobError::Blob(blob::BlobError::Io { .. }) => StatusCode::INTERNAL_SERVER_ERROR,
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
    // Storage semantics live in `core::blob` — hash check, temp-then-rename,
    // idempotent. This handler's own job is the status code: `CREATED` when the
    // blob is new, `OK` when it was already there.
    match blob::store_as(&state.blob_dir, &hash, &body).await {
        Ok(true) => Ok(StatusCode::CREATED),
        Ok(false) => Ok(StatusCode::OK),
        Err(e) => Err(e.into()),
    }
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

    // `nosniff` + an attachment disposition.
    //
    // The content type here is *sniffed from the bytes*, and blob PUT accepts
    // any content that matches its claimed hash — so without these headers,
    // storing a blob that sniffs as `text/html` and then opening its URL
    // executes script on the server's own origin, with whatever that origin can
    // reach. `nosniff` stops the browser second-guessing the type, and
    // `attachment` stops it rendering the response as a document at all. The
    // app's own reader fetches these bytes programmatically, so neither header
    // changes anything for the legitimate caller.
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_LENGTH, &bytes.len().to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CONTENT_DISPOSITION, "attachment"),
        ],
        bytes,
    )
        .into_response())
}
