use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Multipart, Query, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use serde::{Deserialize, Serialize};

use omni_me_core::archive;
use omni_me_core::auto_import::to_proposed_event;
use omni_me_core::credentials::PdfPasswords;
use omni_me_core::events::{AttachmentRef, EventWriter, NewEvent};
use omni_me_core::extraction::document::{reading_from_extraction, to_fields_payload};
use omni_me_core::extraction::event_mapper::receipt_extraction_to_drafts;
use omni_me_core::extraction::{
    DEFAULT_CONFIDENCE_THRESHOLD, DocumentKind, DocumentPart, ExtractionHint, ExtractionResult,
    TotalCheck, add_counter_legs, verify,
};
use omni_me_core::purge;

use crate::AppState;

/// The blob dir, device and PDF passwords every ingest here files under.
///
/// `passwords` is borrowed rather than resolved inside, because
/// [`archive::IngestContext`] holds a reference and a temporary would not outlive
/// the call. The same `secrets` map the statement-upload route resolves a *named*
/// password from — here every configured one is tried, since an uploaded file
/// names none.
fn ingest_context<'a>(
    state: &'a AppState,
    passwords: &'a PdfPasswords,
) -> archive::IngestContext<'a> {
    archive::IngestContext {
        blob_dir: &state.blob_dir,
        device_id: &state.device_id,
        passwords,
    }
}

const MAX_DOCUMENT_BYTES: usize = 15 * 1024 * 1024;
const MAX_CAPTURE_PAGES_BYTES: usize = 40 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct ExtractQuery {
    pub hint: ExtractionHint,
    /// When true, the handler also writes `body` to the blob dir (idempotent,
    /// same on-disk shape as `PUT /blobs/{hash}`) and returns the resulting
    /// `AttachmentRef` so the client can persist it on the
    /// `TransactionRecorded` event without a second upload.
    #[serde(default)]
    pub attach: bool,
    /// Archive's "Add document": when the reading is a receipt, also propose it
    /// for review, as a mailed receipt would be. Needs `attach`.
    #[serde(default)]
    pub propose: bool,
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
    /// Whether the total cross-check compared anything independent. An empty
    /// `warnings` alone does not mean the arithmetic was verified.
    pub total_check: TotalCheck,
    /// Set when `propose` filed the capture for review.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_batch_id: Option<String>,
}

pub fn documents_routes() -> Router<AppState> {
    Router::new()
        .route("/documents/extract", post(extract_handler))
        // Eight phone photos outgrow the router's cap; the PDF they become does not.
        .route(
            "/documents/extract_pages",
            post(extract_pages_handler).layer(DefaultBodyLimit::max(MAX_CAPTURE_PAGES_BYTES)),
        )
        .route("/documents/archive", post(archive_handler))
        // ⛔ Two routes, never one. The preview is what the user reads; the
        // confirm may only act on what that preview returned.
        .route("/documents/purge/preview", post(purge_preview_handler))
        .route("/documents/purge", post(purge_handler))
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

    let passwords = PdfPasswords::from_secrets(&state.secrets);
    let ingested = archive::ingest_one(
        &ingest_context(&state, &passwords),
        &body,
        filename,
        mime,
        source,
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

    read_and_file(
        &state,
        &q,
        &[DocumentPart::new(&body, mime)],
        (&body, mime, filename),
    )
    .await
    .map(Json)
}

/// `POST /documents/extract_pages?hint=…&attach=true` — several photos of one
/// document as multipart fields, in page order. Read in one model call and
/// archived as one PDF, a page per photo (`media::photos_to_pdf`).
async fn extract_pages_handler(
    State(state): State<AppState>,
    Query(q): Query<ExtractQuery>,
    mut multipart: Multipart,
) -> Result<Json<ExtractResponse>, (StatusCode, String)> {
    let bad = |e: String| (StatusCode::BAD_REQUEST, e);
    let mut pages: Vec<(Bytes, String)> = Vec::new();
    let mut first_name = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad(e.to_string()))?
    {
        if first_name.is_none() {
            first_name = field.file_name().map(str::to_string);
        }
        let mime = field.content_type().unwrap_or("image/jpeg").to_string();
        pages.push((field.bytes().await.map_err(|e| bad(e.to_string()))?, mime));
    }
    tracing::info!(pages = pages.len(), hint = ?q.hint, attach = q.attach, "extract_pages");

    // Built before the model call, so a bad page costs no reading.
    let owned: Vec<Bytes> = pages.iter().map(|(b, _)| b.clone()).collect();
    let pdf = tokio::task::spawn_blocking(move || {
        let photos: Vec<&[u8]> = owned.iter().map(|b| b.as_ref()).collect();
        omni_me_core::extraction::media::photos_to_pdf(&photos)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| bad(e.to_string()))?;

    let stem = first_name
        .as_deref()
        .and_then(|n| std::path::Path::new(n).file_stem()?.to_str())
        .unwrap_or("capture");
    let filename = format!("{stem}.pdf");
    let parts: Vec<DocumentPart<'_>> = pages.iter().map(|(b, m)| DocumentPart::new(b, m)).collect();
    read_and_file(&state, &q, &parts, (&pdf, "application/pdf", &filename))
        .await
        .map(Json)
}

/// Read `parts` with the model, file `filed` in the archive when `attach` is set,
/// and run the receipt cross-check. Shared by the one-file and many-photo routes.
async fn read_and_file(
    state: &AppState,
    q: &ExtractQuery,
    parts: &[DocumentPart<'_>],
    (bytes, mime, filename): (&[u8], &str, &str),
) -> Result<ExtractResponse, (StatusCode, String)> {
    let mut extraction = state
        .extractor
        .extract(parts, q.hint)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // Archived only after extraction succeeds, as the bare blob store was: a failed read is
    // retried by the device with the same bytes, and archiving first would file each retry.
    let attachment = if q.attach {
        Some(archive_capture(state, bytes, mime, filename, &extraction, q.hint).await?)
    } else {
        None
    };

    // Before the counter leg, which would cancel the line-item sum this compares to the total.
    // It used to run only in the extraction bench, never on a capture.
    let report = verify(&extraction, q.hint, DEFAULT_CONFIDENCE_THRESHOLD);

    // Drafts from the reading as the mail path builds them, before counter legs:
    // the Unmatched leg is what reconciliation pairs against the bank row.
    let document_id = attachment.as_ref().and_then(|a| a.document_id.clone());
    let proposed_batch_id = match document_id {
        Some(id) if q.propose && extraction.kind() == Some(DocumentKind::Receipt) => {
            let drafts = receipt_extraction_to_drafts(&extraction, &format!("capture-{id}"));
            let metadata = serde_json::json!({
                "document_id": id,
                "subject": filename,
                "document_kind": extraction.document_kind,
                "effective_confidence": report.effective_confidence,
                "needs_manual_review": report.needs_manual_review,
                "warnings": report.warnings,
                "total_check": report.total_check,
            });
            let event = to_proposed_event(
                "capture",
                format!("capture-{id}"),
                drafts,
                Some(metadata),
                state.device_id.clone(),
            );
            let batch_id = event.aggregate_id.clone();
            let appended = state
                .store
                .append_batch(vec![event])
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("append: {e}")))?;
            state.projections.apply_events_resilient(&appended).await;
            Some(batch_id)
        }
        _ => None,
    };

    extraction.confidence = report.effective_confidence;
    add_counter_legs(&mut extraction, q.hint);

    Ok(ExtractResponse {
        extraction,
        attachment,
        warnings: report.warnings,
        needs_review: report.needs_manual_review,
        total_check: report.total_check,
        proposed_batch_id,
    })
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
    let passwords = PdfPasswords::from_secrets(&state.secrets);
    let mut ingested = archive::ingest_one(
        &ingest_context(state, &passwords),
        body,
        filename,
        mime,
        archive::IngestSource::Scan,
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

/// A preview the user has been shown, and the only set a purge may act on.
///
/// ⛔ **The gate that keeps deletion out of a caller's hands.** `apply` accepts a
/// subset of the ids the matching preview returned and nothing else, so a client
/// — buggy, or driven from a console — cannot purge a group nobody looked at, and
/// cannot widen a set the user narrowed. It is `last_import_root`'s pattern,
/// moved server-side because this is where the bytes are.
///
/// ⚠️ Single-use and single-slot. A second preview replaces the first, so a stale
/// ticket cannot be replayed later against a group that has since changed.
#[derive(Debug, Clone)]
pub struct PurgeTicket {
    pub token: String,
    pub group: String,
    pub ids: std::collections::HashSet<String>,
}

#[derive(Debug, Deserialize)]
pub struct PurgePreviewRequest {
    /// The tag forming this group.
    pub tag: String,
    /// RFC3339. Narrows the group to documents archived before it, which is how a
    /// retention group is previewed — "tagged X and past X's rule" rather than the
    /// whole tag. Absent means the whole tag.
    #[serde(default)]
    pub archived_before: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PurgePreviewResponse {
    #[serde(flatten)]
    pub preview: purge::PurgePreview,
    /// Hand back with the confirm. See [`PurgeTicket`].
    pub token: String,
    /// How many of `total` are listed in `items`.
    pub listed: usize,
}

#[derive(Debug, Deserialize)]
pub struct PurgeRequest {
    pub token: String,
    /// The ids to purge — the previewed set, minus anything spared.
    pub document_ids: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// What a purge of this group would remove. Writes nothing.
async fn purge_preview_handler(
    State(state): State<AppState>,
    Json(body): Json<PurgePreviewRequest>,
) -> Result<Json<PurgePreviewResponse>, (StatusCode, String)> {
    let preview = purge::preview(&state.db, &body.tag, body.archived_before.as_deref())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let token = ulid::Ulid::new().to_string();
    let ticket = PurgeTicket {
        token: token.clone(),
        group: body.tag.clone(),
        ids: preview
            .items
            .iter()
            .map(|i| i.document_id.clone())
            .collect(),
    };

    // ⚠️ A group larger than one preview page cannot be confirmed in one action,
    // and that is deliberate rather than a limitation to route around: the ruling
    // is that every item going is *listed*, so a confirm covering rows the user
    // never saw would be the thing the grouped preview exists to avoid.
    let listed = preview.items.len();
    *state.purge_ticket.lock().await = Some(ticket);

    Ok(Json(PurgePreviewResponse {
        preview,
        token,
        listed,
    }))
}

/// Purge a confirmed set. ⛔ Irreversible.
async fn purge_handler(
    State(state): State<AppState>,
    Json(body): Json<PurgeRequest>,
) -> Result<Json<purge::PurgeReport>, (StatusCode, String)> {
    // Taken, not read: a ticket is spent by the confirm it authorises, so a
    // retry after a partial failure has to preview again and see current state.
    let ticket = state.purge_ticket.lock().await.take();
    let Some(ticket) = ticket.filter(|t| t.token == body.token) else {
        return Err((
            StatusCode::CONFLICT,
            "no matching preview — preview the group again before confirming".to_string(),
        ));
    };

    // ⛔ Subset only. Sparing items shrinks the set, which is the whole point of
    // the checkboxes; nothing may add to it.
    if let Some(stray) = body
        .document_ids
        .iter()
        .find(|id| !ticket.ids.contains(*id))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{stray} was not in the preview this token belongs to"),
        ));
    }

    let writer = EventWriter::new(
        state.store.clone(),
        state.projections.clone(),
        omni_me_core::config::ALL_FEATURES.iter().copied().collect(),
        state.device_id.clone(),
    );
    let reason = body.reason.clone().or_else(|| Some(ticket.group.clone()));

    // Its own task: a client that hangs up must not stop a purge between deleting
    // records and reclaiming their blobs. Same reason as the feature wipe.
    let ids = body.document_ids;
    let report = tokio::spawn(async move {
        purge::apply(&state.db, &writer, &state.blob_dir, &ids, reason.as_deref()).await
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("purge task: {e}"),
        )
    })?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(report))
}
