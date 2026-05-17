//! Tauri command for document extraction (Phase 3.1+).
//!
//! Forwards captured document bytes to `omni-me-server`'s
//! `/documents/extract` endpoint, which runs `GeminiExtractor`. Server-side
//! host honors `feedback_llm_server_side.md` — Gemini API keys never live
//! on the client.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

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
        "{}/documents/extract?hint={hint}",
        server_url.trim_end_matches('/'),
    );
    tracing::info!(bytes = bytes.len(), mime = %mime, hint = %hint, %url, "extract_document");

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

    resp.json::<ExtractedDraft>()
        .await
        .map_err(|e| format!("parse response: {e}"))
}
