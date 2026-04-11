use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Definition of a tool that the LLM can call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the tool's parameters.
    pub parameters: Value,
}

/// A single tool call made by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

/// Response from an LLM that may be plain text, tool calls, or structured JSON.
#[derive(Debug, Clone)]
pub enum LlmResponse {
    /// Plain text response.
    Text(String),
    /// One or more tool calls requested by the model.
    ToolCalls(Vec<ToolCall>),
    /// Structured JSON response (from complete_json).
    Structured(Value),
}

/// Returns the default set of tool definitions for note processing.
///
/// Includes: `create_tag`, `extract_task`, `extract_date`, `extract_expense`.
pub fn default_note_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "create_tag".to_string(),
            description: "Create a tag for categorizing the note".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tag": {
                        "type": "string",
                        "description": "The tag name to apply"
                    }
                },
                "required": ["tag"]
            }),
        },
        ToolDef {
            name: "extract_task".to_string(),
            description: "Extract an actionable task from the note".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "Description of the task"
                    },
                    "priority": {
                        "type": "string",
                        "enum": ["low", "medium", "high"],
                        "description": "Priority level of the task"
                    }
                },
                "required": ["description", "priority"]
            }),
        },
        ToolDef {
            name: "extract_date".to_string(),
            description: "Extract a date mentioned in the entry. Interpret relative references (tomorrow, next week, last friday) using the entry's context. Call once per distinct date found.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "date": {
                        "type": "string",
                        "description": "The date in YYYY-MM-DD format"
                    },
                    "context": {
                        "type": "string",
                        "description": "Brief description of what this date refers to (e.g. 'dentist appointment', 'entry date')"
                    }
                },
                "required": ["date", "context"]
            }),
        },
        ToolDef {
            name: "extract_expense".to_string(),
            description: "Extract an expense or financial transaction mentioned in the entry. Interpret fuzzy amounts (about 15 bucks, ~R200, ten dollars) as best as possible.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "amount": {
                        "type": "number",
                        "description": "The monetary amount as a number"
                    },
                    "currency": {
                        "type": "string",
                        "description": "Three-letter currency code (e.g. USD, EUR, ZAR, GBP)"
                    },
                    "description": {
                        "type": "string",
                        "description": "What the expense was for"
                    }
                },
                "required": ["amount", "currency", "description"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_note_tools_count() {
        let tools = default_note_tools();
        assert_eq!(tools.len(), 4);
    }

    #[test]
    fn test_default_note_tools_names() {
        let tools = default_note_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["create_tag", "extract_task", "extract_date", "extract_expense"]
        );
    }

    #[test]
    fn test_tool_def_serialization() {
        let tools = default_note_tools();
        let json = serde_json::to_string(&tools[0]).unwrap();
        let deser: ToolDef = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "create_tag");
    }

    #[test]
    fn test_tool_call_serialization() {
        let call = ToolCall {
            name: "create_tag".to_string(),
            arguments: json!({"tag": "personal"}),
        };
        let json = serde_json::to_string(&call).unwrap();
        let deser: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "create_tag");
        assert_eq!(deser.arguments["tag"], "personal");
    }

    #[test]
    fn test_extract_task_schema_has_enum() {
        let tools = default_note_tools();
        let extract_task = &tools[1];
        let priority = &extract_task.parameters["properties"]["priority"];
        let enum_vals = priority["enum"].as_array().unwrap();
        assert_eq!(enum_vals.len(), 3);
    }

}
