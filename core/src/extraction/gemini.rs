//! Gemini multimodal `DocumentExtractor` implementation (Phase 2.4).
//!
//! Cycle 3's sole extractor — Veryfi is deferred to Cycle 4. POC 0.3
//! (2026-05-09) validated `gemini-2.5-flash` on a 5-page brokerage
//! statement: clean structured extraction in ~7s, balance + dates correct.
//!
//! Per-hint prompts live in this file (small enough not to warrant a
//! `prompts` submodule yet). All hints target the same `ExtractionResult`
//! JSON shape via a single response schema — schema-uniformity makes the
//! parse path trivial and lets the verification pass (2.6) work uniformly.

use async_trait::async_trait;
use std::sync::Arc;

use crate::llm::{GeminiClient, LlmClient};

use super::{
    parse_response, prompt_for, response_schema, DocumentExtractor, ExtractionError,
    ExtractionHint, ExtractionResult,
};

/// Model id pinned to the POC-validated version. Override via `with_model` if
/// you need to test a different revision; keep production on this until a new
/// POC certifies a swap.
const EXTRACTOR_MODEL: &str = "gemini-2.5-flash";

pub struct GeminiExtractor {
    client: Arc<GeminiClient>,
    model: String,
}

impl GeminiExtractor {
    /// Build with a default Gemini client pinned to the extractor model.
    pub fn new(api_key: String) -> Self {
        let client = GeminiClient::new(api_key).with_model(EXTRACTOR_MODEL.to_string());
        Self {
            client: Arc::new(client),
            model: EXTRACTOR_MODEL.to_string(),
        }
    }

    /// Build from an existing client (used in tests with a wiremock-backed
    /// client + by callers that want to share a single Gemini connection
    /// pool across multiple extractors).
    pub fn with_client(client: Arc<GeminiClient>) -> Self {
        let model = client.model_name().to_string();
        Self { client, model }
    }
}

#[async_trait]
impl DocumentExtractor for GeminiExtractor {
    fn name(&self) -> &str {
        &self.model
    }

    fn supports(&self, mime: &str) -> bool {
        matches!(
            mime,
            "application/pdf"
                | "image/jpeg"
                | "image/png"
                | "image/webp"
                | "image/heic"
                | "image/heif"
                | "text/plain"
                | "text/html"
        )
    }

    async fn extract(
        &self,
        bytes: &[u8],
        mime: &str,
        hint: ExtractionHint,
    ) -> Result<ExtractionResult, ExtractionError> {
        if !self.supports(mime) {
            return Err(ExtractionError::UnsupportedMime {
                extractor: self.model.clone(),
                mime: mime.to_string(),
            });
        }

        let prompt = prompt_for(hint);
        let schema = response_schema();

        let raw = self
            .client
            .complete_multimodal_json(&prompt, bytes, mime, &schema)
            .await
            .map_err(|e| ExtractionError::Upstream(e.to_string()))?;

        parse_response(raw, &self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use serde_json::json;
    use std::str::FromStr;

    #[test]
    fn supports_known_mime_types() {
        let ext = GeminiExtractor::new("test-key".to_string());
        assert!(ext.supports("application/pdf"));
        assert!(ext.supports("image/jpeg"));
        assert!(ext.supports("image/png"));
        assert!(ext.supports("text/plain"));
        assert!(!ext.supports("application/octet-stream"));
        assert!(!ext.supports("video/mp4"));
    }

    #[test]
    fn prompts_differ_per_hint() {
        // Sanity: each hint produces a substantively different prompt.
        let p1 = prompt_for(ExtractionHint::Receipt);
        let p2 = prompt_for(ExtractionHint::BankStatement);
        let p3 = prompt_for(ExtractionHint::Paystub);
        assert_ne!(p1, p2);
        assert_ne!(p2, p3);
        assert!(p1.contains("receipt"));
        assert!(p2.contains("bank statement"));
        assert!(p3.contains("paystub"));
    }

    #[test]
    fn parse_response_clamps_invalid_confidence() {
        let raw = json!({
            "date": "2026-05-16",
            "description": "Coffee",
            "postings": [
                { "commodity": "CAD", "amount": "5.25" }
            ],
            "confidence": 1.5
        });
        let result = parse_response(raw, "test-model").unwrap();
        assert_eq!(result.confidence, 1.0);

        let raw2 = json!({
            "postings": [],
            "confidence": -0.3
        });
        let result2 = parse_response(raw2, "test-model").unwrap();
        assert_eq!(result2.confidence, 0.0);
    }

    #[test]
    fn parse_response_decimal_string_preserved() {
        let raw = json!({
            "postings": [
                { "commodity": "CAD", "amount": "1234.56" },
                { "commodity": "USD", "amount": "-10.00" }
            ],
            "confidence": 0.9
        });
        let result = parse_response(raw, "test-model").unwrap();
        assert_eq!(result.postings.len(), 2);
        assert_eq!(result.postings[0].amount, Decimal::from_str("1234.56").unwrap());
        assert_eq!(result.postings[1].amount, Decimal::from_str("-10.00").unwrap());
    }

    #[test]
    fn parse_response_rejects_numeric_amount() {
        // Guard against Gemini ignoring the schema's `string` constraint —
        // if it slips through, we want a clear parse error, not silent f64.
        let raw = json!({
            "postings": [
                { "commodity": "CAD", "amount": 5.25 }
            ],
            "confidence": 0.9
        });
        assert!(parse_response(raw, "test-model").is_err());
    }

    #[test]
    fn parse_response_optional_fields() {
        let raw = json!({
            "postings": [
                { "commodity": "CAD", "amount": "5.25" }
            ],
            "confidence": 0.5
        });
        let result = parse_response(raw, "test-model").unwrap();
        assert!(result.date.is_none());
        assert!(result.description.is_none());
        assert!(result.postings[0].account_hint.is_none());
        assert!(result.postings[0].line_label.is_none());
    }

    #[tokio::test]
    async fn extract_rejects_unsupported_mime() {
        let ext = GeminiExtractor::new("test-key".to_string());
        let err = ext
            .extract(b"x", "video/mp4", ExtractionHint::Receipt)
            .await
            .unwrap_err();
        match err {
            ExtractionError::UnsupportedMime { mime, .. } => assert_eq!(mime, "video/mp4"),
            other => panic!("expected UnsupportedMime, got {other:?}"),
        }
    }
}
