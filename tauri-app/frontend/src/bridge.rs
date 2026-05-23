use wasm_bindgen::prelude::*;

#[cfg(feature = "mock")]
use crate::types::{
    AttachmentRef, CommodityBalanceView, ExtractedPostingView, MonthlyTrendBucketView,
    RecurringObligationView, TaskResult,
};
use crate::types::{
    AccountSummaryView, AffordVerdictView, AutoImportSourceView, CommitBatchResult, CompletionEntry,
    DashboardSummaryView, ExtractedDraft, GenericNoteItem, JournalEntryItem, LlmResult,
    PendingBatchView, PendingShareCapture, RoutineGroup, RoutineItem, SyncInfo, SyncStatus,
    SyncStatusSnapshot, TimezoneInfo, TransactionFormDraft, TransactionView, TxnFilter,
};

// Tauri IPC
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    pub fn tauri_invoke(cmd: &str, args: JsValue) -> js_sys::Promise;
}

// CodeMirror interop
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = createEditor)]
    pub fn js_create_editor(
        element_id: &str,
        initial_content: &str,
        on_change: Option<&js_sys::Function>,
        options: JsValue,
    );
    #[wasm_bindgen(js_name = getEditorContent)]
    pub fn js_get_editor_content() -> String;
    #[wasm_bindgen(js_name = setEditorContent)]
    pub fn js_set_editor_content(content: &str);
    #[wasm_bindgen(js_name = destroyEditor)]
    pub fn js_destroy_editor();
    #[wasm_bindgen(js_name = markClean)]
    pub fn js_mark_editor_clean();
}

// --- Internal Invoke Helpers ---

#[cfg(not(feature = "mock"))]
async fn invoke<T: serde::de::DeserializeOwned>(
    cmd: &str,
    args: &impl serde::Serialize,
) -> Result<T, String> {
    let args_js = serde_wasm_bindgen::to_value(args).map_err(|e| format!("serialize args: {e}"))?;
    let promise = tauri_invoke(cmd, args_js);
    let result = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("{e:?}"))?;
    serde_wasm_bindgen::from_value(result).map_err(|e| format!("deserialize result: {e}"))
}

#[cfg(not(feature = "mock"))]
async fn invoke_unit(cmd: &str, args: &impl serde::Serialize) -> Result<(), String> {
    let args_js = serde_wasm_bindgen::to_value(args).map_err(|e| format!("serialize args: {e}"))?;
    let promise = tauri_invoke(cmd, args_js);
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Journal entries (date-keyed, one-per-day)
// -----------------------------------------------------------------------------

pub async fn invoke_create_journal_entry(
    date: &str,
    raw_text: &str,
) -> Result<JournalEntryItem, String> {
    #[cfg(feature = "mock")]
    {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(JournalEntryItem {
            id: date.to_string(),
            journal_id: format!("mock-journal-{date}"),
            date: date.to_string(),
            raw_text: raw_text.to_string(),
            tags: vec![],
            summary: None,
            closed: false,
            complete: false,
            created_at: now.clone(),
            updated_at: now,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            date: &'a str,
            raw_text: &'a str,
            legacy_properties: Option<serde_json::Value>,
        }
        invoke(
            "create_journal_entry",
            &Args {
                date,
                raw_text,
                legacy_properties: None,
            },
        )
        .await
    }
}

pub async fn invoke_update_journal_entry(journal_id: &str, raw_text: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (journal_id, raw_text);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            journal_id: &'a str,
            raw_text: &'a str,
        }
        invoke_unit(
            "update_journal_entry",
            &Args {
                journal_id,
                raw_text,
            },
        )
        .await
    }
}

pub async fn invoke_close_journal_entry(journal_id: &str, trigger: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (journal_id, trigger);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            journal_id: &'a str,
            trigger: &'a str,
        }
        invoke_unit(
            "close_journal_entry",
            &Args {
                journal_id,
                trigger,
            },
        )
        .await
    }
}

pub async fn invoke_reopen_journal_entry(journal_id: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = journal_id;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            journal_id: &'a str,
        }
        invoke_unit("reopen_journal_entry", &Args { journal_id }).await
    }
}

pub async fn invoke_get_journal_by_date(date: &str) -> Result<Option<JournalEntryItem>, String> {
    #[cfg(feature = "mock")]
    {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        if date == today {
            let now = chrono::Utc::now().to_rfc3339();
            Ok(Some(JournalEntryItem {
                id: today.clone(),
                journal_id: "mock-journal-today".to_string(),
                date: today,
                raw_text: "# Today\n\nFeeling productive. Working on the Blue Topaz UI shell."
                    .to_string(),
                tags: vec!["journal".to_string()],
                summary: None,
                closed: false,
                complete: false,
                created_at: now.clone(),
                updated_at: now,
            }))
        } else {
            Ok(None)
        }
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            date: &'a str,
        }
        invoke("get_journal_by_date", &Args { date }).await
    }
}

pub async fn invoke_list_journal_dates(
    from_date: &str,
    to_date: &str,
) -> Result<Vec<String>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = (from_date, to_date);
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        Ok(vec![today])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            from_date: &'a str,
            to_date: &'a str,
        }
        invoke("list_journal_dates", &Args { from_date, to_date }).await
    }
}

// -----------------------------------------------------------------------------
// Generic notes (id-keyed, user-titled, free-form)
// -----------------------------------------------------------------------------

pub async fn invoke_create_generic_note(
    title: &str,
    raw_text: &str,
) -> Result<GenericNoteItem, String> {
    #[cfg(feature = "mock")]
    {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(GenericNoteItem {
            id: "mock-new".to_string(),
            title: title.to_string(),
            raw_text: raw_text.to_string(),
            tags: vec![],
            summary: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            title: &'a str,
            raw_text: &'a str,
            legacy_properties: Option<serde_json::Value>,
        }
        invoke(
            "create_generic_note",
            &Args {
                title,
                raw_text,
                legacy_properties: None,
            },
        )
        .await
    }
}

pub async fn invoke_update_generic_note(note_id: &str, raw_text: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (note_id, raw_text);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            note_id: &'a str,
            raw_text: &'a str,
        }
        invoke_unit("update_generic_note", &Args { note_id, raw_text }).await
    }
}

pub async fn invoke_rename_generic_note(note_id: &str, title: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (note_id, title);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            note_id: &'a str,
            title: &'a str,
        }
        invoke_unit("rename_generic_note", &Args { note_id, title }).await
    }
}

pub async fn invoke_get_generic_note(id: &str) -> Result<GenericNoteItem, String> {
    #[cfg(feature = "mock")]
    {
        let _ = id;
        let now = chrono::Utc::now().to_rfc3339();
        Ok(GenericNoteItem {
            id: id.to_string(),
            title: "Mock Note".to_string(),
            raw_text: "# Mock\n\nFetched by id.".to_string(),
            tags: vec![],
            summary: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            id: &'a str,
        }
        invoke("get_generic_note", &Args { id }).await
    }
}

pub async fn invoke_list_generic_notes() -> Result<Vec<GenericNoteItem>, String> {
    #[cfg(feature = "mock")]
    {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(vec![
            GenericNoteItem {
                id: "n1".into(),
                title: "UI Workflow".into(),
                raw_text: "## Workflow Notes\n- dx serve + Playwright MCP\n- mock feature flag"
                    .into(),
                tags: vec!["process".into()],
                summary: Some("UI development workflow for Cycle 2.".into()),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            GenericNoteItem {
                id: "n2".into(),
                title: "Meeting Ideas".into(),
                raw_text: "Random thoughts captured during standup.".into(),
                tags: vec![],
                summary: None,
                created_at: now.clone(),
                updated_at: now,
            },
        ])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("list_generic_notes", &Args {}).await
    }
}

pub async fn invoke_search_generic_notes(query: &str) -> Result<Vec<GenericNoteItem>, String> {
    #[cfg(feature = "mock")]
    {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let q = query.to_lowercase();
        let notes = invoke_list_generic_notes().await?;
        Ok(notes
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&q) || n.raw_text.to_lowercase().contains(&q)
            })
            .collect())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            query: &'a str,
        }
        invoke("search_generic_notes", &Args { query }).await
    }
}

// -----------------------------------------------------------------------------
// LLM processing (routes via aggregate_id — works for either journal or generic)
// -----------------------------------------------------------------------------

pub async fn invoke_process_note_llm(aggregate_id: &str) -> Result<LlmResult, String> {
    #[cfg(feature = "mock")]
    {
        let _ = aggregate_id;
        Ok(LlmResult {
            tags: vec!["derived-tag".into(), "ai-insight".into()],
            tasks: vec![TaskResult {
                description: "This is a mock task from AI".into(),
                priority: "high".into(),
            }],
            dates: vec![],
            expenses: vec![],
            summary: Some("This is a mock AI summary of your note.".into()),
            warnings: vec![],
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            aggregate_id: &'a str,
        }
        invoke("process_note_llm", &Args { aggregate_id }).await
    }
}

// -----------------------------------------------------------------------------
// Routine groups
// -----------------------------------------------------------------------------

pub async fn invoke_list_routine_groups() -> Result<Vec<RoutineGroup>, String> {
    #[cfg(feature = "mock")]
    {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(vec![
            RoutineGroup {
                id: "rg1".into(),
                name: "Morning Ritual".into(),
                frequency: "daily".into(),
                order_num: 0,
                removed: false,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            RoutineGroup {
                id: "rg2".into(),
                name: "Weekly Review".into(),
                frequency: "weekly".into(),
                order_num: 1,
                removed: false,
                created_at: now.clone(),
                updated_at: now,
            },
        ])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("list_routine_groups", &Args {}).await
    }
}

pub async fn invoke_create_routine_group(
    name: &str,
    frequency: &str,
    order: u32,
) -> Result<RoutineGroup, String> {
    #[cfg(feature = "mock")]
    {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(RoutineGroup {
            id: "new-mock".into(),
            name: name.into(),
            frequency: frequency.into(),
            order_num: order as i64,
            removed: false,
            created_at: now.clone(),
            updated_at: now,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            name: &'a str,
            frequency: &'a str,
            order: u32,
        }
        invoke(
            "create_routine_group",
            &Args {
                name,
                frequency,
                order,
            },
        )
        .await
    }
}

pub async fn invoke_reorder_routine_groups(orderings: &serde_json::Value) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = orderings;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            orderings: &'a serde_json::Value,
        }
        invoke_unit("reorder_routine_groups", &Args { orderings }).await
    }
}

pub async fn invoke_remove_routine_group(group_id: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = group_id;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            group_id: &'a str,
        }
        invoke_unit("remove_routine_group", &Args { group_id }).await
    }
}

// -----------------------------------------------------------------------------
// Routine items
// -----------------------------------------------------------------------------

pub async fn invoke_list_routine_items(group_id: &str) -> Result<Vec<RoutineItem>, String> {
    #[cfg(feature = "mock")]
    {
        let gid = group_id.to_string();
        Ok(vec![
            RoutineItem {
                id: "i1".into(),
                group_id: gid.clone(),
                name: "Glass of water".into(),
                estimated_duration_min: 1,
                order_num: 0,
                removed: false,
            },
            RoutineItem {
                id: "i2".into(),
                group_id: gid,
                name: "Meditation".into(),
                estimated_duration_min: 10,
                order_num: 1,
                removed: false,
            },
        ])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            group_id: &'a str,
        }
        invoke("list_routine_items", &Args { group_id }).await
    }
}

pub async fn invoke_add_routine_item(
    group_id: &str,
    name: &str,
    duration_min: u32,
    order: u32,
) -> Result<RoutineItem, String> {
    #[cfg(feature = "mock")]
    {
        Ok(RoutineItem {
            id: "new-item".into(),
            group_id: group_id.to_string(),
            name: name.into(),
            estimated_duration_min: duration_min as i64,
            order_num: order as i64,
            removed: false,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            group_id: &'a str,
            name: &'a str,
            duration_min: u32,
            order: u32,
        }
        invoke(
            "add_routine_item",
            &Args {
                group_id,
                name,
                duration_min,
                order,
            },
        )
        .await
    }
}

pub async fn invoke_modify_routine_item(
    item_id: &str,
    changes: &serde_json::Value,
) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (item_id, changes);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            item_id: &'a str,
            changes: &'a serde_json::Value,
        }
        invoke_unit("modify_routine_item", &Args { item_id, changes }).await
    }
}

pub async fn invoke_remove_routine_item(item_id: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = item_id;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            item_id: &'a str,
        }
        invoke_unit("remove_routine_item", &Args { item_id }).await
    }
}

// -----------------------------------------------------------------------------
// Completions and undos
// -----------------------------------------------------------------------------

pub async fn invoke_complete_routine_item(
    item_id: &str,
    group_id: &str,
    date: &str,
) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (item_id, group_id, date);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            item_id: &'a str,
            group_id: &'a str,
            date: &'a str,
        }
        invoke_unit(
            "complete_routine_item",
            &Args {
                item_id,
                group_id,
                date,
            },
        )
        .await
    }
}

pub async fn invoke_undo_completion(item_id: &str, date: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (item_id, date);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            item_id: &'a str,
            date: &'a str,
        }
        invoke_unit("undo_completion", &Args { item_id, date }).await
    }
}

pub async fn invoke_skip_routine_item(
    item_id: &str,
    group_id: &str,
    date: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (item_id, group_id, date, reason);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            item_id: &'a str,
            group_id: &'a str,
            date: &'a str,
            reason: Option<&'a str>,
        }
        invoke_unit(
            "skip_routine_item",
            &Args {
                item_id,
                group_id,
                date,
                reason,
            },
        )
        .await
    }
}

pub async fn invoke_undo_skip(item_id: &str, date: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (item_id, date);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            item_id: &'a str,
            date: &'a str,
        }
        invoke_unit("undo_skip", &Args { item_id, date }).await
    }
}

pub async fn invoke_get_completions_for_date(
    group_id: &str,
    date: &str,
) -> Result<Vec<CompletionEntry>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = (group_id, date);
        Ok(vec![CompletionEntry {
            id: "c1".into(),
            item_id: "i1".into(),
            group_id: group_id.to_string(),
            date: date.to_string(),
            skipped: false,
            reason: None,
        }])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            group_id: &'a str,
            date: &'a str,
        }
        invoke("get_completions_for_date", &Args { group_id, date }).await
    }
}

pub async fn invoke_get_routine_history(
    group_id: &str,
    days: u32,
) -> Result<Vec<CompletionEntry>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = (group_id, days);
        Ok(vec![])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            group_id: &'a str,
            days: u32,
        }
        invoke("get_routine_history", &Args { group_id, days }).await
    }
}

// -----------------------------------------------------------------------------
// Destructive wipe
// -----------------------------------------------------------------------------

pub async fn invoke_wipe_all_data(confirmation: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = confirmation;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            confirmation: &'a str,
        }
        invoke_unit("wipe_all_data", &Args { confirmation }).await
    }
}

// -----------------------------------------------------------------------------
// Sync and settings
// -----------------------------------------------------------------------------

pub async fn invoke_get_sync_info() -> Result<SyncInfo, String> {
    #[cfg(feature = "mock")]
    {
        Ok(SyncInfo {
            server_url: "https://mock-vps.omni-me.com".into(),
            device_id: "SAMSUNG-GALAXY-S21-MOCK".into(),
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("get_sync_info", &Args {}).await
    }
}

pub async fn invoke_trigger_sync() -> Result<SyncStatus, String> {
    #[cfg(feature = "mock")]
    {
        Ok(SyncStatus {
            pulled: 5,
            pushed: 2,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("trigger_sync", &Args {}).await
    }
}

pub async fn invoke_update_server_url(server_url: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = server_url;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            server_url: &'a str,
        }
        invoke_unit("update_server_url", &Args { server_url }).await
    }
}

/// Track D (Phase 2.6) background sync status — returns the full
/// `SyncStatusSnapshot` as surfaced by `core::sync::StatusReporter`.
/// Mock path returns the default (Idle, 0 retries, no error).
pub async fn invoke_get_sync_status() -> Result<SyncStatusSnapshot, String> {
    #[cfg(feature = "mock")]
    {
        Ok(SyncStatusSnapshot::default())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("get_sync_status", &Args {}).await
    }
}

// -----------------------------------------------------------------------------
// Timezone
// -----------------------------------------------------------------------------

pub async fn invoke_get_timezone() -> Result<TimezoneInfo, String> {
    #[cfg(feature = "mock")]
    {
        Ok(TimezoneInfo {
            timezone: "UTC".to_string(),
            is_override: false,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("get_timezone", &Args {}).await
    }
}

pub async fn invoke_update_timezone(timezone: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = timezone;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            timezone: &'a str,
        }
        invoke_unit("update_timezone", &Args { timezone }).await
    }
}

// -----------------------------------------------------------------------------
// Obsidian import / export
// -----------------------------------------------------------------------------

use crate::types::{
    AcceptedImportRow, ExportSummary, ImportCommitSummary, ImportPreviewSummary,
};

pub async fn invoke_preview_import(root: &str) -> Result<ImportPreviewSummary, String> {
    #[cfg(feature = "mock")]
    {
        use crate::types::ImportPreviewRow;
        let _ = root;
        Ok(ImportPreviewSummary {
            root: root.to_string(),
            rows: vec![
                ImportPreviewRow {
                    path: "/mock/vault/Daily/2026-04-21.md".into(),
                    relative_path: "Daily/2026-04-21.md".into(),
                    kind: "journal".into(),
                    key: "2026-04-21".into(),
                    tags: vec!["daily_note".into()],
                    body_preview: "yesterday was a great day".into(),
                    body_len: 128,
                    has_legacy_properties: false,
                    error: None,
                },
                ImportPreviewRow {
                    path: "/mock/vault/Notes/Ideas.md".into(),
                    relative_path: "Notes/Ideas.md".into(),
                    kind: "generic".into(),
                    key: "Ideas".into(),
                    tags: vec!["brainstorm".into()],
                    body_preview: "ideas worth trying".into(),
                    body_len: 200,
                    has_legacy_properties: true,
                    error: None,
                },
            ],
            journal_count: 1,
            generic_count: 1,
            error_count: 0,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            root: &'a str,
        }
        invoke("preview_import", &Args { root }).await
    }
}

pub async fn invoke_commit_import(
    rows: Vec<AcceptedImportRow>,
) -> Result<ImportCommitSummary, String> {
    #[cfg(feature = "mock")]
    {
        let journal_created = rows.iter().filter(|r| r.kind == "journal").count();
        let generic_created = rows.iter().filter(|r| r.kind == "generic").count();
        Ok(ImportCommitSummary {
            journal_created,
            generic_created,
            errors: vec![],
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {
            rows: Vec<AcceptedImportRow>,
        }
        invoke("commit_import", &Args { rows }).await
    }
}

pub async fn invoke_export_obsidian(target: &str) -> Result<ExportSummary, String> {
    #[cfg(feature = "mock")]
    {
        Ok(ExportSummary {
            target: target.to_string(),
            journal_written: 42,
            generic_written: 17,
            errors: vec![],
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            target: &'a str,
        }
        invoke("export_obsidian", &Args { target }).await
    }
}

// -----------------------------------------------------------------------------
// Capture / document extraction (Phase 3.1+)
// -----------------------------------------------------------------------------

/// Send a captured document (photo / PDF / email body bytes) through to the
/// server-side extractor. `hint` mirrors `core::extraction::ExtractionHint`
/// serialised snake_case (`"receipt"`, `"bank_statement"`, ...).
///
/// The mock branch fakes a ~1.2s round trip + returns a canned receipt so
/// `dx serve --features mock` flows end-to-end without a backend.
pub async fn invoke_extract_document(
    bytes: Vec<u8>,
    mime: &str,
    hint: &str,
) -> Result<ExtractedDraft, String> {
    #[cfg(feature = "mock")]
    {
        let size = bytes.len() as u64;
        let _ = (bytes, hint);
        // Simulate network + LLM latency so the UI's wait state is visible.
        crate::timer::sleep_ms(1200).await;
        Ok(ExtractedDraft {
            date: Some("2026-05-17".into()),
            description: Some("Loblaws — Groceries".into()),
            postings: vec![
                ExtractedPostingView {
                    account_hint: Some("Expenses:Groceries".into()),
                    commodity: "CAD".into(),
                    amount: "42.18".into(),
                    line_label: Some("Subtotal".into()),
                },
                ExtractedPostingView {
                    account_hint: Some("Assets:Wealthsimple:Cash".into()),
                    commodity: "CAD".into(),
                    amount: "-42.18".into(),
                    line_label: None,
                },
            ],
            total: Some("42.18".into()),
            confidence: 0.91,
            model: "mock-extractor".into(),
            attachment: Some(AttachmentRef {
                sha256: "0".repeat(64),
                filename: "mock-receipt".into(),
                mime_type: mime.to_string(),
                size,
            }),
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            bytes: Vec<u8>,
            mime: &'a str,
            hint: &'a str,
        }
        invoke("extract_document", &Args { bytes, mime, hint }).await
    }
}

/// Persist a manually-entered or confirmed-draft transaction by appending a
/// `TransactionRecorded` event. The backend returns the projected
/// `TransactionRow`; the frontend doesn't need it (Phase 4 list will reload
/// independently), so we discard and only surface success/failure.
pub async fn invoke_record_transaction(draft: TransactionFormDraft) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        // Fake a short round-trip so the UI's saving state is visible.
        let _ = draft;
        crate::timer::sleep_ms(400).await;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {
            draft: TransactionFormDraft,
        }
        invoke_unit("record_transaction", &Args { draft }).await
    }
}

/// Mock fixture set used by every Phase-4 read path under `--features mock`.
/// Four transactions covering the visual variety the list/detail screens need
/// to render meaningfully: mixed categories, mixed tags (with `food` and
/// `recurring` each appearing twice so tag filters return >1 row), one
/// uncategorized transfer (so empty-category state is visible), one image
/// attachment (whose bytes are served by `invoke_fetch_attachment` below).
#[cfg(feature = "mock")]
const LOBLAWS_RECEIPT_SHA256: &str =
    "0000000000000000000000000000000000000000000000000000000000000001";

#[cfg(feature = "mock")]
const LOBLAWS_RECEIPT_BYTES: &[u8] = include_bytes!("mocks/receipt-loblaws.png");

#[cfg(feature = "mock")]
fn mock_transactions() -> Vec<TransactionView> {
    fn posting(account: &str, amount: &str, commodity: &str) -> serde_json::Value {
        serde_json::json!({
            "account": account,
            "amount": amount,
            "commodity": commodity,
            "tags": [],
        })
    }
    fn attachment(filename: &str, mime: &str, size: u64) -> serde_json::Value {
        serde_json::json!({
            "sha256": LOBLAWS_RECEIPT_SHA256,
            "filename": filename,
            "mime_type": mime,
            "size": size,
        })
    }

    vec![
        TransactionView {
            id: "mock-loblaws".into(),
            date: "2026-05-18".into(),
            description: "Loblaws — weekly groceries".into(),
            postings: serde_json::Value::Array(vec![
                posting("Expenses:Groceries", "42.18", "CAD"),
                posting("Assets:Wealthsimple:Cash", "-42.18", "CAD"),
            ]),
            attachment: Some(attachment(
                "loblaws-2026-05-18.png",
                "image/png",
                LOBLAWS_RECEIPT_BYTES.len() as u64,
            )),
            category: Some("groceries".into()),
            tags_top: vec!["food".into(), "recurring".into()],
            cleared: true,
            statement_source: Some("gmail_personal".into()),
            cleared_date: Some("2026-05-19".into()),
        },
        TransactionView {
            id: "mock-wholefoods".into(),
            date: "2026-05-15".into(),
            description: "Whole Foods — produce run".into(),
            postings: serde_json::Value::Array(vec![
                posting("Expenses:Groceries", "31.04", "CAD"),
                posting("Liabilities:Visa", "-31.04", "CAD"),
            ]),
            attachment: None,
            category: Some("groceries".into()),
            tags_top: vec!["food".into()],
            cleared: true,
            statement_source: Some("cibc_statement".into()),
            cleared_date: Some("2026-05-16".into()),
        },
        TransactionView {
            id: "mock-indigo".into(),
            date: "2026-05-14".into(),
            description: "Indigo — Rust for Rustaceans".into(),
            postings: serde_json::Value::Array(vec![
                posting("Expenses:Books", "57.49", "CAD"),
                posting("Liabilities:Visa", "-57.49", "CAD"),
            ]),
            attachment: None,
            category: Some("books".into()),
            tags_top: vec!["learning".into()],
            cleared: false,
            statement_source: None,
            cleared_date: None,
        },
        TransactionView {
            id: "mock-spotify".into(),
            date: "2026-05-10".into(),
            description: "Spotify Premium".into(),
            postings: serde_json::Value::Array(vec![
                posting("Expenses:Subscriptions", "11.30", "CAD"),
                posting("Liabilities:Visa", "-11.30", "CAD"),
            ]),
            attachment: None,
            category: None,
            tags_top: vec!["recurring".into()],
            cleared: true,
            statement_source: Some("cibc_statement".into()),
            cleared_date: Some("2026-05-11".into()),
        },
    ]
}

/// Mirror of the backend's WHERE-clause behavior: AND across set axes;
/// account match is case-insensitive substring; tag/category are exact;
/// date_from/date_to are inclusive ISO-string compares.
#[cfg(feature = "mock")]
fn mock_apply_filter(rows: Vec<TransactionView>, filter: &TxnFilter) -> Vec<TransactionView> {
    rows.into_iter()
        .filter(|t| {
            if let Some(df) = filter.date_from.as_deref().filter(|s| !s.is_empty()) {
                if t.date.as_str() < df {
                    return false;
                }
            }
            if let Some(dt) = filter.date_to.as_deref().filter(|s| !s.is_empty()) {
                if t.date.as_str() > dt {
                    return false;
                }
            }
            if let Some(cat) = filter.category.as_deref().filter(|s| !s.is_empty()) {
                if t.category.as_deref() != Some(cat) {
                    return false;
                }
            }
            if let Some(tag) = filter.tag.as_deref().filter(|s| !s.is_empty()) {
                if !t.tags_top.iter().any(|x| x == tag) {
                    return false;
                }
            }
            if let Some(acc) = filter.account.as_deref().filter(|s| !s.is_empty()) {
                let needle = acc.to_lowercase();
                let postings = t.postings.as_array().cloned().unwrap_or_default();
                let any_match = postings.iter().any(|p| {
                    p.get("account")
                        .and_then(|a| a.as_str())
                        .map(|s| s.to_lowercase().contains(&needle))
                        .unwrap_or(false)
                });
                if !any_match {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// List committed transactions, newest first. Hidden rows (`removed=true` or
/// `superseded_by IS NOT NONE`) are filtered server-side. The optional
/// `filter` narrows by date range / account substring / tag / category;
/// blank strings inside are treated as unset.
pub async fn invoke_list_transactions(
    filter: TxnFilter,
    limit: u32,
    offset: u32,
) -> Result<Vec<TransactionView>, String> {
    #[cfg(feature = "mock")]
    {
        let filtered = mock_apply_filter(mock_transactions(), &filter);
        let start = offset as usize;
        let end = (start + limit as usize).min(filtered.len());
        Ok(filtered.into_iter().skip(start).take(end - start).collect())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {
            filter: TxnFilter,
            limit: u32,
            offset: u32,
        }
        invoke(
            "list_transactions",
            &Args {
                filter,
                limit,
                offset,
            },
        )
        .await
    }
}

/// Set (or clear) the LLM-derived category for a transaction. Empty string
/// clears it. Backend appends a `TransactionCategorized` event; the
/// projection writes `category` on the row. Used by inline-edit chips in
/// the Phase 4.1 list and Phase 4.2 detail views.
pub async fn invoke_categorize_transaction(txn_id: &str, category: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (txn_id, category);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            txn_id: &'a str,
            category: &'a str,
        }
        invoke_unit("categorize_transaction", &Args { txn_id, category }).await
    }
}

/// Replace the top-level tag set for a transaction. The full vector is the
/// new state — to add a tag, send `current_tags + [new]`; to remove, send
/// the filtered vector. Backend's `on_transaction_tagged` projection sets
/// `tags_top = $tags`, so this is a complete-replacement op.
pub async fn invoke_tag_transaction(txn_id: &str, tags: Vec<String>) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (txn_id, tags);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            txn_id: &'a str,
            tags: Vec<String>,
        }
        invoke_unit("tag_transaction", &Args { txn_id, tags }).await
    }
}

/// Fetch a single transaction by id. Returns `None` if the row is missing,
/// removed, or has been superseded by a merge (the projection hides those).
pub async fn invoke_get_transaction(txn_id: &str) -> Result<Option<TransactionView>, String> {
    #[cfg(feature = "mock")]
    {
        Ok(mock_transactions().into_iter().find(|t| t.id == txn_id))
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            txn_id: &'a str,
        }
        invoke("get_transaction", &Args { txn_id }).await
    }
}

/// Fetch attachment bytes by content-address. Cache-first inside Tauri;
/// on miss, the backend round-trips to the server's `/blobs/{sha256}` and
/// populates the local LRU. Used by the Phase 4.2 detail-view attachment
/// viewer.
pub async fn invoke_fetch_attachment(sha256: &str) -> Result<Vec<u8>, String> {
    #[cfg(feature = "mock")]
    {
        if sha256 == LOBLAWS_RECEIPT_SHA256 {
            Ok(LOBLAWS_RECEIPT_BYTES.to_vec())
        } else {
            Err(format!("mock attachment not found: {sha256}"))
        }
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            sha256: &'a str,
        }
        invoke("fetch_attachment", &Args { sha256 }).await
    }
}

// -----------------------------------------------------------------------------
// Account summaries (Phase 4.4 — Accounts screen)
// -----------------------------------------------------------------------------

/// Fetch per-account balance summaries for the Accounts screen. Backend reads
/// the local journal in-process via `core::balances::account_summaries`,
/// converts to base currency at the latest available rate, and merges
/// declared-account metadata.
pub async fn invoke_account_summaries(
    base_currency: Option<&str>,
) -> Result<Vec<AccountSummaryView>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = base_currency;
        Ok(mock_account_summaries())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            base_currency: Option<&'a str>,
            as_of: Option<&'a str>,
        }
        invoke(
            "account_summaries",
            &Args {
                base_currency,
                as_of: None,
            },
        )
        .await
    }
}

#[cfg(feature = "mock")]
fn mock_account_summaries() -> Vec<AccountSummaryView> {
    vec![
        AccountSummaryView {
            account: "Assets:Wealthsimple:Cash".into(),
            display_name: Some("Wealthsimple Cash".into()),
            last_reconciled_through: Some("2026-05-15".into()),
            last_statement_balance: Some("4250.00".into()),
            balances: vec![CommodityBalanceView {
                commodity: "CAD".into(),
                quantity: "4287.42".into(),
                value_in_base: Some("4287.42".into()),
            }],
            total_in_base: Some("4287.42".into()),
        },
        AccountSummaryView {
            account: "Assets:Wise:CAD".into(),
            display_name: Some("Wise multi-currency".into()),
            last_reconciled_through: None,
            last_statement_balance: None,
            balances: vec![
                CommodityBalanceView {
                    commodity: "CAD".into(),
                    quantity: "812.50".into(),
                    value_in_base: Some("812.50".into()),
                },
                CommodityBalanceView {
                    commodity: "EUR".into(),
                    quantity: "120.00".into(),
                    value_in_base: Some("180.50".into()),
                },
                CommodityBalanceView {
                    commodity: "USD".into(),
                    quantity: "45.00".into(),
                    value_in_base: Some("61.65".into()),
                },
            ],
            total_in_base: Some("1054.65".into()),
        },
        AccountSummaryView {
            account: "Liabilities:CIBC:CreditCard".into(),
            display_name: Some("CIBC Aventura".into()),
            last_reconciled_through: Some("2026-04-30".into()),
            last_statement_balance: Some("-1182.06".into()),
            balances: vec![CommodityBalanceView {
                commodity: "CAD".into(),
                quantity: "-1450.18".into(),
                value_in_base: Some("-1450.18".into()),
            }],
            total_in_base: Some("-1450.18".into()),
        },
        AccountSummaryView {
            account: "Unmatched".into(),
            display_name: None,
            last_reconciled_through: None,
            last_statement_balance: None,
            balances: vec![CommodityBalanceView {
                commodity: "CAD".into(),
                quantity: "-1.50".into(),
                value_in_base: Some("-1.50".into()),
            }],
            total_in_base: Some("-1.50".into()),
        },
    ]
}

// -----------------------------------------------------------------------------
// Dashboard (Phase 4.5 + 4.6 — R1 financial-health glance)
// -----------------------------------------------------------------------------

pub async fn invoke_dashboard_summary(
    base_currency: Option<&str>,
) -> Result<DashboardSummaryView, String> {
    #[cfg(feature = "mock")]
    {
        let _ = base_currency;
        Ok(mock_dashboard_summary())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            base_currency: Option<&'a str>,
            as_of: Option<&'a str>,
            months_back: Option<u32>,
        }
        invoke(
            "dashboard_summary",
            &Args {
                base_currency,
                as_of: None,
                months_back: None,
            },
        )
        .await
    }
}

pub async fn invoke_check_affordability(
    amount: &str,
    base_currency: Option<&str>,
) -> Result<AffordVerdictView, String> {
    #[cfg(feature = "mock")]
    {
        let _ = base_currency;
        Ok(mock_check_affordability(amount))
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            amount: &'a str,
            base_currency: Option<&'a str>,
            as_of: Option<&'a str>,
            months_back: Option<u32>,
        }
        invoke(
            "check_affordability",
            &Args {
                amount,
                base_currency,
                as_of: None,
                months_back: None,
            },
        )
        .await
    }
}

#[cfg(feature = "mock")]
fn mock_dashboard_summary() -> DashboardSummaryView {
    // Net worth = sum of listable accounts' totals from mock_account_summaries
    // EXCLUDING Unmatched (per dashboard policy): 4287.42 + 1054.65 + -1450.18
    // = 3891.89. Keep these two mocks in sync so dashboard + accounts screens
    // tell the same story in screenshots.
    DashboardSummaryView {
        base_currency: "CAD".into(),
        net_worth_in_base: Some("3891.89".into()),
        unmatched_balance: Some("-1.50".into()),
        monthly_buckets: vec![
            MonthlyTrendBucketView {
                month: "2025-12".into(),
                income: "3200.00".into(),
                spending: "2814.55".into(),
            },
            MonthlyTrendBucketView {
                month: "2026-01".into(),
                income: "3200.00".into(),
                spending: "2940.18".into(),
            },
            MonthlyTrendBucketView {
                month: "2026-02".into(),
                income: "3200.00".into(),
                spending: "2670.42".into(),
            },
            MonthlyTrendBucketView {
                month: "2026-03".into(),
                income: "3450.00".into(),
                spending: "3105.77".into(),
            },
            MonthlyTrendBucketView {
                month: "2026-04".into(),
                income: "3200.00".into(),
                spending: "2890.31".into(),
            },
            MonthlyTrendBucketView {
                month: "2026-05".into(),
                income: "1820.00".into(),
                spending: "1450.18".into(),
            },
        ],
        recurring: vec![
            RecurringObligationView {
                vendor: "Netflix".into(),
                amount: "16.99".into(),
                commodity: "CAD".into(),
                cadence_days: 30,
            },
            RecurringObligationView {
                vendor: "Telus mobile".into(),
                amount: "55.00".into(),
                commodity: "CAD".into(),
                cadence_days: 30,
            },
            RecurringObligationView {
                vendor: "Rent".into(),
                amount: "1850.00".into(),
                commodity: "CAD".into(),
                cadence_days: 30,
            },
        ],
    }
}

#[cfg(feature = "mock")]
fn mock_check_affordability(amount: &str) -> AffordVerdictView {
    // Mirror the conservative-after-recurring policy in the mock so the UI
    // feels right without a real backend round-trip. Net worth literal must
    // match `mock_dashboard_summary`'s `net_worth_in_base` — sum of listable
    // accounts ex-Unmatched per the dashboard policy.
    let amt: f64 = amount.parse().unwrap_or(0.0);
    let net_worth = 3891.89_f64;
    // Mock recurring: 16.99 + 55 + 1850 = 1921.99 (all monthly).
    let recurring = 16.99 + 55.0 + 1850.0;
    let remaining = net_worth - recurring - amt;
    AffordVerdictView {
        can_afford: remaining > 0.0,
        remaining_in_base: format!("{remaining:.2}"),
        base_currency: "CAD".into(),
        policy_label: "Net worth − next month's recurring".into(),
    }
}

// -----------------------------------------------------------------------------
// Attachment cache (Phase 3.8 — surfaces Phase 3.7 cache commands in Settings)
// -----------------------------------------------------------------------------

pub async fn invoke_attachment_cache_size() -> Result<u64, String> {
    #[cfg(feature = "mock")]
    {
        // Plausible "you've captured a few receipts" value.
        Ok(3 * 1024 * 1024 + 412 * 1024)
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("attachment_cache_size", &Args {}).await
    }
}

pub async fn invoke_clear_attachment_cache() -> Result<u64, String> {
    #[cfg(feature = "mock")]
    {
        Ok(3 * 1024 * 1024 + 412 * 1024)
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("clear_attachment_cache", &Args {}).await
    }
}

// -----------------------------------------------------------------------------
// Auto-import observability (Phase 3.9)
// -----------------------------------------------------------------------------

pub async fn invoke_list_auto_import_sources() -> Result<Vec<AutoImportSourceView>, String> {
    #[cfg(feature = "mock")]
    {
        // A plausible four-source picture: healthy Wise, healthy WS, never-run
        // IMAP receipts, and a degraded SC NGN handler. Covers all four
        // health states in one snapshot so the UI can be inspected against
        // each badge color without contriving credentials.
        let now = chrono::Utc::now();
        let two_min_ago = now - chrono::Duration::minutes(2);
        let three_hours_ago = now - chrono::Duration::hours(3);
        Ok(vec![
            AutoImportSourceView {
                name: "wise".into(),
                last_tick_at: Some(two_min_ago.to_rfc3339()),
                last_outcome: serde_json::json!({ "kind": "success", "events_appended": 0 }),
                interval_secs: 1800,
                health: "healthy".into(),
            },
            AutoImportSourceView {
                name: "wealthsimple-snaptrade".into(),
                last_tick_at: Some(three_hours_ago.to_rfc3339()),
                last_outcome: serde_json::json!({ "kind": "success", "events_appended": 12 }),
                interval_secs: 1800,
                health: "stale".into(),
            },
            AutoImportSourceView {
                name: "imap-receipts".into(),
                last_tick_at: None,
                last_outcome: serde_json::json!({ "kind": "not_yet_run" }),
                interval_secs: 1800,
                health: "unknown".into(),
            },
            AutoImportSourceView {
                name: "imap-standardchartered-ngn".into(),
                last_tick_at: Some((now - chrono::Duration::minutes(10)).to_rfc3339()),
                last_outcome: serde_json::json!({
                    "kind": "failure",
                    "error": "decrypt failed: wrong password",
                }),
                interval_secs: 1800,
                health: "degraded".into(),
            },
        ])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("list_auto_import_sources", &Args {}).await
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManualTickResult {
    pub events_appended: usize,
}

pub async fn invoke_trigger_auto_import_tick(source: &str) -> Result<ManualTickResult, String> {
    #[cfg(feature = "mock")]
    {
        let _ = source;
        crate::timer::sleep_ms(900).await;
        Ok(ManualTickResult { events_appended: 3 })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            source: &'a str,
        }
        invoke("trigger_auto_import_tick", &Args { source }).await
    }
}

// -----------------------------------------------------------------------------
// Auto-import batch review (Phase 3.10.5/.6)
// -----------------------------------------------------------------------------

pub async fn invoke_list_pending_batches() -> Result<Vec<PendingBatchView>, String> {
    #[cfg(feature = "mock")]
    {
        use crate::types::{DraftTransactionView, PostingInput};
        // One Wise batch (CAD/USD) + one Standard Chartered NGN batch — the
        // latter has a manual-FX currency so the FX prompt path is exercisable.
        let now = chrono::Utc::now();
        Ok(vec![
            PendingBatchView {
                batch_id: "01HXMOCKWISE000000000001".into(),
                source: "wise".into(),
                dedup_key: "wise-01HXMOCKWISE000000000001".into(),
                fetched_at: (now - chrono::Duration::minutes(4)).to_rfc3339(),
                draft_postings: vec![
                    DraftTransactionView {
                        external_id: "wise-12345".into(),
                        date: "2026-05-17".into(),
                        description: "Wise transfer — landlord".into(),
                        postings: vec![
                            PostingInput {
                                account: "Expenses:Rent".into(),
                                commodity: "CAD".into(),
                                amount: "1850.00".into(),
                                tags: vec![],
                            },
                            PostingInput {
                                account: "Assets:Wise:CAD".into(),
                                commodity: "CAD".into(),
                                amount: "-1850.00".into(),
                                tags: vec![],
                            },
                        ],
                    },
                    DraftTransactionView {
                        external_id: "wise-12346".into(),
                        date: "2026-05-18".into(),
                        description: "Wise FX — USD top-up".into(),
                        postings: vec![
                            PostingInput {
                                account: "Assets:Wise:USD".into(),
                                commodity: "USD".into(),
                                amount: "500.00".into(),
                                tags: vec![],
                            },
                            PostingInput {
                                account: "Assets:Wise:CAD".into(),
                                commodity: "CAD".into(),
                                amount: "-686.42".into(),
                                tags: vec![],
                            },
                        ],
                    },
                ],
                source_metadata: None,
            },
            PendingBatchView {
                batch_id: "01HXMOCKSCNG000000000001".into(),
                source: "sc_ngn".into(),
                dedup_key: "sc_ngn-uid-42".into(),
                fetched_at: (now - chrono::Duration::minutes(11)).to_rfc3339(),
                draft_postings: vec![DraftTransactionView {
                    external_id: "sc-april-statement-row-1".into(),
                    date: "2026-04-29".into(),
                    description: "Standard Chartered Lagos — POS".into(),
                    postings: vec![
                        PostingInput {
                            account: "Expenses:Groceries".into(),
                            commodity: "NGN".into(),
                            amount: "32500".into(),
                            tags: vec![],
                        },
                        PostingInput {
                            account: "Assets:StandardChartered:NGN".into(),
                            commodity: "NGN".into(),
                            amount: "-32500".into(),
                            tags: vec![],
                        },
                    ],
                }],
                source_metadata: Some(serde_json::json!({
                    "from": "statements@sc.com",
                    "subject": "April statement",
                    "uid": 42,
                })),
            },
        ])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("list_pending_batches", &Args {}).await
    }
}

pub async fn invoke_commit_batch(
    batch_id: &str,
    accepted_indices: Vec<usize>,
    fx_rate: Option<String>,
    fx_commodity: Option<String>,
) -> Result<CommitBatchResult, String> {
    #[cfg(feature = "mock")]
    {
        let _ = (batch_id, fx_rate, fx_commodity);
        crate::timer::sleep_ms(450).await;
        Ok(CommitBatchResult {
            events_appended: accepted_indices.len() + 1,
            txns_recorded: accepted_indices.len(),
            fx_recorded: false,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            batch_id: &'a str,
            accepted_indices: Vec<usize>,
            #[serde(skip_serializing_if = "Option::is_none")]
            fx_rate: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            fx_commodity: Option<String>,
        }
        invoke(
            "commit_batch",
            &Args {
                batch_id,
                accepted_indices,
                fx_rate,
                fx_commodity,
            },
        )
        .await
    }
}

pub async fn invoke_dismiss_batch(
    batch_id: &str,
    reason: Option<String>,
) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (batch_id, reason);
        crate::timer::sleep_ms(300).await;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            batch_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            reason: Option<String>,
        }
        invoke_unit("dismiss_batch", &Args { batch_id, reason }).await
    }
}

// -----------------------------------------------------------------------------
// Android share-target intake (Phase 3.3)
// -----------------------------------------------------------------------------

/// Returns the pending shared file if MainActivity.kt has captured one since
/// the last call. Idempotent: each take consumes (and deletes) the underlying
/// side-files, so calling repeatedly during a session is safe.
pub async fn invoke_take_pending_share_intent() -> Result<Option<PendingShareCapture>, String> {
    #[cfg(feature = "mock")]
    {
        // No share intent in browser dev. Returning None lets the regular
        // capture-tile flow take over.
        Ok(None)
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("take_pending_share_intent", &Args {}).await
    }
}
