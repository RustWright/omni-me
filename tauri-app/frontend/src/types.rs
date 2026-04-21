use serde::{Deserialize, Serialize};

/// A journal entry (one per day). Mirrors `JournalEntryRow` from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntryItem {
    /// The date this journal is keyed by (YYYY-MM-DD). Also the SurrealDB record id.
    pub id: String,
    pub journal_id: String,
    pub date: String,
    pub raw_text: String,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub closed: bool,
    pub complete: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A free-form (generic) note. Mirrors `GenericNoteRow` from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericNoteItem {
    pub id: String,
    pub title: String,
    pub raw_text: String,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine group. Mirrors `RoutineGroupRow` from the backend.
///
/// Phase 0 dropped `time_of_day` and introduced `order` + a `removed` flag
/// (soft-delete). The frontend filters removed groups out of the default list view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineGroup {
    pub id: String,
    pub name: String,
    pub frequency: String,
    #[serde(default)]
    pub order_num: i64,
    #[serde(default)]
    pub removed: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A routine item. Mirrors `RoutineItemRow` from the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineItem {
    pub id: String,
    pub group_id: String,
    pub name: String,
    pub estimated_duration_min: i64,
    pub order_num: i64,
    #[serde(default)]
    pub removed: bool,
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

/// Result of a manual sync operation.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimezoneInfo {
    pub timezone: String,
    pub is_override: bool,
}

/// LLM processing result from the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmResult {
    pub tags: Vec<String>,
    pub tasks: Vec<TaskResult>,
    pub dates: Vec<DateResult>,
    pub expenses: Vec<ExpenseResult>,
    pub summary: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
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

/// 4-state sync status reported by the background debouncer/retry loop.
/// Matches `SyncStatus` exposed by the Phase 2 `get_sync_status` Tauri command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SyncState {
    #[default]
    Idle,
    Syncing,
    Retrying,
    Error,
}

/// Mirrors `core::sync::SyncStatusSnapshot` — the full payload returned by
/// the `get_sync_status` Tauri command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatusSnapshot {
    pub status: SyncState,
    pub retry_attempt: u32,
    pub last_error: Option<String>,
}

impl Default for SyncStatusSnapshot {
    fn default() -> Self {
        Self {
            status: SyncState::Idle,
            retry_attempt: 0,
            last_error: None,
        }
    }
}
