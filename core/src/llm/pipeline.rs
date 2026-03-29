use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::events::{EventStore, EventType, NewEvent, NoteLlmProcessedPayload};
use crate::preprocess;

use super::client::{LlmClient, LlmError};
use super::prompts::{CallMetadata, PromptRegistry};
use super::tools::{LlmResponse, default_note_tools};

/// Result of processing a note through the LLM pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteProcessingResult {
    pub tags: Vec<String>,
    pub mood: Option<MoodAssessment>,
    pub tasks: Vec<ExtractedTask>,
    pub dates: Vec<ExtractedDate>,
    pub expenses: Vec<ExtractedExpense>,
    pub summary: Option<String>,
    pub urls: Vec<String>,
    pub metadata: CallMetadata,
}

/// A mood assessment extracted from a journal entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoodAssessment {
    pub mood: String,
    pub confidence: f64,
}

/// A task extracted from a journal entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedTask {
    pub description: String,
    pub priority: String,
}

/// A date mentioned in a journal entry, interpreted by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedDate {
    pub date: String,
    pub context: String,
}

/// An expense or financial transaction extracted from a journal entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedExpense {
    pub amount: f64,
    pub currency: String,
    pub description: String,
}

/// Errors that can occur during note processing.
#[derive(Debug)]
pub enum PipelineError {
    Llm(LlmError),
    Event(crate::events::EventError),
    Processing(String),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Llm(e) => write!(f, "LLM error: {e}"),
            PipelineError::Event(e) => write!(f, "Event error: {e}"),
            PipelineError::Processing(msg) => write!(f, "Processing error: {msg}"),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<LlmError> for PipelineError {
    fn from(e: LlmError) -> Self {
        PipelineError::Llm(e)
    }
}

impl From<crate::events::EventError> for PipelineError {
    fn from(e: crate::events::EventError) -> Self {
        PipelineError::Event(e)
    }
}

/// Process a journal note through the full LLM pipeline.
///
/// Flow: raw_text → preprocess → build prompt → LLM with tools → parse tool calls → emit event
pub async fn process_note(
    note_id: &str,
    raw_text: &str,
    device_id: &str,
    llm: &dyn LlmClient,
    event_store: &dyn EventStore,
) -> Result<NoteProcessingResult, PipelineError> {
    // Step 1: Deterministic pre-processing (URLs only — fuzzy data handled by LLM)
    let preprocessed = preprocess::preprocess(raw_text);

    // Step 2: Build prompt from template
    let registry = PromptRegistry::new();
    let template = registry.get("note_process_v1").ok_or_else(|| {
        PipelineError::Processing("note_process_v1 template not found".to_string())
    })?;

    let context = json!({
        "urls": preprocessed.urls,
        "raw_text": raw_text,
    });

    let prompt = registry.render("note_process_v1", &context)?;

    // Step 3: Call LLM with tools
    let tools = default_note_tools();
    let model_name = "gemini-2.0-flash";
    let response = llm.complete_with_tools(&prompt, &tools).await?;

    // Step 4: Parse tool calls into structured result
    let result_data = interpret_tool_calls(response)?;

    let metadata = CallMetadata {
        prompt_name: template.name.to_string(),
        prompt_version: template.version.to_string(),
        model: model_name.to_string(),
        timestamp: Utc::now(),
    };

    let result = NoteProcessingResult {
        tags: result_data.tags,
        mood: result_data.mood,
        tasks: result_data.tasks,
        dates: result_data.dates,
        expenses: result_data.expenses,
        summary: result_data.summary,
        urls: preprocessed.urls,
        metadata: metadata.clone(),
    };

    // Step 5: Emit note_llm_processed event
    let derived = serde_json::to_value(&result)
        .map_err(|e| PipelineError::Processing(format!("Failed to serialize result: {e}")))?;

    let payload = NoteLlmProcessedPayload {
        note_id: note_id.to_string(),
        prompt_version: format!("{}@{}", template.name, template.version),
        model: model_name.to_string(),
        derived,
    };

    event_store
        .append(NewEvent {
            id: None,
            event_type: EventType::NoteLlmProcessed.to_string(),
            aggregate_id: note_id.to_string(),
            timestamp: Utc::now(),
            device_id: device_id.to_string(),
            payload: serde_json::to_value(&payload).map_err(|e| {
                PipelineError::Processing(format!("Failed to serialize payload: {e}"))
            })?,
        })
        .await?;

    Ok(result)
}

/// Intermediate struct for collecting parsed tool call results.
struct ParsedToolCalls {
    tags: Vec<String>,
    mood: Option<MoodAssessment>,
    tasks: Vec<ExtractedTask>,
    dates: Vec<ExtractedDate>,
    expenses: Vec<ExtractedExpense>,
    summary: Option<String>,
}

/// Helper to extract a required string field from tool call arguments.
fn require_str(args: &serde_json::Value, field: &str, tool: &str) -> Result<String, PipelineError> {
    args[field]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| PipelineError::Processing(format!("{tool}: missing or invalid '{field}'")))
}

/// Helper to extract a required f64 field from tool call arguments.
fn require_f64(args: &serde_json::Value, field: &str, tool: &str) -> Result<f64, PipelineError> {
    args[field]
        .as_f64()
        .ok_or_else(|| PipelineError::Processing(format!("{tool}: missing or invalid '{field}'")))
}

/// Interpret LLM tool calls into structured note data.
fn interpret_tool_calls(response: LlmResponse) -> Result<ParsedToolCalls, PipelineError> {
    match response {
        LlmResponse::Text(text_response) => Ok(ParsedToolCalls {
            tags: vec![],
            mood: None,
            tasks: vec![],
            dates: vec![],
            expenses: vec![],
            summary: Some(text_response),
        }),
        LlmResponse::ToolCalls(tool_call_vec) => {
            let mut tags = Vec::new();
            let mut tasks = Vec::new();
            let mut dates = Vec::new();
            let mut expenses = Vec::new();
            let mut mood: Option<MoodAssessment> = None;

            for tc in tool_call_vec {
                match tc.name.as_str() {
                    "create_tag" => {
                        tags.push(require_str(&tc.arguments, "tag", "create_tag")?);
                    }
                    "assess_mood" => {
                        mood = Some(MoodAssessment {
                            mood: require_str(&tc.arguments, "mood", "assess_mood")?,
                            confidence: require_f64(&tc.arguments, "confidence", "assess_mood")?,
                        });
                    }
                    "extract_task" => {
                        tasks.push(ExtractedTask {
                            description: require_str(&tc.arguments, "description", "extract_task")?,
                            priority: require_str(&tc.arguments, "priority", "extract_task")?,
                        });
                    }
                    "extract_date" => {
                        dates.push(ExtractedDate {
                            date: require_str(&tc.arguments, "date", "extract_date")?,
                            context: require_str(&tc.arguments, "context", "extract_date")?,
                        });
                    }
                    "extract_expense" => {
                        expenses.push(ExtractedExpense {
                            amount: require_f64(&tc.arguments, "amount", "extract_expense")?,
                            currency: require_str(&tc.arguments, "currency", "extract_expense")?,
                            description: require_str(&tc.arguments, "description", "extract_expense")?,
                        });
                    }
                    _ => {}
                }
            }

            Ok(ParsedToolCalls {
                tags,
                mood,
                tasks,
                dates,
                expenses,
                summary: None,
            })
        }
        LlmResponse::Structured(structured_response) => Err(PipelineError::Processing(format!(
            "Behaviour not defined for llm structured format, the following structured format was provided:\n{structured_response}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tools::ToolCall;
    use super::*;
    use async_trait::async_trait;
    use serde_json::Value;

    /// Mock LLM client that returns a predetermined tool-call response.
    struct _MockLlmClient {
        response: LlmResponse,
    }

    #[async_trait]
    impl LlmClient for _MockLlmClient {
        async fn complete(&self, _prompt: &str) -> Result<String, LlmError> {
            Ok("mock".to_string())
        }

        async fn complete_json(&self, _prompt: &str, _schema: &Value) -> Result<Value, LlmError> {
            Ok(json!({}))
        }

        async fn complete_with_tools(
            &self,
            _prompt: &str,
            _tools: &[super::super::tools::ToolDef],
        ) -> Result<LlmResponse, LlmError> {
            Ok(self.response.clone())
        }
    }

    #[test]
    fn test_preprocess_feeds_into_prompt() {
        let text = "On 2026-03-27 I spent $15.00 at https://store.com";
        let preprocessed = preprocess::preprocess(text);
        assert_eq!(preprocessed.urls.len(), 1);

        let registry = PromptRegistry::new();
        let context = json!({
            "urls": preprocessed.urls,
            "raw_text": text,
        });
        let prompt = registry.render("note_process_v1", &context).unwrap();
        assert!(prompt.contains("https://store.com"));
        // Dates and amounts are now in the raw_text for the LLM to find
        assert!(prompt.contains("2026-03-27"));
        assert!(prompt.contains("$15.00"));
    }

    #[test]
    fn test_interpret_tool_calls_with_all_tools() {
        let response = LlmResponse::ToolCalls(vec![
            ToolCall {
                name: "create_tag".to_string(),
                arguments: json!({"tag": "shopping"}),
            },
            ToolCall {
                name: "create_tag".to_string(),
                arguments: json!({"tag": "food"}),
            },
            ToolCall {
                name: "assess_mood".to_string(),
                arguments: json!({"mood": "content", "confidence": 0.8}),
            },
            ToolCall {
                name: "extract_task".to_string(),
                arguments: json!({"description": "Buy groceries", "priority": "medium"}),
            },
            ToolCall {
                name: "extract_date".to_string(),
                arguments: json!({"date": "2026-03-28", "context": "entry date"}),
            },
            ToolCall {
                name: "extract_expense".to_string(),
                arguments: json!({"amount": 15.0, "currency": "USD", "description": "lunch"}),
            },
        ]);

        let result = interpret_tool_calls(response).unwrap();
        assert_eq!(result.tags, vec!["shopping", "food"]);
        assert!(result.mood.is_some());
        assert_eq!(result.mood.as_ref().unwrap().mood, "content");
        assert!((result.mood.as_ref().unwrap().confidence - 0.8).abs() < f64::EPSILON);
        assert_eq!(result.tasks.len(), 1);
        assert_eq!(result.tasks[0].description, "Buy groceries");
        assert_eq!(result.dates.len(), 1);
        assert_eq!(result.dates[0].date, "2026-03-28");
        assert_eq!(result.dates[0].context, "entry date");
        assert_eq!(result.expenses.len(), 1);
        assert!((result.expenses[0].amount - 15.0).abs() < f64::EPSILON);
        assert_eq!(result.expenses[0].currency, "USD");
    }

    #[test]
    fn test_interpret_tool_calls_text_fallback() {
        let response = LlmResponse::Text("The entry seems positive.".to_string());
        let result = interpret_tool_calls(response).unwrap();
        assert!(result.tags.is_empty());
        assert!(result.mood.is_none());
        assert!(result.tasks.is_empty());
        assert_eq!(
            result.summary,
            Some("The entry seems positive.".to_string())
        );
    }

    #[test]
    fn test_interpret_tool_calls_empty() {
        let response = LlmResponse::ToolCalls(vec![]);
        let result = interpret_tool_calls(response).unwrap();
        assert!(result.tags.is_empty());
        assert!(result.mood.is_none());
        assert!(result.tasks.is_empty());
    }

    #[test]
    fn test_interpret_tool_calls_unknown_tool_ignored() {
        let response = LlmResponse::ToolCalls(vec![
            ToolCall {
                name: "create_tag".to_string(),
                arguments: json!({"tag": "test"}),
            },
            ToolCall {
                name: "unknown_tool".to_string(),
                arguments: json!({"foo": "bar"}),
            },
        ]);

        let result = interpret_tool_calls(response).unwrap();
        assert_eq!(result.tags, vec!["test"]);
    }
}
