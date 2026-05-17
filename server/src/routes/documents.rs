use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use omni_me_core::events::AttachmentRef;
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
        .layer(DefaultBodyLimit::max(MAX_DOCUMENT_BYTES))
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

/// Mirrors `routes::blobs::put_blob_handler` storage semantics: hash the
/// bytes, atomic temp + rename, idempotent if the hash already exists. Kept
/// inline here (not extracted to a shared helper) because the only other
/// caller is the PUT route — duplication is two short functions, not a
/// pattern worth abstracting yet.
async fn store_blob(
    state: &AppState,
    body: &[u8],
    mime: &str,
    filename: &str,
) -> Result<AttachmentRef, String> {
    let hash = Sha256::digest(body)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    let final_path = state.blob_dir.join(&hash);
    if !tokio::fs::try_exists(&final_path)
        .await
        .map_err(|e| format!("blob exists check: {e}"))?
    {
        let tmp = state.blob_dir.join(format!(".tmp-{}", ulid::Ulid::new()));
        tokio::fs::write(&tmp, body)
            .await
            .map_err(|e| format!("blob temp write: {e}"))?;
        tokio::fs::rename(&tmp, &final_path)
            .await
            .map_err(|e| format!("blob rename: {e}"))?;
    }

    Ok(AttachmentRef {
        sha256: hash,
        filename: filename.to_string(),
        mime_type: mime.to_string(),
        size: body.len() as u64,
    })
}
