use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use serde::{Deserialize, Serialize};

use omni_me_core::archive;
use omni_me_core::events::{AttachmentRef, NewEvent};
use omni_me_core::extraction::document::{reading_from_extraction, to_fields_payload};
use omni_me_core::extraction::{
    DEFAULT_CONFIDENCE_THRESHOLD, DocumentPart, ExtractionHint, ExtractionResult, add_counter_legs,
    verify,
};

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
    /// What the receipt cross-check found, such as line items not adding up to the total.
    pub warnings: Vec<String>,
    pub needs_review: bool,
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

    let ingested = archive::ingest_one(
        &state.blob_dir,
        &body,
        filename,
        mime,
        source,
        &state.device_id,
        // ⛔ A single uploaded file has no container. Only email ingest nests.
        None,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Append then project. Note this path is NOT feature-gated: the server resolves
    // no `ResolvedConfig`, so `EventWriter`'s guard has nothing to read here.
    // Pre-existing and shared with every auto-import source — see `tasks.md`.
    //
    // ⚠️ **One batch, not one call per event.** The fields event is about the
    // document the archive event creates; appending them separately would let a
    // crash in between leave fields for a document nothing else records.
    let appended = state
        .store
        .append_batch(ingested.events)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("append: {e}")))?;
    // Best-effort once stored: an error response here makes the device retry, and the
    // retry files the same bytes as a second document.
    let failed = state.projections.apply_events_resilient(&appended).await;
    if failed > 0 {
        tracing::warn!(
            document_id = %ingested.document_id,
            failed,
            "archived but not all projected"
        );
    }

    tracing::info!(
        document_id = %ingested.document_id,
        bytes = body.len(),
        mime = %mime,
        text_source = %ingested.text_source.as_str(),
        parsed_fields = ingested.parsed_fields,
        "document_archived"
    );

    Ok(Json(ArchiveResponse {
        document_id: ingested.document_id,
        sha256: ingested.sha256,
        text_source: ingested.text_source.as_str().to_string(),
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

    let mut extraction = state
        .extractor
        .extract(&[DocumentPart::new(&body, mime)], q.hint)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Archived only after extraction succeeds, as the bare blob store was: a failed read is
    // retried by the device with the same bytes, and archiving first would file each retry.
    let attachment = if q.attach {
        Some(archive_capture(&state, &body, mime, filename, &extraction, q.hint).await?)
    } else {
        None
    };

    // Before the counter leg, which would cancel the line-item sum this compares to the total.
    // It used to run only in the extraction bench, never on a capture.
    let report = verify(&extraction, q.hint, DEFAULT_CONFIDENCE_THRESHOLD);
    extraction.confidence = report.effective_confidence;
    add_counter_legs(&mut extraction, q.hint);

    Ok(Json(ExtractResponse {
        extraction,
        attachment,
        warnings: report.warnings,
        needs_review: report.needs_manual_review,
    }))
}

/// File a capture in the archive and return the attachment that links a transaction to it.
///
/// The extraction's reading goes in the same batch, so the reader never re-reads a capture;
/// what it leaves (a capture with no document type) the scheduled pass catalogues as usual.
async fn archive_capture(
    state: &AppState,
    body: &[u8],
    mime: &str,
    filename: &str,
    extraction: &ExtractionResult,
    hint: ExtractionHint,
) -> Result<AttachmentRef, (StatusCode, String)> {
    let internal = |e: String| (StatusCode::INTERNAL_SERVER_ERROR, e);
    let mut ingested = archive::ingest_one(
        &state.blob_dir,
        body,
        filename,
        mime,
        archive::IngestSource::Scan,
        &state.device_id,
        None,
    )
    .await
    .map_err(|e| internal(e.to_string()))?;

    if let Some(reading) = reading_from_extraction(extraction, hint) {
        let payload = to_fields_payload(&ingested.document_id, &reading);
        let event = NewEvent::document_fields_extracted(&state.device_id, &payload)
            .map_err(|e| internal(e.to_string()))?;
        ingested.events.push(event);
    }

    let appended = state
        .store
        .append_batch(ingested.events)
        .await
        .map_err(|e| internal(format!("append: {e}")))?;
    let failed = state.projections.apply_events_resilient(&appended).await;
    if failed > 0 {
        tracing::warn!(document_id = %ingested.document_id, failed, "capture archived but not all projected");
    }

    Ok(AttachmentRef {
        sha256: ingested.sha256,
        filename: filename.to_string(),
        mime_type: mime.to_string(),
        size: body.len() as u64,
        document_id: Some(ingested.document_id),
    })
}
