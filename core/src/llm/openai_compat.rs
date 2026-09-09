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

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use super::chat::{ChatRequest, ChatResponse, Usage};
use super::client::{LlmClient, LlmError};
use super::tools::{LlmResponse, ToolCall, ToolDef};

/// How many times a 429 is retried before the call is given up on.
///
/// ⚠️ A 429 is not necessarily about *our* request rate, and treating it as one
/// misreads the most common case. Pinning a gateway to a single upstream — which
/// any measurement must do, or it cannot name the stack that answered — means a
/// transient overload in that provider's shared pool comes back to us instead of
/// being rerouted, arriving as `limit_source: upstream_provider_shared_pool` with
/// the advice to retry shortly. Failing the call on the first one scores
/// congestion as a model failure, which is the same class of mistake as having no
/// request spacing at all: the number ends up describing the harness.
///
/// This is therefore the *third* rate-limit control here and they are not
/// redundant — [`OpenAiCompatClient::min_interval`] stops us causing a 429,
/// this survives one we did not cause.
const MAX_RATE_LIMIT_RETRIES: u32 = 3;

/// Client for any OpenAI-compatible chat-completions endpoint.
pub struct OpenAiCompatClient {
    api_key: String,
    model: String,
    base_url: String,
    http: reqwest::Client,
    /// Provider-specific fields merged into every request body.
    ///
    /// Exists for gateways that route to an upstream of their choosing:
    /// OpenRouter needs `{"provider": {"order": [..], "allow_fallbacks": false}}`
    /// or a silent reroute makes a measurement unattributable — you learn that
    /// *something* answered, not whose stack did.
    extra_body: Option<Value>,
    /// Minimum gap between requests, when the endpoint caps request rate.
    ///
    /// `None` by default because "OpenAI-compatible" covers a local Ollama with
    /// no limit at all as well as a free tier allowing ten requests a minute —
    /// a fixed interval would either throttle the first or be useless for the
    /// second. Where a cap exists this is a **correctness** control, not a
    /// politeness one: an agent loop makes several calls per question and a
    /// benchmark makes dozens, so without it the run reports rate-limit errors
    /// as model failures.
    min_interval: Option<Duration>,
    /// First wait after a 429, doubling per retry. See [`MAX_RATE_LIMIT_RETRIES`].
    retry_backoff: Duration,
    last_request: Arc<Mutex<Instant>>,
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
            http: crate::http::llm_client(),
            extra_body: None,
            min_interval: None,
            retry_backoff: Duration::from_secs(2),
            // Far enough back that the first request never waits.
            last_request: Arc::new(Mutex::new(
                Instant::now()
                    .checked_sub(Duration::from_secs(3600))
                    .unwrap_or_else(Instant::now),
            )),
        }
    }

    /// Merge provider-specific fields into every request. See [`Self::extra_body`].
    pub fn with_extra_body(mut self, extra: Value) -> Self {
        self.extra_body = Some(extra);
        self
    }

    /// Space requests at least this far apart. See [`Self::min_interval`].
    pub fn with_min_interval(mut self, interval: Duration) -> Self {
        self.min_interval = Some(interval);
        self
    }

    /// First wait after a 429; each further retry doubles it.
    ///
    /// Two seconds by default, which suits a congested shared pool. A local
    /// server that cannot rate limit at all wants it short, and so do tests —
    /// the default would otherwise put fourteen seconds of real sleeping into
    /// the suite.
    pub fn with_retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff = backoff;
        self
    }

    /// Sleep if the last request was too recent.
    async fn rate_limit(&self) {
        let Some(min) = self.min_interval else {
            return;
        };
        // The guard is held across the sleep on purpose: it serialises callers,
        // which is what makes the interval hold when an agent loop has several
        // requests in flight. Releasing it first would let them all through.
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < min {
            tokio::time::sleep(min - elapsed).await;
        }
        *last = Instant::now();
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Fold [`Self::extra_body`] into a request body, top-level keys only.
    fn apply_extra(&self, mut body: Value) -> Value {
        if let (Some(Value::Object(extra)), Some(target)) = (&self.extra_body, body.as_object_mut())
        {
            for (k, v) in extra {
                target.insert(k.clone(), v.clone());
            }
        }
        body
    }

    /// POST a chat-completions body and return the parsed JSON response. Errors
    /// are scrubbed of the URL (and thus any key in it) via `without_url`,
    /// mirroring `GeminiClient` — a leaked key in a log line is the failure mode
    /// guarded against.
    async fn send(&self, body: Value) -> Result<Value, LlmError> {
        let body = self.apply_extra(body);
        let mut attempt = 0u32;

        loop {
            self.rate_limit().await;
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
                if attempt >= MAX_RATE_LIMIT_RETRIES {
                    return Err(LlmError::RateLimited);
                }
                // Honour `Retry-After` where the endpoint sends one, capped so a
                // hostile or mistaken value cannot park the run for an hour.
                let wait = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or(self.retry_backoff * 2u32.pow(attempt))
                    .min(Duration::from_secs(60));
                attempt += 1;
                // Loud, and counted: a scorecard produced under repeated
                // congestion is worth reading differently from a clean one.
                tracing::warn!(
                    attempt,
                    wait_ms = wait.as_millis() as u64,
                    "rate limited; backing off and retrying"
                );
                tokio::time::sleep(wait).await;
                continue;
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
            return Ok(response_body);
        }
    }

    /// Pull `choices[0].message.content` text out of a chat-completions response.
    fn extract_content(response: &Value) -> Result<String, LlmError> {
        response["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| LlmError::ParseError("no message content in response".to_string()))
    }

    /// Parse `choices[0].message.tool_calls[]` into `ToolCall`s. OpenAI encodes
    /// each call's `arguments` as a JSON *string*, so we parse it (falling back
    /// to the raw value if some endpoint sends an object instead).
    fn parse_tool_calls(response: &Value) -> Option<Vec<ToolCall>> {
        let calls = parse_tool_calls_vec(response);
        if calls.is_empty() { None } else { Some(calls) }
    }
}

/// Convert `ToolDef`s to the OpenAI `tools` array (function-calling shape).
///
/// Free function rather than an inherent one so [`super::chat::ChatRequest`] can
/// build a body without owning a client.
pub(super) fn tool_defs_to_openai(tools: &[ToolDef]) -> Value {
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

/// Shared by the single-shot and multi-turn paths.
///
/// The `id` is carried through because a multi-turn conversation answers a call
/// by referencing it; the single-shot path ignores it.
fn parse_tool_calls_vec(response: &Value) -> Vec<ToolCall> {
    let Some(raw) = response["choices"][0]["message"]["tool_calls"].as_array() else {
        return Vec::new();
    };
    raw.iter()
        .filter_map(|tc| {
            let f = tc.get("function")?;
            let name = f["name"].as_str()?.to_string();
            let arguments = f["arguments"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .unwrap_or_else(|| f["arguments"].clone());
            Some(ToolCall {
                id: tc["id"].as_str().unwrap_or_default().to_string(),
                name,
                arguments,
            })
        })
        .collect()
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
            "tools": tool_defs_to_openai(tools),
        });
        let response = self.send(body).await?;
        if let Some(calls) = Self::parse_tool_calls(&response) {
            return Ok(LlmResponse::ToolCalls(calls));
        }
        // No tool calls (endpoint ignored `tools`, or chose to answer in prose).
        let text = Self::extract_content(&response)?;
        Ok(LlmResponse::Text(text))
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let started = std::time::Instant::now();
        let response = self.send(request.to_wire(&self.model)).await?;
        let latency = started.elapsed();

        // `content` is absent on a pure tool-call turn, which is not an error —
        // unlike `extract_content`, which treats it as one.
        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let out = ChatResponse {
            content,
            tool_calls: parse_tool_calls_vec(&response),
            usage: Usage::from_response(&response),
            latency,
            finish_reason: response["choices"][0]["finish_reason"]
                .as_str()
                .map(|s| s.to_string()),
            // Top level, not inside `choices` — OpenRouter reports the upstream
            // it routed to here. Absent on every plain OpenAI-compatible server,
            // which is why it is an `Option` rather than a failure.
            provider: response["provider"].as_str().map(|s| s.to_string()),
        };
        out.record(&self.model);
        Ok(out)
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
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_text_response("Hello world")),
            )
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
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_text_response("just prose")),
            )
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

        let client = test_client(&server).with_retry_backoff(Duration::from_millis(1));
        assert!(matches!(
            client.complete("x").await.unwrap_err(),
            LlmError::RateLimited
        ));
    }

    /// The full multi-turn round trip: a tool call comes back with its id, and
    /// the usage block is decoded with reasoning tokens kept separate.
    #[tokio::test]
    async fn chat_returns_tool_calls_with_ids_and_usage() {
        let server = MockServer::start().await;
        let resp = json!({
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": { "name": "search", "arguments": "{\"query\": \"rent\"}" }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 412,
                "completion_tokens": 190,
                "total_tokens": 602,
                "completion_tokens_details": { "reasoning_tokens": 158 }
            }
        });
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .mount(&server)
            .await;

        let req =
            super::super::chat::ChatRequest::new(vec![super::super::chat::ChatMessage::User(
                "find the rent notice".into(),
            )]);
        let out = test_client(&server).chat(&req).await.unwrap();

        assert_eq!(out.tool_calls.len(), 1);
        // The id is what makes the answer addressable on the next turn.
        assert_eq!(out.tool_calls[0].id, "call_abc");
        assert_eq!(out.tool_calls[0].arguments["query"], "rent");
        assert!(out.content.is_none(), "a pure tool-call turn has no prose");
        assert_eq!(out.usage.reasoning_tokens, 158);
        assert_eq!(out.usage.prompt_tokens, 412);
        assert!(!out.truncated());
    }

    /// A reasoning model can spend the whole budget thinking and return nothing.
    /// That is a budget problem, not a capability one, and must be tellable apart.
    #[tokio::test]
    async fn an_empty_reply_that_ran_out_of_budget_reports_truncated() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "finish_reason": "length", "message": { "role": "assistant", "content": "" } }],
                "usage": { "completion_tokens": 3000, "completion_tokens_details": { "reasoning_tokens": 3000 } }
            })))
            .mount(&server)
            .await;

        let req =
            super::super::chat::ChatRequest::new(vec![super::super::chat::ChatMessage::User(
                "hi".into(),
            )]);
        let out = test_client(&server).chat(&req).await.unwrap();
        assert!(out.content.is_none());
        assert!(out.tool_calls.is_empty());
        assert!(
            out.truncated(),
            "must be distinguishable from 'nothing to say'"
        );
        assert_eq!(out.usage.reasoning_tokens, 3000);
    }

    /// Without pinning, a gateway may silently reroute and a measurement then
    /// says only that *something* answered, not whose stack did.
    #[tokio::test]
    async fn extra_body_is_merged_into_the_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(wiremock::matchers::body_partial_json(json!({
                "provider": { "order": ["deepinfra"], "allow_fallbacks": false }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_text_response("ok")))
            .mount(&server)
            .await;

        let client = OpenAiCompatClient::new(server.uri(), "test-model", "k").with_extra_body(
            json!({ "provider": { "order": ["deepinfra"], "allow_fallbacks": false } }),
        );
        // The mock only matches when the pin is present, so reaching a reply at
        // all is the assertion.
        assert_eq!(client.complete("x").await.unwrap(), "ok");
    }

    /// The other half of pinning: a pin is an instruction, this is the receipt.
    /// A constraint-tax number is about a serving stack, so a run that cannot
    /// name the stack that answered is unattributable — which already cost the
    /// archived spike one unusable result.
    #[tokio::test]
    async fn chat_reports_which_upstream_served_the_call() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                // Top level, alongside `choices` — not inside it.
                "provider": "DeepInfra",
                "choices": [{ "finish_reason": "stop", "message": { "role": "assistant", "content": "ok" } }]
            })))
            .mount(&server)
            .await;

        let req =
            super::super::chat::ChatRequest::new(vec![super::super::chat::ChatMessage::User(
                "hi".into(),
            )]);
        let out = test_client(&server).chat(&req).await.unwrap();
        assert_eq!(out.provider.as_deref(), Some("DeepInfra"));
    }

    /// Most OpenAI-compatible servers report no such field. That is silence, not
    /// a failure — a local Ollama has exactly one upstream and nothing to say.
    #[tokio::test]
    async fn an_endpoint_that_reports_no_provider_is_not_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_text_response("ok")))
            .mount(&server)
            .await;

        let req =
            super::super::chat::ChatRequest::new(vec![super::super::chat::ChatMessage::User(
                "hi".into(),
            )]);
        let out = test_client(&server).chat(&req).await.unwrap();
        assert!(out.provider.is_none());
    }

    /// A pinned upstream's shared pool can be briefly overloaded, and the 429
    /// that produces literally says "retry shortly". Giving up on the first one
    /// scores congestion as a model failure — observed live against DeepInfra,
    /// three requests in, with credit to spare.
    #[tokio::test]
    async fn a_call_rate_limited_once_still_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_text_response("ok")))
            .mount(&server)
            .await;

        let client = test_client(&server).with_retry_backoff(Duration::from_millis(1));
        assert_eq!(client.complete("x").await.unwrap(), "ok");
        // Counted, so the test cannot pass by never having been rate limited.
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            2,
            "one refusal, then the retry that succeeded"
        );
    }

    /// Retrying is bounded: a provider that is down stays down, and a run that
    /// never gives up is worse than one that reports the failure.
    #[tokio::test]
    async fn persistent_rate_limiting_gives_up_and_says_so() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let client = test_client(&server).with_retry_backoff(Duration::from_millis(1));
        let err = client.complete("x").await.unwrap_err();
        assert!(matches!(err, LlmError::RateLimited), "got {err:?}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1 + MAX_RATE_LIMIT_RETRIES as usize,
            "the first attempt plus every retry"
        );
    }

    /// Without this, an agent loop against a ten-per-minute free tier reports
    /// rate-limit errors that read as model failures.
    #[tokio::test]
    async fn a_min_interval_spaces_consecutive_requests() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_text_response("ok")))
            .mount(&server)
            .await;

        let client = OpenAiCompatClient::new(server.uri(), "m", "k")
            .with_min_interval(Duration::from_millis(300));

        let started = std::time::Instant::now();
        client.complete("a").await.unwrap();
        client.complete("b").await.unwrap();
        let elapsed = started.elapsed();

        // The first call does not wait; the second does.
        assert!(
            elapsed >= Duration::from_millis(280),
            "second request was not spaced: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn without_a_min_interval_nothing_is_throttled() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_text_response("ok")))
            .mount(&server)
            .await;

        let client = test_client(&server);
        let started = std::time::Instant::now();
        for _ in 0..3 {
            client.complete("x").await.unwrap();
        }
        // A local endpoint must not pay for a free tier's cap.
        assert!(started.elapsed() < Duration::from_millis(200));
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
