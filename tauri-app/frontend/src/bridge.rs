use wasm_bindgen::prelude::*;

#[cfg(feature = "mock")]
use crate::types::{
    AttachmentRef, CommodityBalanceView, ExtractedPostingView, MonthlyTrendBucketView,
    RecurringObligationView, TaskResult,
};
use crate::types::{
    AccountSummaryView, AffordVerdictView, AutoImportSourceView, BalanceCheckView, BudgetProgress,
    BudgetRow, CommitBatchResult, CompletionEntry, DashboardSummaryView, ExtractedDraft,
    GenericNoteItem, ImportStatementCsvResult, JournalEntryItem, LlmResult, MatchCandidateView,
    PendingBatchView, PendingShareCapture, ReconciliationTxnPreview, RecurringPattern,
    RoutineGroup, RoutineItem, ScanRecurringResult, SyncInfo, SyncStatus, SyncStatusSnapshot,
    TimezoneInfo, TransactionFormDraft, TransactionView, TxnFilter,
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

/// Base currency used by dashboard / accounts FX aggregation (Phase 7.3).
pub async fn invoke_get_base_currency() -> Result<String, String> {
    #[cfg(feature = "mock")]
    {
        Ok("CAD".to_string())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("get_base_currency", &Args {}).await
    }
}

pub async fn invoke_update_base_currency(currency: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = currency;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            currency: &'a str,
        }
        invoke_unit("update_base_currency", &Args { currency }).await
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
            if let Some(df) = filter.date_from.as_deref().filter(|s| !s.is_empty())
                && t.date.as_str() < df
            {
                return false;
            }
            if let Some(dt) = filter.date_to.as_deref().filter(|s| !s.is_empty())
                && t.date.as_str() > dt
            {
                return false;
            }
            if let Some(cat) = filter.category.as_deref().filter(|s| !s.is_empty())
                && t.category.as_deref() != Some(cat)
            {
                return false;
            }
            if let Some(tag) = filter.tag.as_deref().filter(|s| !s.is_empty())
                && !t.tags_top.iter().any(|x| x == tag)
            {
                return false;
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

/// R2 ad-hoc query (Phase 7.2): send a DSL string to the host engine and get
/// back the filtered, paginated transaction page. Real evaluation lives in
/// `omni_me_core::query`; the mock branch does a light account/desc substring
/// approximation so `dx serve --features mock` stays interactive without a
/// backend (it intentionally does *not* reproduce segment-prefix / amount / date
/// semantics — those are covered by the core engine's own tests).
pub async fn invoke_run_transaction_query(
    dsl: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<TransactionView>, String> {
    #[cfg(feature = "mock")]
    {
        let filtered = mock_apply_dsl(mock_transactions(), dsl);
        let start = offset as usize;
        let end = (start + limit as usize).min(filtered.len());
        Ok(filtered
            .into_iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            dsl: &'a str,
            limit: u32,
            offset: u32,
        }
        invoke(
            "run_transaction_query",
            &Args { dsl, limit, offset },
        )
        .await
    }
}

/// Mock-only, deliberately loose DSL approximation: handles `account:` and
/// `desc:` substring terms (case-insensitive), ANDs them (or ORs if the query
/// contains a bare `OR`), and ignores every other field. Enough to make the
/// builder feel live under `dx serve --features mock`.
#[cfg(feature = "mock")]
fn mock_apply_dsl(rows: Vec<TransactionView>, dsl: &str) -> Vec<TransactionView> {
    let use_any = dsl.split_whitespace().any(|t| t == "OR");
    let terms: Vec<(String, String)> = dsl
        .split_whitespace()
        .filter(|t| *t != "OR" && *t != "AND")
        .filter_map(|t| t.split_once(':').map(|(f, v)| (f.to_lowercase(), v.to_string())))
        .filter(|(f, _)| matches!(f.as_str(), "account" | "acct" | "desc" | "description"))
        .map(|(f, v)| (f, v.trim_matches('"').trim_end_matches('$').to_lowercase()))
        .collect();
    if terms.is_empty() {
        return rows;
    }
    rows.into_iter()
        .filter(|t| {
            let results: Vec<bool> = terms
                .iter()
                .map(|(field, val)| match field.as_str() {
                    "account" | "acct" => t
                        .postings
                        .as_array()
                        .map(|ps| {
                            ps.iter().any(|p| {
                                p.get("account")
                                    .and_then(|a| a.as_str())
                                    .map(|s| s.to_lowercase().contains(val.as_str()))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false),
                    _ => t.description.to_lowercase().contains(val.as_str()),
                })
                .collect();
            if use_any {
                results.into_iter().any(|x| x)
            } else {
                results.into_iter().all(|x| x)
            }
        })
        .collect()
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
            account: "Assets:StandardChartered:NGN".into(),
            display_name: Some("Standard Chartered NGN".into()),
            last_reconciled_through: None,
            last_statement_balance: None,
            balances: vec![CommodityBalanceView {
                commodity: "NGN".into(),
                quantity: "52400.00".into(),
                value_in_base: None,
            }],
            total_in_base: None,
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

// -----------------------------------------------------------------------------
// Budgets (Phase 5.1) — per-category targets persisted via BudgetSet event.
// -----------------------------------------------------------------------------

pub async fn invoke_list_budgets() -> Result<Vec<BudgetRow>, String> {
    #[cfg(feature = "mock")]
    {
        Ok(mock_budgets())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("list_budgets", &Args {}).await
    }
}

pub async fn invoke_set_budget(
    category: &str,
    amount: &str,
    period: &str,
) -> Result<BudgetRow, String> {
    #[cfg(feature = "mock")]
    {
        Ok(BudgetRow {
            id: category.to_string(),
            amount: amount.to_string(),
            period: period.to_string(),
            removed: false,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            category: &'a str,
            amount: &'a str,
            period: &'a str,
        }
        invoke(
            "set_budget",
            &Args {
                category,
                amount,
                period,
            },
        )
        .await
    }
}

pub async fn invoke_budget_progress(
    base_currency: Option<&str>,
) -> Result<Vec<BudgetProgress>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = base_currency;
        Ok(mock_budget_progress())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            base_currency: Option<&'a str>,
            as_of: Option<&'a str>,
        }
        invoke(
            "budget_progress",
            &Args {
                base_currency,
                as_of: None,
            },
        )
        .await
    }
}

// -----------------------------------------------------------------------------
// Recurring patterns (Phase 5.3 + 5.4) — scan / list / confirm / dismiss.
// -----------------------------------------------------------------------------

pub async fn invoke_scan_recurring(
    lookback_days: Option<u32>,
) -> Result<ScanRecurringResult, String> {
    #[cfg(feature = "mock")]
    {
        let _ = lookback_days;
        Ok(ScanRecurringResult {
            detected: 3,
            new_emitted: 2,
            already_tracked: 1,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {
            lookback_days: Option<u32>,
        }
        invoke("scan_recurring", &Args { lookback_days }).await
    }
}

pub async fn invoke_list_recurring(
    status: Option<&str>,
) -> Result<Vec<RecurringPattern>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = status;
        Ok(mock_recurring_patterns())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            status: Option<&'a str>,
        }
        invoke("list_recurring", &Args { status }).await
    }
}

pub async fn invoke_confirm_recurring(pattern_id: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = pattern_id;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            pattern_id: &'a str,
        }
        invoke_unit("confirm_recurring", &Args { pattern_id }).await
    }
}

// -----------------------------------------------------------------------------
// Statement CSV import (Phase 5.5) — CIBC chequing today, more formats later.
// -----------------------------------------------------------------------------

pub async fn invoke_import_cibc_chequing_csv(
    csv_text: &str,
    source_account: &str,
    statement_source: &str,
    commodity: Option<&str>,
) -> Result<ImportStatementCsvResult, String> {
    #[cfg(feature = "mock")]
    {
        let _ = (csv_text, source_account, statement_source, commodity);
        Ok(ImportStatementCsvResult {
            imported: 3,
            skipped_zero_rows: 0,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            csv_text: &'a str,
            source_account: &'a str,
            statement_source: &'a str,
            commodity: Option<&'a str>,
        }
        invoke(
            "import_cibc_chequing_csv",
            &Args {
                csv_text,
                source_account,
                statement_source,
                commodity,
            },
        )
        .await
    }
}

// -----------------------------------------------------------------------------
// Reconciliation (Phase 5.6 + 5.7) — list candidates, merge a pair.
// -----------------------------------------------------------------------------

pub async fn invoke_list_match_candidates(
    max_days_gap: Option<u32>,
) -> Result<Vec<MatchCandidateView>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = max_days_gap;
        Ok(mock_match_candidates())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {
            max_days_gap: Option<u32>,
        }
        invoke("list_match_candidates", &Args { max_days_gap }).await
    }
}

pub async fn invoke_list_unmatched_without_candidates(
    max_days_gap: Option<u32>,
) -> Result<Vec<ReconciliationTxnPreview>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = max_days_gap;
        Ok(mock_unmatched_no_candidate())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {
            max_days_gap: Option<u32>,
        }
        invoke("list_unmatched_without_candidates", &Args { max_days_gap }).await
    }
}

pub async fn invoke_resolve_unmatched(
    txn_id: &str,
    category: &str,
) -> Result<(), String> {
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
        invoke_unit("resolve_unmatched", &Args { txn_id, category }).await
    }
}

pub async fn invoke_merge_transactions(
    primary_id: &str,
    secondary_id: &str,
) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = (primary_id, secondary_id);
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            primary_id: &'a str,
            secondary_id: &'a str,
        }
        invoke_unit(
            "merge_transactions",
            &Args {
                primary_id,
                secondary_id,
            },
        )
        .await
    }
}

#[cfg(feature = "mock")]
fn mock_unmatched_no_candidate() -> Vec<ReconciliationTxnPreview> {
    vec![
        ReconciliationTxnPreview {
            txn_id: "01JK099".to_string(),
            date: "2026-05-12".to_string(),
            description: "Costco Wholesale".to_string(),
            unmatched_amount: "-185.42".to_string(),
            unmatched_commodity: "CAD".to_string(),
            statement_source: Some("cibc-chequing-2026-05".to_string()),
        },
        ReconciliationTxnPreview {
            txn_id: "01JK100".to_string(),
            date: "2026-05-13".to_string(),
            description: "Etransfer to Jane".to_string(),
            unmatched_amount: "-50.00".to_string(),
            unmatched_commodity: "CAD".to_string(),
            statement_source: Some("cibc-chequing-2026-05".to_string()),
        },
    ]
}

#[cfg(feature = "mock")]
fn mock_match_candidates() -> Vec<MatchCandidateView> {
    vec![
        MatchCandidateView {
            primary_id: "01JK001".to_string(),
            secondary_id: "01JK002".to_string(),
            score: 0.95,
            days_apart: 0,
            description_similarity: 1.0,
            clears_statement: true,
            primary: ReconciliationTxnPreview {
                txn_id: "01JK001".to_string(),
                date: "2026-05-15".to_string(),
                description: "Loblaws Groceries".to_string(),
                unmatched_amount: "42.18".to_string(),
                unmatched_commodity: "CAD".to_string(),
                statement_source: None,
            },
            secondary: ReconciliationTxnPreview {
                txn_id: "01JK002".to_string(),
                date: "2026-05-15".to_string(),
                description: "LOBLAWS".to_string(),
                unmatched_amount: "-42.18".to_string(),
                unmatched_commodity: "CAD".to_string(),
                statement_source: Some("cibc-chequing-2026-05".to_string()),
            },
        },
        MatchCandidateView {
            primary_id: "01JK003".to_string(),
            secondary_id: "01JK004".to_string(),
            score: 0.72,
            days_apart: 3,
            description_similarity: 0.5,
            clears_statement: true,
            primary: ReconciliationTxnPreview {
                txn_id: "01JK003".to_string(),
                date: "2026-05-10".to_string(),
                description: "Hydro Bill".to_string(),
                unmatched_amount: "87.50".to_string(),
                unmatched_commodity: "CAD".to_string(),
                statement_source: None,
            },
            secondary: ReconciliationTxnPreview {
                txn_id: "01JK004".to_string(),
                date: "2026-05-13".to_string(),
                description: "Toronto Hydro".to_string(),
                unmatched_amount: "-87.50".to_string(),
                unmatched_commodity: "CAD".to_string(),
                statement_source: Some("cibc-chequing-2026-05".to_string()),
            },
        },
    ]
}

pub async fn invoke_check_account_balance(
    account: &str,
    commodity: &str,
    statement_balance: &str,
    as_of: Option<&str>,
) -> Result<BalanceCheckView, String> {
    #[cfg(feature = "mock")]
    {
        let _ = (account, commodity, statement_balance, as_of);
        Ok(BalanceCheckView {
            account: account.to_string(),
            commodity: commodity.to_string(),
            cleared_total: "1480.00".to_string(),
            statement_balance: statement_balance.to_string(),
            discrepancy: "-20.00".to_string(),
            ok: false,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            account: &'a str,
            commodity: &'a str,
            statement_balance: &'a str,
            as_of: Option<&'a str>,
        }
        invoke(
            "check_account_balance",
            &Args {
                account,
                commodity,
                statement_balance,
                as_of,
            },
        )
        .await
    }
}

pub async fn invoke_dismiss_recurring(pattern_id: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = pattern_id;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            pattern_id: &'a str,
        }
        invoke_unit("dismiss_recurring", &Args { pattern_id }).await
    }
}

#[cfg(feature = "mock")]
fn mock_recurring_patterns() -> Vec<RecurringPattern> {
    vec![
        RecurringPattern {
            pattern_id: "recurring-aaaaaaaaaaaaaaaa".to_string(),
            status: "detected".to_string(),
            vendor: "Expenses:Netflix".to_string(),
            amount: "15.99".to_string(),
            commodity: "CAD".to_string(),
            cadence_days: 30,
            occurrences: 6,
            first_seen: Some("2025-12-15".to_string()),
            last_seen: Some("2026-05-15".to_string()),
        },
        RecurringPattern {
            pattern_id: "recurring-bbbbbbbbbbbbbbbb".to_string(),
            status: "detected".to_string(),
            vendor: "Expenses:Coffee".to_string(),
            amount: "5.50".to_string(),
            commodity: "CAD".to_string(),
            cadence_days: 7,
            occurrences: 14,
            first_seen: Some("2026-02-01".to_string()),
            last_seen: Some("2026-05-15".to_string()),
        },
    ]
}

pub async fn invoke_remove_budget(category: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = category;
        Ok(())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            category: &'a str,
        }
        invoke_unit("remove_budget", &Args { category }).await
    }
}

#[cfg(feature = "mock")]
fn mock_budget_progress() -> Vec<BudgetProgress> {
    vec![
        BudgetProgress {
            category: "Expenses:Groceries".to_string(),
            period: "monthly".to_string(),
            period_start: "2026-05-01".to_string(),
            period_end: "2026-05-31".to_string(),
            target: "600.00".to_string(),
            actual: "225.00".to_string(),
            percent_used: 37.5,
            over_budget: false,
        },
        BudgetProgress {
            category: "Expenses:DiningOut".to_string(),
            period: "biweekly".to_string(),
            period_start: "2026-05-02".to_string(),
            period_end: "2026-05-15".to_string(),
            target: "120.00".to_string(),
            actual: "145.00".to_string(),
            percent_used: 120.83,
            over_budget: true,
        },
        BudgetProgress {
            category: "Expenses:Transit".to_string(),
            period: "weekly".to_string(),
            period_start: "2026-05-10".to_string(),
            period_end: "2026-05-16".to_string(),
            target: "40.00".to_string(),
            actual: "32.50".to_string(),
            percent_used: 81.25,
            over_budget: false,
        },
    ]
}

#[cfg(feature = "mock")]
fn mock_budgets() -> Vec<BudgetRow> {
    vec![
        BudgetRow {
            id: "Expenses:Groceries".to_string(),
            amount: "600.00".to_string(),
            period: "monthly".to_string(),
            removed: false,
        },
        BudgetRow {
            id: "Expenses:DiningOut".to_string(),
            amount: "120.00".to_string(),
            period: "biweekly".to_string(),
            removed: false,
        },
        BudgetRow {
            id: "Expenses:Transit".to_string(),
            amount: "40.00".to_string(),
            period: "weekly".to_string(),
            removed: false,
        },
    ]
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

// ---------------------------------------------------------------------------
// Phase 6.2 / 6.3 — hledger journal import.
// ---------------------------------------------------------------------------

pub async fn invoke_preview_journal_import(
    path: &str,
) -> Result<crate::types::JournalImportPreview, String> {
    #[cfg(feature = "mock")]
    {
        use crate::types::*;
        let _ = path;
        Ok(JournalImportPreview {
            root: path.to_string(),
            files_parsed: 3,
            total_bytes: 12_400,
            transactions_count: 42,
            per_account: vec![
                JournalImportAccountStats {
                    account: "Assets:Cash".into(),
                    transaction_count: 30,
                    posting_count: 30,
                },
                JournalImportAccountStats {
                    account: "Expenses:Groceries".into(),
                    transaction_count: 10,
                    posting_count: 10,
                },
                JournalImportAccountStats {
                    account: "Expenses:Business:Subscriptions".into(),
                    transaction_count: 2,
                    posting_count: 2,
                },
            ],
            commodities: vec!["CAD".into(), "USD".into()],
            sample_transactions: vec![JournalImportSampleTxn {
                source_index: 0,
                txn_id: "import-deadbeefcafef00d-1".into(),
                date: "2026-05-26".into(),
                description: "Coffee".into(),
                postings: vec![
                    JournalImportPosting {
                        account: "Expenses:Cafe".into(),
                        commodity: "CAD".into(),
                        amount: "5.25".into(),
                        fx_quote: None,
                        fx_rate: None,
                        tags: vec![],
                    },
                    JournalImportPosting {
                        account: "Assets:Cash".into(),
                        commodity: "CAD".into(),
                        amount: "-5.25".into(),
                        fx_quote: None,
                        fx_rate: None,
                        tags: vec![],
                    },
                ],
            }],
            parse_errors: vec![],
            balance_failures: vec![],
            already_imported_count: 0,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            path: &'a str,
        }
        invoke("preview_journal_import", &Args { path }).await
    }
}

pub async fn invoke_commit_journal_import(
    path: &str,
    plan: crate::types::JournalImportPlan,
) -> Result<crate::types::JournalImportResult, String> {
    #[cfg(feature = "mock")]
    {
        let _ = (path, &plan);
        Ok(crate::types::JournalImportResult {
            committed_count: 40,
            skipped_existing_count: 0,
            dropped_count: 2,
            balance_failures: vec![],
            parse_errors: vec![],
            a2_rewrites: 2,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> {
            path: &'a str,
            plan: &'a crate::types::JournalImportPlan,
        }
        invoke("commit_journal_import", &Args { path, plan: &plan }).await
    }
}
