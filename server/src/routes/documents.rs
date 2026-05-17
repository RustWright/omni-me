use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use serde::Deserialize;

use omni_me_core::extraction::{ExtractionHint, ExtractionResult};

use crate::AppState;

const MAX_DOCUMENT_BYTES: usize = 15 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct ExtractQuery {
    pub hint: ExtractionHint,
}

pub fn documents_routes() -> Router<AppState> {
    Router::new()
        .route("/documents/extract", post(extract_handler))
        .layer(DefaultBodyLimit::max(MAX_DOCUMENT_BYTES))
}

/// `POST /documents/extract?hint=<receipt|bank_statement|...>`
/// Body: raw document bytes. `Content-Type` header carries the MIME used to
/// route the extractor (image/jpeg → photo path, application/pdf → PDF path).
async fn extract_handler(
    State(state): State<AppState>,
    Query(q): Query<ExtractQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ExtractionResult>, (StatusCode, String)> {
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    tracing::info!(bytes = body.len(), mime = %mime, hint = ?q.hint, "extract_document");

    state
        .extractor
        .extract(&body, mime, q.hint)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}
