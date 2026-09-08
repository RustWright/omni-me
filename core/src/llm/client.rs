use async_trait::async_trait;
use serde_json::Value;

use super::chat::{ChatRequest, ChatResponse};
use super::tools::{LlmResponse, ToolDef};

/// Errors that can occur during LLM interactions.
#[derive(Debug, Clone, thiserror::Error)]
pub enum LlmError {
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Rate limited by API")]
    RateLimited,
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
}

/// Trait for interacting with LLM providers.
///
/// Object-safe: no generic methods, can be used as `Box<dyn LlmClient>`.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// The model identifier used by this client (e.g. "gemini-2.0-flash").
    fn model_name(&self) -> &str;

    /// Generate a plain text completion from the given prompt.
    async fn complete(&self, prompt: &str) -> Result<String, LlmError>;

    /// Generate a structured JSON completion conforming to the given JSON Schema.
    async fn complete_json(&self, prompt: &str, schema: &Value) -> Result<Value, LlmError>;

    /// Generate a completion that may include tool calls.
    async fn complete_with_tools(
        &self,
        prompt: &str,
        tools: &[ToolDef],
    ) -> Result<LlmResponse, LlmError>;

    /// One turn of a multi-turn conversation, with token counts and latency.
    ///
    /// Separate from [`Self::complete_with_tools`] because an agent loop needs to
    /// hand tool *results* back and let the model continue, which a single prompt
    /// string cannot express.
    ///
    /// **No default implementation, deliberately.** A default that returned
    /// "unsupported" would let a new provider compile while silently being unable
    /// to run the assistant; requiring the method makes that a decision someone
    /// has to make on purpose.
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError>;
}
