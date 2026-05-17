//! Tauri command for document extraction (Phase 3.1+).
//!
//! Forwards captured document bytes to `omni-me-server`'s
//! `/documents/extract` endpoint, which runs `GeminiExtractor`. Server-side
//! host honors `feedback_llm_server_side.md` — Gemini API keys never live
//! on the client.
//!
//! Phase 3.7 wiring: every call sets `attach=true` so the server stores the
//! bytes content-addressably as well as extracting from them. The
//! `AttachmentRef` round-trips back so the client can mirror the bytes into
//! the local LRU cache (see `commands::attachments`) and surface the ref to
//! the confirm-draft form for inclusion on the `TransactionRecorded` event.

use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::events::AttachmentRef;

use crate::AppState;
use crate::commands::attachments;

/// Single extracted posting line. Amount is wire-side string (server's
/// `rust_decimal::serde::str`); frontend mirror lives in
/// `tauri-app/frontend/src/types.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedPostingView {
    #[serde(default)]
    pub account_hint: Option<String>,
    pub commodity: String,
    pub amount: String,
    #[serde(default)]
    pub line_label: Option<String>,
}

/// Mirror of `core::extraction::ExtractionResult` minus `raw_response` —
/// frontend doesn't need the raw LLM JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedDraft {
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub postings: Vec<ExtractedPostingView>,
    #[serde(default)]
    pub total: Option<String>,
    pub confidence: f64,
    #[serde(default)]
    pub model: String,
    /// Populated when the capture flow sets `attach=true` on the server call.
    /// Threaded through `TransactionForm` so the confirm-draft Save bundles
    /// this onto the `TransactionRecorded` event.
    #[serde(default)]
    pub attachment: Option<AttachmentRef>,
}

/// Wire shape returned by `/documents/extract` when `attach=true`. Mirrors
/// `omni_me_server::routes::documents::ExtractResponse`.
#[derive(Debug, Clone, Deserialize)]
struct ExtractResponseWire {
    extraction: ExtractionWire,
    attachment: Option<AttachmentRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct ExtractionWire {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    description: Option<String>,
    postings: Vec<ExtractedPostingView>,
    #[serde(default)]
    total: Option<String>,
    confidence: f64,
    #[serde(default)]
    model: String,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn extract_document(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
    mime: String,
    hint: String,
) -> Result<ExtractedDraft, String> {
    let server_url = state.server_url.read().await.clone();
    // Hint values are simple snake_case strings (receipt, bank_statement, ...)
    // so no URL encoding is needed; the server will 400 on anything unknown.
    let url = format!(
        "{}/documents/extract?hint={hint}&attach=true",
        server_url.trim_end_matches('/'),
    );
    tracing::info!(bytes = bytes.len(), mime = %mime, hint = %hint, %url, "extract_document");

    let body_for_cache = bytes.clone();

    let resp = state
        .http
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, &mime)
        .body(bytes)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("server returned {status}: {body}"));
    }

    let wire: ExtractResponseWire = resp
        .json()
        .await
        .map_err(|e| format!("parse response: {e}"))?;

    // Mirror bytes into the local LRU cache so the Phase 4 transaction detail
    // view can render the receipt offline. Failure is non-fatal — extraction
    // itself succeeded, and `fetch_attachment` will re-fetch from `/blobs` on
    // miss. We log + continue rather than masking the extraction result.
    if let Some(att) = &wire.attachment
        && let Err(e) =
            attachments::cache_write(&state.attachment_cache_dir, &att.sha256, &body_for_cache)
                .await
    {
        tracing::warn!(error = %e, sha256 = %att.sha256, "attachment cache write failed");
    }

    Ok(ExtractedDraft {
        date: wire.extraction.date,
        description: wire.extraction.description,
        postings: wire.extraction.postings,
        total: wire.extraction.total,
        confidence: wire.extraction.confidence,
        model: wire.extraction.model,
        attachment: wire.attachment,
    })
}
