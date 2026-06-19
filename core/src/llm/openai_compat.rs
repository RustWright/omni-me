//! OpenAI-compatible chat-completions client — the "provider-swap" half of the
//! extensibility mechanism (3.8 bring-your-own-LLM).
//!
//! One generic HTTP client + config (base URL / model / key) covers Ollama,
//! llama.cpp's server, vLLM, LM Studio, and the OpenAI API itself, because they
//! all expose the same `/chat/completions` surface. Selecting it is a config
//! choice (`[llm] provider = "openai_compatible"` in `credentials.toml`); the
//! existing `GeminiClient` stays the default.
//!
//! Scope: this is the *text* side of `LlmClient` (note processing + structured
//! text). The multimodal `DocumentExtractor` swap rides the same `[llm]` config
//! but is a deferred fast-follow — many OpenAI-compatible endpoints have no
//! vision support, so it needs its own graceful-degradation handling.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::client::{LlmClient, LlmError};
use super::tools::{LlmResponse, ToolCall, ToolDef};

/// Client for any OpenAI-compatible chat-completions endpoint.
pub struct OpenAiCompatClient {
    api_key: String,
    model: String,
    base_url: String,
    http: reqwest::Client,
}

impl OpenAiCompatClient {
    /// `base_url` is the API root (e.g. `http://localhost:11434/v1` for Ollama,
    /// `https://api.openai.com/v1`); the client appends `/chat/completions`.
    /// `api_key` may be empty for local servers that don't check it (the bearer
    /// header is then omitted).
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

    /// POST a chat-completions body and return the parsed JSON response. Errors
    /// are scrubbed of the URL (and thus any key in it) via `without_url`,
    /// mirroring `GeminiClient` — a leaked key in a log line is the failure mode
    /// guarded against.
    async fn send(&self, body: Value) -> Result<Value, LlmError> {
        let mut req = self.http.post(self.endpoint()).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let response = req
            .send()
            .await
            .map_err(|e| LlmError::NetworkError(e.without_url().to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited);
        }

        let response_body: Value = response.json().await.map_err(|e| {
            LlmError::ParseError(format!("parse response JSON: {}", e.without_url()))
        })?;

        if !status.is_success() {
            let msg = response_body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown API error");
            return Err(LlmError::ApiError(format!("HTTP {status}: {msg}")));
        }
        Ok(response_body)
    }

    /// Pull `choices[0].message.content` text out of a chat-completions response.
    fn extract_content(response: &Value) -> Result<String, LlmError> {
        response["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| LlmError::ParseError("no message content in response".to_string()))
    }

    /// Convert `ToolDef`s to the OpenAI `tools` array (function-calling shape).
    fn tool_defs_to_openai(tools: &[ToolDef]) -> Value {
        let defs: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        Value::Array(defs)
    }

    /// Parse `choices[0].message.tool_calls[]` into `ToolCall`s. OpenAI encodes
    /// each call's `arguments` as a JSON *string*, so we parse it (falling back
    /// to the raw value if some endpoint sends an object instead).
    fn parse_tool_calls(response: &Value) -> Option<Vec<ToolCall>> {
        let raw = response["choices"][0]["message"]["tool_calls"].as_array()?;
        let calls: Vec<ToolCall> = raw
            .iter()
            .filter_map(|tc| {
                let f = tc.get("function")?;
                let name = f["name"].as_str()?.to_string();
                let arguments = f["arguments"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .unwrap_or_else(|| f["arguments"].clone());
                Some(ToolCall { name, arguments })
            })
            .collect();
        if calls.is_empty() {
            None
        } else {
            Some(calls)
        }
    }
}

#[async_trait]
impl LlmClient for OpenAiCompatClient {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let response = self.send(body).await?;
        Self::extract_content(&response)
    }

    async fn complete_json(&self, prompt: &str, schema: &Value) -> Result<Value, LlmError> {
        // Portable structured output: request `json_object` (widely supported by
        // local servers, unlike the newer `json_schema` form) and steer the shape
        // via the prompt by embedding the JSON Schema. Trades strict server-side
        // validation for broad compatibility — the right call for "any endpoint".
        let steered = format!(
            "{prompt}\n\nRespond with a single JSON object conforming to this JSON \
             Schema. Output JSON only, no prose or code fences:\n{schema}",
            schema = serde_json::to_string(schema).unwrap_or_default()
        );
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": steered }],
            "response_format": { "type": "json_object" },
        });
        let response = self.send(body).await?;
        let text = Self::extract_content(&response)?;
        serde_json::from_str(&text)
            .map_err(|e| LlmError::ParseError(format!("parse JSON response: {e}")))
    }

    async fn complete_with_tools(
        &self,
        prompt: &str,
        tools: &[ToolDef],
    ) -> Result<LlmResponse, LlmError> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }],
            "tools": Self::tool_defs_to_openai(tools),
        });
        let response = self.send(body).await?;
        if let Some(calls) = Self::parse_tool_calls(&response) {
            return Ok(LlmResponse::ToolCalls(calls));
        }
        // No tool calls (endpoint ignored `tools`, or chose to answer in prose).
        let text = Self::extract_content(&response)?;
        Ok(LlmResponse::Text(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(server: &MockServer) -> OpenAiCompatClient {
        OpenAiCompatClient::new(server.uri(), "test-model", "test-key")
    }

    fn chat_text_response(content: &str) -> Value {
        json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] })
    }

    #[tokio::test]
    async fn complete_returns_message_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_text_response("Hello world")))
            .mount(&server)
            .await;

        let text = test_client(&server).complete("hi").await.unwrap();
        assert_eq!(text, "Hello world");
    }

    #[tokio::test]
    async fn complete_json_parses_object_content() {
        let server = MockServer::start().await;
        let content = r#"{"category": "productivity", "tags": ["work"]}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_text_response(content)))
            .mount(&server)
            .await;

        let parsed = test_client(&server)
            .complete_json("extract", &json!({"type": "object"}))
            .await
            .unwrap();
        assert_eq!(parsed["category"], "productivity");
        assert_eq!(parsed["tags"][0], "work");
    }

    #[tokio::test]
    async fn complete_with_tools_parses_openai_tool_calls() {
        let server = MockServer::start().await;
        // OpenAI encodes `arguments` as a JSON *string* — exercise that path.
        let resp = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "create_tag", "arguments": "{\"tag\": \"personal\"}" }
                    }]
                }
            }]
        });
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .mount(&server)
            .await;

        let out = test_client(&server)
            .complete_with_tools("process", super::super::tools::default_note_tools())
            .await
            .unwrap();
        match out {
            LlmResponse::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "create_tag");
                assert_eq!(calls[0].arguments["tag"], "personal");
            }
            other => panic!("expected tool calls, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn complete_with_tools_falls_back_to_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_text_response("just prose")))
            .mount(&server)
            .await;

        let out = test_client(&server)
            .complete_with_tools("process", super::super::tools::default_note_tools())
            .await
            .unwrap();
        assert!(matches!(out, LlmResponse::Text(t) if t == "just prose"));
    }

    #[tokio::test]
    async fn api_error_surfaces_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "error": { "message": "bad model" } })),
            )
            .mount(&server)
            .await;

        let err = test_client(&server).complete("x").await.unwrap_err();
        match err {
            LlmError::ApiError(msg) => assert!(msg.contains("bad model")),
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn too_many_requests_maps_to_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({})))
            .mount(&server)
            .await;

        assert!(matches!(
            test_client(&server).complete("x").await.unwrap_err(),
            LlmError::RateLimited
        ));
    }

    #[tokio::test]
    async fn network_error_does_not_leak_api_key() {
        let secret = "super-secret-openai-key-xyz";
        let client = OpenAiCompatClient::new("http://127.0.0.1:1", "m", secret); // unreachable
        let err = client.complete("x").await.unwrap_err();
        assert!(
            !err.to_string().contains(secret),
            "api key leaked in error: {err}"
        );
    }
}
