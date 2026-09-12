use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use serde::{Deserialize, Serialize};

use omni_me_core::archive;
use omni_me_core::blob;
use omni_me_core::events::{AttachmentRef, DocumentArchivedPayload};
use omni_me_core::extraction::{ExtractionHint, ExtractionResult};

use crate::AppState;

const MAX_DOCUMENT_BYTES: usize = 15 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct ExtractQuery {
    pub hint: ExtractionHint,
    /// When true, the handler also writes `body` to the blob dir (idempotent,
    /// same on-disk shape as `PUT /blobs/{hash}`) and returns the resulting
    /// `AttachmentRef` so the client can persist it on the
    /// `TransactionRecorded` event without a second upload.
    #[serde(default)]
    pub attach: bool,
}

/// Wrapper so the response shape stays stable whether or not `attach=true`.
/// Frontend always parses this; `attachment = None` when `attach` is false.
#[derive(Debug, Serialize)]
pub struct ExtractResponse {
    pub extraction: ExtractionResult,
    pub attachment: Option<AttachmentRef>,
}

pub fn documents_routes() -> Router<AppState> {
    Router::new()
        .route("/documents/extract", post(extract_handler))
        .route("/documents/archive", post(archive_handler))
        .layer(DefaultBodyLimit::max(MAX_DOCUMENT_BYTES))
}

#[derive(Debug, Deserialize)]
pub struct ArchiveQuery {
    /// How the document reached us: `scan` | `upload` | `email` | `bulk`.
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "upload".to_string()
}

#[derive(Debug, Serialize)]
pub struct ArchiveResponse {
    pub document_id: String,
    pub sha256: String,
    /// `extracted` | `transcribed` | `none`.
    ///
    /// Returned because it is the difference between a document the archive can
    /// find by content and one it can only find by name, and the caller is the
    /// only party in a position to tell the user which they just filed.
    pub text_source: String,
}

/// `POST /documents/archive?source=<scan|upload|email|bulk>`
///
/// Body: raw document bytes. `Content-Type` carries the MIME; `x-filename` the
/// name to file it under.
///
/// ⚠️ **Archiving is not extracting.** This stores the bytes and whatever text
/// they already carry, and deliberately runs no model — a document nothing can
/// read is archived all the same, and gains fields later from a separate event.
/// ⛔ Do not fold `/documents/extract` into this: that route answers "what does
/// this receipt say", which is a different question with a finance-shaped answer.
async fn archive_handler(
    State(state): State<AppState>,
    Query(q): Query<ArchiveQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ArchiveResponse>, (StatusCode, String)> {
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    let filename = headers
        .get("x-filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("document");

    let source = parse_ingest_source(&q.source).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            format!("unknown source: {}", q.source),
        )
    })?;

    let (event, text_source) = archive::ingest_one(
        &state.blob_dir,
        &body,
        filename,
        mime,
        source,
        &state.device_id,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let document_id = event.aggregate_id.clone();
    let payload: DocumentArchivedPayload = serde_json::from_value(event.payload.clone())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Append then project, matching `auto_import::rest`. ⚠️ Note this path is
    // NOT feature-gated: the server resolves no `ResolvedConfig`, so
    // `EventWriter`'s guard has nothing to read here. Pre-existing and shared
    // with every auto-import source — see `tasks.md`.
    let appended = state
        .store
        .append_batch(vec![event])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("append: {e}")))?;
    state
        .projections
        .apply_events(&appended)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("project: {e}")))?;

    tracing::info!(
        document_id = %document_id,
        bytes = body.len(),
        mime = %mime,
        text_source = %payload.text_source,
        "document_archived"
    );

    Ok(Json(ArchiveResponse {
        document_id,
        sha256: payload.sha256,
        text_source: text_source.as_str().to_string(),
    }))
}

/// ⛔ Rejects an unknown value rather than defaulting to one. Silently filing a
/// typo'd `source` as `upload` would put a wrong provenance on a permanent event.
fn parse_ingest_source(s: &str) -> Option<archive::IngestSource> {
    match s {
        "scan" => Some(archive::IngestSource::Scan),
        "upload" => Some(archive::IngestSource::Upload),
        "email" => Some(archive::IngestSource::Email),
        "bulk" => Some(archive::IngestSource::Bulk),
        _ => None,
    }
}

/// `POST /documents/extract?hint=<receipt|bank_statement|...>&attach=true`
/// Body: raw document bytes. `Content-Type` header carries the MIME used to
/// route the extractor (image/jpeg → photo path, application/pdf → PDF path).
async fn extract_handler(
    State(state): State<AppState>,
    Query(q): Query<ExtractQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ExtractResponse>, (StatusCode, String)> {
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    let filename = headers
        .get("x-filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("attachment");

    tracing::info!(
        bytes = body.len(),
        mime = %mime,
        hint = ?q.hint,
        attach = q.attach,
        "extract_document"
    );

    let extraction = state
        .extractor
        .extract(&body, mime, q.hint)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let attachment = if q.attach {
        Some(
            store_blob(&state, &body, mime, filename)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?,
        )
    } else {
        None
    };

    Ok(Json(ExtractResponse {
        extraction,
        attachment,
    }))
}

/// Build an `AttachmentRef` for bytes stored through `core::blob`.
///
/// The hashing, temp-then-rename and idempotency all live there now — this is
/// the metadata the caller wants back, which the store has no business knowing:
/// a filename and a declared MIME are what the *request* said, not properties of
/// the bytes.
async fn store_blob(
    state: &AppState,
    body: &[u8],
    mime: &str,
    filename: &str,
) -> Result<AttachmentRef, String> {
    let sha256 = blob::store(&state.blob_dir, body)
        .await
        .map_err(|e| e.to_string())?;

    Ok(AttachmentRef {
        sha256,
        filename: filename.to_string(),
        mime_type: mime.to_string(),
        size: body.len() as u64,
    })
}
