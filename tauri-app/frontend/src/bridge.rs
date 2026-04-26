use wasm_bindgen::prelude::*;

#[cfg(feature = "mock")]
use crate::types::TaskResult;
use crate::types::{
    CompletionEntry, GenericNoteItem, JournalEntryItem, LlmResult, RoutineGroup, RoutineItem,
    SyncInfo, SyncStatus, SyncStatusSnapshot, TimezoneInfo,
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
