use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use super::client::{LlmClient, LlmError};
use super::tools::{LlmResponse, ToolCall, ToolDef};

/// Minimum interval between API requests (rate limiting).
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(40);

/// Client for the Google Gemini API.
pub struct GeminiClient {
    api_key: String,
    model: String,
    base_url: String,
    http: reqwest::Client,
    last_request: Arc<Mutex<Instant>>,
}

impl GeminiClient {
    /// Create a new Gemini client with the given API key.
    ///
    /// Uses `gemini-2.0-flash` as the default model.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "gemini-2.0-flash".to_string(),
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            http: reqwest::Client::new(),
            last_request: Arc::new(Mutex::new(
                Instant::now()
                    .checked_sub(MIN_REQUEST_INTERVAL)
                    .unwrap_or_else(Instant::now),
            )),
        }
    }

    #[cfg(test)]
    fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Build the API endpoint URL.
    fn endpoint(&self) -> String {
        format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        )
    }

    /// Enforce rate limiting: sleep if needed to maintain minimum interval.
    async fn rate_limit(&self) {
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < MIN_REQUEST_INTERVAL {
            tokio::time::sleep(MIN_REQUEST_INTERVAL - elapsed).await;
        }
        *last = Instant::now();
    }

    /// Send a request body to the Gemini API and return the raw JSON response.
    async fn send_request(&self, body: Value) -> Result<Value, LlmError> {
        self.rate_limit().await;

        let response = self
            .http
            .post(&self.endpoint())
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::NetworkError(e.without_url().to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited);
        }

        let response_body: Value = response
            .json()
            .await
            .map_err(|e| LlmError::ParseError(format!("Failed to parse response JSON: {}", e.without_url())))?;

        if !status.is_success() {
            let error_msg = response_body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown API error");
            return Err(LlmError::ApiError(format!(
                "HTTP {status}: {error_msg}"
            )));
        }

        Ok(response_body)
    }

    /// Extract the text content from a Gemini API response.
    fn extract_text(response: &Value) -> Result<String, LlmError> {
        response["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                LlmError::ParseError("No text found in response".to_string())
            })
    }

    /// Convert ToolDef list to Gemini API function declarations format.
    fn tool_defs_to_gemini(tools: &[ToolDef]) -> Value {
        let declarations: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                })
            })
            .collect();

        json!([{ "functionDeclarations": declarations }])
    }

    /// Parse tool calls from a Gemini API response.
    fn parse_tool_calls(response: &Value) -> Option<Vec<ToolCall>> {
        let parts = response["candidates"][0]["content"]["parts"].as_array()?;

        let calls: Vec<ToolCall> = parts
            .iter()
            .filter_map(|part| {
                let fc = part.get("functionCall")?;
                Some(ToolCall {
                    name: fc["name"].as_str()?.to_string(),
                    arguments: fc["args"].clone(),
                })
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
impl LlmClient for GeminiClient {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        let body = json!({
            "contents": [{ "parts": [{ "text": prompt }] }]
        });

        let response = self.send_request(body).await?;
        Self::extract_text(&response)
    }

    async fn complete_json(
        &self,
        prompt: &str,
        schema: &Value,
    ) -> Result<Value, LlmError> {
        let body = json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "generationConfig": {
                "responseMimeType": "application/json",
                "responseSchema": schema
            }
        });

        let response = self.send_request(body).await?;
        let text = Self::extract_text(&response)?;

        serde_json::from_str(&text).map_err(|e| {
            LlmError::ParseError(format!("Failed to parse JSON response: {e}"))
        })
    }

    async fn complete_with_tools(
        &self,
        prompt: &str,
        tools: &[ToolDef],
    ) -> Result<LlmResponse, LlmError> {
        let body = json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "tools": Self::tool_defs_to_gemini(tools)
        });

        let response = self.send_request(body).await?;

        // Check for tool calls first
        if let Some(calls) = Self::parse_tool_calls(&response) {
            return Ok(LlmResponse::ToolCalls(calls));
        }

        // Fall back to text response
        let text = Self::extract_text(&response)?;
        Ok(LlmResponse::Text(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(server: &MockServer) -> GeminiClient {
        GeminiClient::new("test-key".to_string()).with_base_url(server.uri())
    }

    fn gemini_text_response(text: &str) -> Value {
        json!({
            "candidates": [{
                "content": {
                    "parts": [{ "text": text }]
                }
            }]
        })
    }

    fn gemini_tool_call_response(name: &str, args: Value) -> Value {
        json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": name,
                            "args": args
                        }
                    }]
                }
            }]
        })
    }

    #[tokio::test]
    async fn test_complete_basic() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(gemini_text_response("Hello world")),
            )
            .mount(&server)
            .await;

        let client = test_client(&server);
        let body = json!({
            "contents": [{ "parts": [{ "text": "Say hello" }] }]
        });
        let response = client.send_request(body).await.unwrap();
        let text = GeminiClient::extract_text(&response).unwrap();
        assert_eq!(text, "Hello world");
    }

    #[tokio::test]
    async fn test_complete_json() {
        let server = MockServer::start().await;

        let json_response = r#"{"tags": ["work", "meeting"], "category": "productivity"}"#;
        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(gemini_text_response(json_response)),
            )
            .mount(&server)
            .await;

        let client = test_client(&server);
        let body = json!({
            "contents": [{ "parts": [{ "text": "Extract data" }] }],
            "generationConfig": {
                "responseMimeType": "application/json"
            }
        });
        let response = client.send_request(body).await.unwrap();
        let text = GeminiClient::extract_text(&response).unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["category"], "productivity");
        assert_eq!(parsed["tags"][0], "work");
    }

    #[tokio::test]
    async fn test_complete_with_tools_returns_tool_calls() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                gemini_tool_call_response("create_tag", json!({"tag": "personal"})),
            ))
            .mount(&server)
            .await;

        let client = test_client(&server);
        let body = json!({
            "contents": [{ "parts": [{ "text": "Process this note" }] }],
            "tools": GeminiClient::tool_defs_to_gemini(&super::super::tools::default_note_tools())
        });
        let response = client.send_request(body).await.unwrap();
        let calls = GeminiClient::parse_tool_calls(&response).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "create_tag");
        assert_eq!(calls[0].arguments["tag"], "personal");
    }

    #[tokio::test]
    async fn test_api_error_handling() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {
                    "message": "Invalid request"
                }
            })))
            .mount(&server)
            .await;

        let client = test_client(&server);
        let body = json!({"contents": [{"parts": [{"text": "test"}]}]});
        let result = client.send_request(body).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::ApiError(msg) => assert!(msg.contains("Invalid request")),
            other => panic!("Expected ApiError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_rate_limit_response() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({
                "error": { "message": "Rate limited" }
            })))
            .mount(&server)
            .await;

        let client = test_client(&server);
        let body = json!({"contents": [{"parts": [{"text": "test"}]}]});
        let result = client.send_request(body).await;
        assert!(matches!(result.unwrap_err(), LlmError::RateLimited));
    }

    #[tokio::test]
    async fn test_rate_limiting_enforced() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(gemini_text_response("ok")),
            )
            .expect(2)
            .mount(&server)
            .await;

        let client = test_client(&server);

        let start = Instant::now();
        let body1 = json!({"contents": [{"parts": [{"text": "first"}]}]});
        let _ = client.send_request(body1).await.unwrap();
        let body2 = json!({"contents": [{"parts": [{"text": "second"}]}]});
        let _ = client.send_request(body2).await.unwrap();
        let elapsed = start.elapsed();

        // Second request should have been delayed by at least ~40ms
        assert!(
            elapsed >= Duration::from_millis(35),
            "Rate limiting should enforce minimum interval, elapsed: {elapsed:?}"
        );
    }

    #[test]
    fn test_extract_text_missing() {
        let bad_response = json!({"candidates": [{"content": {"parts": []}}]});
        assert!(GeminiClient::extract_text(&bad_response).is_err());
    }

    #[test]
    fn test_tool_defs_to_gemini_format() {
        let tools = super::super::tools::default_note_tools();
        let gemini_tools = GeminiClient::tool_defs_to_gemini(&tools);
        let declarations = &gemini_tools[0]["functionDeclarations"];
        assert!(declarations.is_array());
        assert_eq!(declarations.as_array().unwrap().len(), 4);
        assert_eq!(declarations[0]["name"], "create_tag");
    }

    #[test]
    fn test_parse_tool_calls_none_when_text() {
        let response = gemini_text_response("Just text");
        assert!(GeminiClient::parse_tool_calls(&response).is_none());
    }

    #[test]
    fn test_parse_tool_calls_multiple() {
        let response = json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {
                            "functionCall": {
                                "name": "create_tag",
                                "args": {"tag": "work"}
                            }
                        },
                        {
                            "functionCall": {
                                "name": "extract_task",
                                "args": {"description": "Review PR", "priority": "high"}
                            }
                        }
                    ]
                }
            }]
        });
        let calls = GeminiClient::parse_tool_calls(&response).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "create_tag");
        assert_eq!(calls[1].name, "extract_task");
    }

    #[test]
    fn test_gemini_client_default_model() {
        let client = GeminiClient::new("key".to_string());
        assert_eq!(client.model, "gemini-2.0-flash");
    }

    #[tokio::test]
    async fn test_network_error_does_not_leak_api_key() {
        let secret_key = "super-secret-api-key-12345";
        let client = GeminiClient::new(secret_key.to_string())
            .with_base_url("http://127.0.0.1:1".to_string()); // unreachable port

        let body = json!({"contents": [{"parts": [{"text": "test"}]}]});
        let err = client.send_request(body).await.unwrap_err();
        let err_msg = err.to_string();
        assert!(!err_msg.contains(secret_key), "API key leaked in network error: {err_msg}");
    }

    #[tokio::test]
    async fn test_parse_error_does_not_leak_api_key() {
        let server = MockServer::start().await;
        let secret_key = "super-secret-api-key-67890";

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&server)
            .await;

        let client = GeminiClient::new(secret_key.to_string())
            .with_base_url(server.uri());

        let body = json!({"contents": [{"parts": [{"text": "test"}]}]});
        let err = client.send_request(body).await.unwrap_err();
        let err_msg = err.to_string();
        assert!(!err_msg.contains(secret_key), "API key leaked in parse error: {err_msg}");
    }
}
