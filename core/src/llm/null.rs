//! The client that answers nothing, and says why.
//!
//! Two situations need a `dyn LlmClient` that cannot work: no `[llm]` section at
//! all, and a section naming a model we refuse to send data to. Both must still
//! **boot** — a host with no LLM configured is a supported deployment, and the
//! server has no way to ask a human for a key mid-startup — so the failure has to
//! be deferred to call time rather than raised at construction.
//!
//! What it replaces is worth stating: the previous fallback built a *Gemini*
//! client around an empty key, so "no LLM configured" surfaced as a Gemini auth
//! error against an endpoint the operator never chose. The reason travels with
//! the client now, so the message names the actual problem.

use async_trait::async_trait;
use serde_json::Value;

use super::chat::{ChatRequest, ChatResponse};
use super::client::{LlmClient, LlmError};
use super::tools::{LlmResponse, ToolDef};

/// A client whose every call fails with a fixed, specific explanation.
#[derive(Debug, Clone)]
pub struct NullLlmClient {
    reason: String,
}

impl NullLlmClient {
    /// No `[llm]` section, or one too incomplete to build a client from.
    pub fn unconfigured() -> Self {
        Self {
            reason: "no LLM endpoint is configured: set [llm] provider = \
                     \"openai_compatible\" with base_url, model and api_key in \
                     credentials.toml"
                .to_string(),
        }
    }

    /// A configured model we will not send records to. See [`super::provider`].
    pub fn refused(model: &str, detail: &str) -> Self {
        Self {
            reason: format!("refusing to use model `{model}`: {detail}"),
        }
    }

    fn err<T>(&self) -> Result<T, LlmError> {
        Err(LlmError::ApiError(self.reason.clone()))
    }
}

#[async_trait]
impl LlmClient for NullLlmClient {
    fn model_name(&self) -> &str {
        "none"
    }

    async fn complete(&self, _prompt: &str) -> Result<String, LlmError> {
        self.err()
    }

    async fn complete_json(&self, _prompt: &str, _schema: &Value) -> Result<Value, LlmError> {
        self.err()
    }

    async fn complete_with_tools(
        &self,
        _prompt: &str,
        _tools: &[ToolDef],
    ) -> Result<LlmResponse, LlmError> {
        self.err()
    }

    async fn chat(&self, _request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        self.err()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unconfigured_names_the_missing_section() {
        let err = NullLlmClient::unconfigured()
            .complete("x")
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("[llm]"),
            "should name the config section: {msg}"
        );
    }

    /// The refusal has to carry the model, or an operator reading a log line
    /// cannot tell which of several configured endpoints was rejected.
    #[tokio::test]
    async fn refusal_names_the_model_and_the_reason() {
        let err = NullLlmClient::refused("anthropic/claude-opus-4-8", "closed weights")
            .complete("x")
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("anthropic/claude-opus-4-8"), "{msg}");
        assert!(msg.contains("closed weights"), "{msg}");
    }

    #[tokio::test]
    async fn every_method_fails_rather_than_returning_an_empty_success() {
        let c = NullLlmClient::unconfigured();
        assert!(c.complete_json("x", &Value::Null).await.is_err());
        assert!(c.complete_with_tools("x", &[]).await.is_err());
        assert!(c.chat(&ChatRequest::new(vec![])).await.is_err());
    }
}
