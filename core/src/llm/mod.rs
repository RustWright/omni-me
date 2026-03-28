mod client;
mod gemini;
mod prompts;
mod tools;

pub use client::{LlmClient, LlmError};
pub use gemini::GeminiClient;
pub use prompts::{CallMetadata, PromptRegistry, PromptTemplate};
pub use tools::{LlmResponse, ToolCall, ToolDef, ToolExecutor, default_note_tools};
