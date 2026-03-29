use serde::{Deserialize, Serialize};

/// A note as returned from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteListItem {
    pub id: String,
    pub raw_text: String,
    pub date: String,
    pub tags: Vec<String>,
    pub mood: Option<String>,
    pub summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine group as returned from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineGroup {
    pub id: String,
    pub name: String,
    pub frequency: String,
    pub time_of_day: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine item as returned from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineItem {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub estimated_duration_min: i64,
    pub order_num: i64,
}

/// A routine completion entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionEntry {
    pub id: String,
    pub item_id: String,
    pub group_id: String,
    pub date: String,
    pub skipped: bool,
    pub reason: Option<String>,
}

/// Result of a sync operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncStatus {
    pub pulled: usize,
    pub pushed: usize,
}

/// Current sync configuration info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncInfo {
    pub server_url: String,
    pub device_id: String,
}

/// LLM processing result from the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmResult {
    pub tags: Vec<String>,
    pub mood: Option<MoodResult>,
    pub tasks: Vec<TaskResult>,
    pub dates: Vec<DateResult>,
    pub expenses: Vec<ExpenseResult>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoodResult {
    pub mood: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResult {
    pub description: String,
    pub priority: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateResult {
    pub date: String,
    pub context: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpenseResult {
    pub amount: f64,
    pub currency: String,
    pub description: String,
}
