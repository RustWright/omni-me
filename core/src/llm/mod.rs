mod client;
mod gemini;
pub mod pipeline;
mod prompts;
mod tools;

pub use client::{LlmClient, LlmError};
pub use gemini::GeminiClient;
pub use pipeline::{
    ExtractedDate, ExtractedExpense, ExtractedTask, MoodAssessment, NoteProcessingResult,
    PipelineError, process_note,
};
pub use prompts::{CallMetadata, PromptRegistry, PromptTemplate};
pub use tools::{LlmResponse, ToolCall, ToolDef, default_note_tools};
