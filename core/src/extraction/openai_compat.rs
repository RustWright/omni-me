//! OpenAI-compatible vision `DocumentExtractor` (3.8a) — the multimodal half of
//! the bring-your-own-LLM provider-swap.
//!
//! The text-side `LlmClient` swap (3.8) deliberately kept multimodal *off* the
//! trait, so receipts/statements get their own extractor here. It POSTs the
//! OpenAI vision shape (`content: [{type:"text"}, {type:"image_url"}]`) to the
//! same `{base_url}/chat/completions` surface the text client uses, reusing the
//! shared per-hint prompts + response schema + parse from the `extraction`
//! module so its output is identical to Gemini's.
//!
//! It rides the same `[llm]` config (base_url / model / key) but is **opt-in**
//! via `[llm] vision = true` (see `server::build_extractor`): vision support
//! varies across OpenAI-compatible endpoints, so we never silently send images
//! to one that can't handle them. `supports()` reports images + text only —
//! raw PDF is excluded (unlike Gemini, most of these endpoints reject it).

use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};

use super::{
    parse_response, prompt_for, response_schema, DocumentExtractor, ExtractionError,
    ExtractionHint, ExtractionResult,
};

/// Vision extractor for any OpenAI-compatible chat-completions endpoint.
pub struct OpenAiCompatExtractor {
    api_key: String,
    model: String,
    base_url: String,
    http: reqwest::Client,
}

impl OpenAiCompatExtractor {
    /// `base_url` is the API root (e.g. `http://localhost:11434/v1`); the client
    /// appends `/chat/completions`. `api_key` may be empty for local servers.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Build the user message `content` array. Images go in as a base64 data
    /// URI; text documents are inlined as a second text block (the endpoint
    /// can't "see" a text/plain attachment otherwise).
    fn content_for(prompt: String, bytes: &[u8], mime: &str) -> Value {
        if mime.starts_with("image/") {
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            json!([
                { "type": "text", "text": prompt },
                { "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{b64}") } },
            ])
        } else {
            // text/plain or text/html — inline the decoded body.
            let body = String::from_utf8_lossy(bytes);
            json!([
                { "type": "text", "text": format!("{prompt}\n\n--- DOCUMENT ---\n{body}") },
            ])
        }
    }

    /// Pull `choices[0].message.content`, tolerating a code-fenced block (some
    /// endpoints wrap JSON in ```` ```json ```` despite `response_format`).
    fn content_json(response: &Value) -> Result<Value, ExtractionError> {
        let text = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| ExtractionError::Parse("no message content in response".into()))?;
        let cleaned = strip_code_fences(text);
        serde_json::from_str(cleaned)
            .map_err(|e| ExtractionError::Parse(format!("parse JSON content: {e}")))
    }
}

/// Strip a leading ```` ```lang ```` fence and trailing ```` ``` ```` if present,
/// else return the input unchanged.
fn strip_code_fences(text: &str) -> &str {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t;
    };
    // Drop the optional language tag up to the first newline.
    let rest = rest.split_once('\n').map_or(rest, |(_, body)| body);
    rest.trim().strip_suffix("```").unwrap_or(rest).trim()
}

#[async_trait]
impl DocumentExtractor for OpenAiCompatExtractor {
    fn name(&self) -> &str {
        &self.model
    }

    fn supports(&self, mime: &str) -> bool {
        // Images + text only. PDF is excluded — most OpenAI-compatible vision
        // endpoints reject raw PDF (unlike Gemini), so claiming support would
        // produce confusing upstream errors instead of a clean fall-through.
        matches!(
            mime,
            "image/jpeg" | "image/png" | "image/webp" | "text/plain" | "text/html"
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

        // Steer the JSON shape via the prompt (the portable path — `json_object`
        // is far more widely supported than server-side `json_schema`), same as
        // the text client's `complete_json`.
        let schema = response_schema();
        let prompt = format!(
            "{}\n\nRespond with a single JSON object conforming to this JSON Schema. \
             Output JSON only, no prose or code fences:\n{}",
            prompt_for(hint),
            serde_json::to_string(&schema).unwrap_or_default()
        );

        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": Self::content_for(prompt, bytes, mime) }],
            "response_format": { "type": "json_object" },
        });

        let mut req = self.http.post(self.endpoint()).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        // `without_url` scrubs any key in the URL from error strings (mirrors the
        // text client) — a leaked key in a log line is the failure mode guarded.
        let response = req
            .send()
            .await
            .map_err(|e| ExtractionError::Upstream(e.without_url().to_string()))?;

        let status = response.status();
        let response_body: Value = response
            .json()
            .await
            .map_err(|e| ExtractionError::Parse(format!("parse response JSON: {}", e.without_url())))?;

        if !status.is_success() {
            let msg = response_body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown API error");
            return Err(ExtractionError::Upstream(format!("HTTP {status}: {msg}")));
        }

        let raw = Self::content_json(&response_body)?;
        parse_response(raw, &self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn extraction_response(content: &str) -> Value {
        json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] })
    }

    fn one_png() -> &'static [u8] {
        // Not a real PNG — bytes are opaque to the extractor (it just base64s
        // them); the mock doesn't inspect the image.
        b"\x89PNG\r\n\x1a\nfake"
    }

    #[tokio::test]
    async fn extract_posts_vision_shape_and_parses_result() {
        let server = MockServer::start().await;
        let content = r#"{"date":"2026-05-16","description":"Coffee",
            "postings":[{"account_hint":"Expenses:Coffee","commodity":"CAD","amount":"5.25"}],
            "confidence":0.9}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k");
        let result = ext
            .extract(one_png(), "image/png", ExtractionHint::Receipt)
            .await
            .unwrap();
        assert_eq!(result.description.as_deref(), Some("Coffee"));
        assert_eq!(result.postings.len(), 1);
        assert_eq!(result.postings[0].account_hint.as_deref(), Some("Expenses:Coffee"));
        assert_eq!(result.confidence, 0.9);
        assert_eq!(result.model, "llava");
    }

    #[tokio::test]
    async fn extract_strips_code_fences() {
        let server = MockServer::start().await;
        let fenced = "```json\n{\"postings\":[{\"commodity\":\"CAD\",\"amount\":\"1.00\"}],\"confidence\":0.5}\n```";
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(fenced)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "m", "");
        let result = ext
            .extract(one_png(), "image/png", ExtractionHint::Receipt)
            .await
            .unwrap();
        assert_eq!(result.postings.len(), 1);
    }

    #[tokio::test]
    async fn unsupported_pdf_mime_rejected_without_call() {
        let ext = OpenAiCompatExtractor::new("http://127.0.0.1:1", "m", "");
        let err = ext
            .extract(b"%PDF-1.7", "application/pdf", ExtractionHint::BankStatement)
            .await
            .unwrap_err();
        assert!(matches!(err, ExtractionError::UnsupportedMime { .. }));
    }

    #[tokio::test]
    async fn api_error_surfaces_as_upstream() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({ "error": { "message": "no vision" } })),
            )
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "m", "k");
        let err = ext
            .extract(one_png(), "image/png", ExtractionHint::Receipt)
            .await
            .unwrap_err();
        match err {
            ExtractionError::Upstream(msg) => assert!(msg.contains("no vision")),
            other => panic!("expected Upstream, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn network_error_does_not_leak_api_key() {
        let secret = "super-secret-vision-key-xyz";
        let ext = OpenAiCompatExtractor::new("http://127.0.0.1:1", "m", secret); // unreachable
        let err = ext
            .extract(one_png(), "image/png", ExtractionHint::Receipt)
            .await
            .unwrap_err();
        assert!(
            !err.to_string().contains(secret),
            "api key leaked in error: {err}"
        );
    }

    #[test]
    fn supports_images_and_text_not_pdf() {
        let ext = OpenAiCompatExtractor::new("u", "m", "");
        assert!(ext.supports("image/png"));
        assert!(ext.supports("image/jpeg"));
        assert!(ext.supports("text/plain"));
        assert!(!ext.supports("application/pdf"));
        assert!(!ext.supports("image/heic"));
    }
}
