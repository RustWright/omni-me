//! Multi-turn chat: the shape an agent loop needs, and the one the single-shot
//! `LlmClient` methods cannot express.
//!
//! `complete_with_tools` takes one prompt and returns one response. An agent has
//! to hand tool *results* back and let the model continue, which needs a message
//! list, the provider's own call ids to answer against, and somewhere to put the
//! token counts. None of that fits a `&str`.
//!
//! ## Instrumentation is part of the contract here, not an add-on
//!
//! Every call records latency and token counts, with **reasoning tokens counted
//! separately** from completion tokens. They are billed and rate-limited like any
//! other output token and were roughly 95% of output on the model this was first
//! measured against, so folding them together misreports both cost and latency.
//!
//! ⚠️ **Message content is never logged.** A model quotes the user's records back,
//! and a log is the easiest place for that to escape — the promise is written down
//! in `docs/src/assistant.md` § "What leaves your machine". Counts and timings
//! only.

use std::time::Duration;

use serde_json::{Value, json};

use super::tools::{ToolCall, ToolDef};

/// One message in the conversation.
#[derive(Debug, Clone)]
pub enum ChatMessage {
    System(String),
    User(String),
    /// What the model said, including any calls it wants made. Kept verbatim and
    /// replayed on the next turn — a provider that sees its own tool call missing
    /// from the history will often reissue it.
    Assistant {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    /// The answer to one call, addressed by the id the model gave it.
    ToolResult {
        tool_call_id: String,
        name: String,
        content: String,
    },
}

impl ChatMessage {
    /// OpenAI chat-completions wire form.
    fn to_wire(&self) -> Value {
        match self {
            ChatMessage::System(c) => json!({ "role": "system", "content": c }),
            ChatMessage::User(c) => json!({ "role": "user", "content": c }),
            ChatMessage::Assistant {
                content,
                tool_calls,
            } => {
                let calls: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                // Arguments go back as a JSON *string*, which is
                                // how they arrived. Sending an object here is
                                // accepted by some endpoints and rejected by
                                // others.
                                "arguments": serde_json::to_string(&tc.arguments)
                                    .unwrap_or_else(|_| "{}".to_string()),
                            }
                        })
                    })
                    .collect();
                let mut m = json!({ "role": "assistant" });
                // `content` is null rather than absent on a pure tool-call turn:
                // several endpoints reject an assistant message with neither.
                m["content"] = match content {
                    Some(c) => Value::String(c.clone()),
                    None => Value::Null,
                };
                if !calls.is_empty() {
                    m["tool_calls"] = Value::Array(calls);
                }
                m
            }
            ChatMessage::ToolResult {
                tool_call_id,
                name,
                content,
            } => json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "name": name,
                "content": content,
            }),
        }
    }
}

/// One turn's request.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDef>,
    /// Constrain the reply to a JSON schema instead of free-form tool calling.
    ///
    /// Present so the constraint tax can be **measured on our own tool surface**
    /// rather than assumed: structured decoding is reported both as a large
    /// accuracy win and as a suppressor of tool calling, and only running it both
    /// ways settles which one we get.
    pub response_schema: Option<Value>,
    /// Must cover reasoning *and* the answer. Set it too low against a reasoning
    /// model and the whole budget is spent thinking, returning nothing at all —
    /// which reads as a capability failure and is not one.
    pub max_tokens: u32,
    /// Qwen chat-template switch (`chat_template_kwargs.enable_thinking`).
    ///
    /// `None` means do not send it, which is the right default for anything that
    /// is not a Qwen: an unknown key risks an HTTP 400. Where it is supported,
    /// disabling thinking is what lets schema-constrained output terminate on
    /// stacks that otherwise apply the grammar from token zero. The widely-cited
    /// `/no_think` prompt suffix is silently ignored and is not an alternative.
    pub enable_thinking: Option<bool>,
}

impl ChatRequest {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            tools: Vec::new(),
            response_schema: None,
            max_tokens: 3000,
            enable_thinking: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolDef>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_response_schema(mut self, schema: Value) -> Self {
        self.response_schema = Some(schema);
        self
    }

    pub fn with_thinking(mut self, enabled: bool) -> Self {
        self.enable_thinking = Some(enabled);
        self
    }

    /// Serialize to an OpenAI chat-completions body.
    pub fn to_wire(&self, model: &str) -> Value {
        let mut body = json!({
            "model": model,
            "messages": self.messages.iter().map(|m| m.to_wire()).collect::<Vec<_>>(),
            "max_tokens": self.max_tokens,
        });
        if !self.tools.is_empty() {
            body["tools"] = super::openai_compat::tool_defs_to_openai(&self.tools);
        }
        if let Some(schema) = &self.response_schema {
            body["response_format"] = schema.clone();
        }
        if let Some(thinking) = self.enable_thinking {
            body["chat_template_kwargs"] = json!({ "enable_thinking": thinking });
        }
        body
    }
}

/// What one call cost. Zeros mean the endpoint reported nothing, not that the
/// call was free — providers differ on whether `usage` is returned at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// Billed and rate-limited as output, and reported separately because it is
    /// routinely the *majority* of it. Folding it into `completion_tokens` hides
    /// where the money and the latency actually went.
    pub reasoning_tokens: u32,
    pub total_tokens: u32,
}

impl Usage {
    /// Parse the `usage` block of a chat-completions response.
    pub fn from_response(response: &Value) -> Self {
        let usage = &response["usage"];
        Self {
            prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
            reasoning_tokens: usage["completion_tokens_details"]["reasoning_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
            total_tokens: usage["total_tokens"].as_u64().unwrap_or(0) as u32,
        }
    }
}

/// One turn's reply.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
    pub latency: Duration,
    /// Why the model stopped. `"length"` here with an empty reply is the
    /// out-of-budget case described on [`ChatRequest::max_tokens`], and is worth
    /// distinguishing from a model that simply had nothing to say.
    pub finish_reason: Option<String>,
}

impl ChatResponse {
    /// Emit the cost line for this call.
    ///
    /// Counts and timings only — never `content`, never a tool argument. See the
    /// warning in this module's header.
    pub fn record(&self, model: &str) {
        tracing::info!(
            model,
            latency_ms = self.latency.as_millis() as u64,
            prompt_tokens = self.usage.prompt_tokens,
            completion_tokens = self.usage.completion_tokens,
            reasoning_tokens = self.usage.reasoning_tokens,
            total_tokens = self.usage.total_tokens,
            tool_calls = self.tool_calls.len(),
            finish_reason = self.finish_reason.as_deref().unwrap_or("none"),
            "llm call"
        );
    }

    /// Whether the reply was cut off before the model finished.
    pub fn truncated(&self) -> bool {
        self.finish_reason.as_deref() == Some("length")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: json!({ "query": "rent" }),
        }
    }

    #[test]
    fn a_tool_result_addresses_the_call_by_id() {
        let wire = ChatMessage::ToolResult {
            tool_call_id: "call_7".into(),
            name: "search".into(),
            content: "{}".into(),
        }
        .to_wire();
        assert_eq!(wire["role"], "tool");
        assert_eq!(wire["tool_call_id"], "call_7");
    }

    /// Arguments must go back as a JSON string, matching how they arrived.
    #[test]
    fn assistant_tool_calls_serialize_arguments_as_a_string() {
        let wire = ChatMessage::Assistant {
            content: None,
            tool_calls: vec![call("call_1", "search")],
        }
        .to_wire();
        let args = &wire["tool_calls"][0]["function"]["arguments"];
        assert!(args.is_string(), "must be a string, got {args}");
        assert_eq!(
            serde_json::from_str::<Value>(args.as_str().unwrap()).unwrap()["query"],
            "rent"
        );
    }

    /// A pure tool-call turn still carries an explicit null content — several
    /// endpoints reject an assistant message with neither field.
    #[test]
    fn a_tool_call_turn_carries_explicit_null_content() {
        let wire = ChatMessage::Assistant {
            content: None,
            tool_calls: vec![call("call_1", "search")],
        }
        .to_wire();
        assert!(wire.get("content").is_some());
        assert!(wire["content"].is_null());
    }

    #[test]
    fn thinking_is_omitted_unless_asked_for() {
        let req = ChatRequest::new(vec![ChatMessage::User("hi".into())]);
        assert!(
            req.to_wire("m").get("chat_template_kwargs").is_none(),
            "sending an unknown key risks a 400 on non-Qwen models"
        );
        let wire = req.with_thinking(false).to_wire("m");
        assert_eq!(wire["chat_template_kwargs"]["enable_thinking"], false);
    }

    #[test]
    fn tools_are_omitted_when_there_are_none() {
        let req = ChatRequest::new(vec![ChatMessage::User("hi".into())]);
        assert!(req.to_wire("m").get("tools").is_none());
    }

    #[test]
    fn reasoning_tokens_are_read_from_their_own_field() {
        let usage = Usage::from_response(&json!({
            "usage": {
                "prompt_tokens": 412,
                "completion_tokens": 190,
                "total_tokens": 602,
                "completion_tokens_details": { "reasoning_tokens": 158 }
            }
        }));
        assert_eq!(usage.prompt_tokens, 412);
        assert_eq!(usage.completion_tokens, 190);
        // The number that gets lost if these are folded together, and the one
        // that was ~95% of output on the model this was measured against.
        assert_eq!(usage.reasoning_tokens, 158);
    }

    #[test]
    fn a_missing_usage_block_is_zeros_not_a_failure() {
        assert_eq!(Usage::from_response(&json!({})), Usage::default());
    }

    #[test]
    fn truncation_is_distinguishable_from_having_nothing_to_say() {
        let cut = ChatResponse {
            content: None,
            tool_calls: vec![],
            usage: Usage::default(),
            latency: Duration::ZERO,
            finish_reason: Some("length".into()),
        };
        assert!(cut.truncated());
        let done = ChatResponse {
            finish_reason: Some("stop".into()),
            ..cut.clone()
        };
        assert!(!done.truncated());
    }
}
