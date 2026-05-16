//! Null extractor — returned when no real provider is configured, and used by
//! tests that want a deterministic no-op result without spinning up Gemini.
//!
//! Always returns a zero-confidence empty draft. The Phase 3.6 confirm-draft
//! screen treats this as "no extraction available, manual entry only."

use async_trait::async_trait;

use super::{DocumentExtractor, ExtractionError, ExtractionHint, ExtractionResult};

pub struct NullExtractor;

#[async_trait]
impl DocumentExtractor for NullExtractor {
    fn name(&self) -> &str {
        "null"
    }

    fn supports(&self, _mime: &str) -> bool {
        true
    }

    async fn extract(
        &self,
        _bytes: &[u8],
        _mime: &str,
        _hint: ExtractionHint,
    ) -> Result<ExtractionResult, ExtractionError> {
        Ok(ExtractionResult {
            date: None,
            description: None,
            postings: vec![],
            confidence: 0.0,
            model: "null".to_string(),
            raw_response: serde_json::Value::Null,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_extractor_returns_empty_zero_confidence_draft() {
        let ext = NullExtractor;
        let result = ext
            .extract(b"any bytes", "image/jpeg", ExtractionHint::Receipt)
            .await
            .unwrap();
        assert_eq!(result.confidence, 0.0);
        assert!(result.postings.is_empty());
        assert!(result.date.is_none());
        assert_eq!(result.model, "null");
    }

    #[tokio::test]
    async fn null_extractor_supports_any_mime() {
        let ext = NullExtractor;
        assert!(ext.supports("application/pdf"));
        assert!(ext.supports("image/jpeg"));
        assert!(ext.supports("text/plain"));
        assert!(ext.supports("anything"));
    }
}
