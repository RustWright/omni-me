use async_trait::async_trait;
use serde_json::Value;
use std::fmt;

use super::tools::{LlmResponse, ToolDef};

/// Errors that can occur during LLM interactions.
#[derive(Debug, Clone)]
pub enum LlmError {
    /// The API returned an error response.
    ApiError(String),
    /// The API rate limited the request.
    RateLimited,
    /// Failed to parse a response from the API.
    ParseError(String),
    /// Network-level error (connection, DNS, timeout, etc).
    NetworkError(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::ApiError(msg) => write!(f, "API error: {msg}"),
            LlmError::RateLimited => write!(f, "Rate limited by API"),
            LlmError::ParseError(msg) => write!(f, "Parse error: {msg}"),
            LlmError::NetworkError(msg) => write!(f, "Network error: {msg}"),
        }
    }
}

impl std::error::Error for LlmError {}

/// Trait for interacting with LLM providers.
///
/// Object-safe: no generic methods, can be used as `Box<dyn LlmClient>`.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Generate a plain text completion from the given prompt.
    async fn complete(&self, prompt: &str) -> Result<String, LlmError>;

    /// Generate a structured JSON completion conforming to the given JSON Schema.
    async fn complete_json(
        &self,
        prompt: &str,
        schema: &Value,
    ) -> Result<Value, LlmError>;

    /// Generate a completion that may include tool calls.
    async fn complete_with_tools(
        &self,
        prompt: &str,
        tools: &[ToolDef],
    ) -> Result<LlmResponse, LlmError>;
}
