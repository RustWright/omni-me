mod client;
mod gemini;
mod openai_compat;
pub mod pipeline;
mod prompts;
mod tools;

pub use client::{LlmClient, LlmError};
pub use gemini::GeminiClient;
pub use openai_compat::OpenAiCompatClient;
pub use pipeline::{
    ExtractedDate, ExtractedExpense, ExtractedTask, NoteProcessingResult,
    PipelineError, process_note,
};
pub use prompts::{CallMetadata, PromptRegistry, PromptTemplate};
pub use tools::{LlmResponse, ToolCall, ToolDef, default_note_tools};
