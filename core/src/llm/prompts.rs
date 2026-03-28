use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::client::LlmError;

/// A versioned prompt template with simple `{{variable}}` substitution.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub name: &'static str,
    pub version: &'static str,
    pub template: &'static str,
    pub description: &'static str,
}

/// Metadata about an LLM call for audit/debugging purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallMetadata {
    pub prompt_name: String,
    pub prompt_version: String,
    pub model: String,
    pub timestamp: chrono::DateTime<Utc>,
}

/// Registry of named prompt templates.
pub struct PromptRegistry {
    templates: HashMap<&'static str, PromptTemplate>,
}

impl PromptRegistry {
    /// Create a new registry populated with all built-in templates.
    pub fn new() -> Self {
        let mut templates = HashMap::new();

        templates.insert(
            NOTE_PROCESS_V1.name,
            NOTE_PROCESS_V1.clone(),
        );

        Self { templates }
    }

    /// Look up a template by name.
    pub fn get(&self, name: &str) -> Option<&PromptTemplate> {
        self.templates.get(name)
    }

    /// Render a template by substituting `{{key}}` placeholders with values
    /// from the provided JSON context object.
    ///
    /// Context must be a JSON object. Each key in the object maps to a
    /// `{{key}}` placeholder in the template. Values are converted to strings
    /// (arrays/objects become JSON strings, scalars use their display form).
    pub fn render(&self, name: &str, context: &serde_json::Value) -> Result<String, LlmError> {
        let template = self
            .get(name)
            .ok_or_else(|| LlmError::ParseError(format!("Unknown template: {name}")))?;

        let ctx_map = context.as_object().ok_or_else(|| {
            LlmError::ParseError("Render context must be a JSON object".to_string())
        })?;

        let mut result = template.template.to_string();
        for (key, value) in ctx_map {
            let placeholder = format!("{{{{{key}}}}}");
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => "".to_string(),
                other => other.to_string(),
            };
            result = result.replace(&placeholder, &replacement);
        }

        Ok(result)
    }
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in prompt template for processing journal notes.
pub static NOTE_PROCESS_V1: PromptTemplate = PromptTemplate {
    name: "note_process_v1",
    version: "1.0.0",
    template: "You are a personal life assistant. Analyze the following journal entry and extract: \
1) tags (list of relevant topics), 2) mood (single word), 3) summary (1-2 sentences), \
4) any actionable tasks mentioned. Use the provided tools to structure your response.\n\n\
Pre-processed data:\n\
URLs: {{urls}}\n\
Dates: {{dates}}\n\
Amounts: {{amounts}}\n\n\
Journal entry:\n\
{{raw_text}}",
    description: "Process a journal entry to extract tags, mood, summary, and tasks",
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_registry_has_note_process_v1() {
        let registry = PromptRegistry::new();
        let tmpl = registry.get("note_process_v1");
        assert!(tmpl.is_some());
        assert_eq!(tmpl.unwrap().version, "1.0.0");
    }

    #[test]
    fn test_registry_unknown_template() {
        let registry = PromptRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_render_basic_substitution() {
        let registry = PromptRegistry::new();
        let context = json!({
            "urls": "https://example.com",
            "dates": "2026-03-27",
            "amounts": "$15.00",
            "raw_text": "Had a great day!"
        });
        let rendered = registry.render("note_process_v1", &context).unwrap();
        assert!(rendered.contains("https://example.com"));
        assert!(rendered.contains("2026-03-27"));
        assert!(rendered.contains("$15.00"));
        assert!(rendered.contains("Had a great day!"));
        // Placeholders should be gone
        assert!(!rendered.contains("{{urls}}"));
        assert!(!rendered.contains("{{raw_text}}"));
    }

    #[test]
    fn test_render_with_array_values() {
        let registry = PromptRegistry::new();
        let context = json!({
            "urls": ["https://a.com", "https://b.com"],
            "dates": ["2026-03-27"],
            "amounts": [],
            "raw_text": "Test entry"
        });
        let rendered = registry.render("note_process_v1", &context).unwrap();
        // Arrays should be serialized as JSON strings
        assert!(rendered.contains("https://a.com"));
        assert!(rendered.contains("https://b.com"));
    }

    #[test]
    fn test_render_unknown_template_error() {
        let registry = PromptRegistry::new();
        let context = json!({});
        let result = registry.render("nonexistent", &context);
        assert!(result.is_err());
    }

    #[test]
    fn test_render_non_object_context_error() {
        let registry = PromptRegistry::new();
        let context = json!("not an object");
        let result = registry.render("note_process_v1", &context);
        assert!(result.is_err());
    }

    #[test]
    fn test_call_metadata_serialization() {
        let meta = CallMetadata {
            prompt_name: "note_process_v1".to_string(),
            prompt_version: "1.0.0".to_string(),
            model: "gemini-2.0-flash".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let deser: CallMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.prompt_name, "note_process_v1");
        assert_eq!(deser.prompt_version, "1.0.0");
        assert_eq!(deser.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_prompt_template_fields() {
        assert_eq!(NOTE_PROCESS_V1.name, "note_process_v1");
        assert_eq!(NOTE_PROCESS_V1.version, "1.0.0");
        assert!(NOTE_PROCESS_V1.template.contains("personal life assistant"));
        assert!(NOTE_PROCESS_V1.description.contains("journal entry"));
    }

    #[test]
    fn test_render_null_value_becomes_empty() {
        let registry = PromptRegistry::new();
        let context = json!({
            "urls": null,
            "dates": null,
            "amounts": null,
            "raw_text": "Hello"
        });
        let rendered = registry.render("note_process_v1", &context).unwrap();
        assert!(rendered.contains("URLs: \n"));
        assert!(rendered.contains("Hello"));
    }
}
