pub mod chat;
mod client;
mod gemini;
mod openai_compat;
pub mod pipeline;
mod prompts;
mod provider;
mod tools;

pub use chat::{ChatMessage, ChatRequest, ChatResponse, Usage};
pub use client::{LlmClient, LlmError};
pub use provider::{ClientOptions, build_llm_client, resolve_gemini_key};
pub use gemini::GeminiClient;
pub use openai_compat::OpenAiCompatClient;
pub use pipeline::{
    ExtractedDate, ExtractedExpense, ExtractedTask, NoteProcessingResult, PipelineError,
    process_note,
};
pub use prompts::{CallMetadata, PromptRegistry, PromptTemplate};
pub use tools::{LlmResponse, ToolCall, ToolDef, default_note_tools};
