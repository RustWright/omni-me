use wasm_bindgen::prelude::*;
use crate::types::{
    CompletionEntry, LlmResult, NoteListItem, RoutineGroup, RoutineItem, SyncInfo, SyncStatus,
    TaskResult, TimezoneInfo,
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
    );
    #[wasm_bindgen(js_name = getEditorContent)]
    pub fn js_get_editor_content() -> String;
    #[wasm_bindgen(js_name = setEditorContent)]
    pub fn js_set_editor_content(content: &str);
    #[wasm_bindgen(js_name = destroyEditor)]
    pub fn js_destroy_editor();
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

// --- Notes ---

pub async fn invoke_create_note(raw_text: &str, date: &str) -> Result<NoteListItem, String> {
    #[cfg(feature = "mock")]
    {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(NoteListItem {
            id: "mock-new".to_string(),
            raw_text: raw_text.to_string(),
            date: date.to_string(),
            tags: vec![],
            summary: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { raw_text: &'a str, date: &'a str }
        invoke("create_note", &Args { raw_text, date }).await
    }
}

pub async fn invoke_list_notes() -> Result<Vec<NoteListItem>, String> {
    #[cfg(feature = "mock")]
    {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let now = chrono::Utc::now().to_rfc3339();
        Ok(vec![
            NoteListItem {
                id: "1".into(),
                raw_text: "# Morning Journal\nFeeling productive today. Goal: Refactor the UI to match Blue Topaz theme.".into(),
                date: today.clone(),
                tags: vec!["journal".into(), "ui-dev".into()],
                summary: Some("Refactoring the UI for Blue Topaz.".into()),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
            NoteListItem {
                id: "2".into(),
                raw_text: "## Workout Log\n- Bench press: 3x10\n- Squats: 3x10\nRemember to drink more water.".into(),
                date: today.clone(),
                tags: vec!["fitness".into()],
                summary: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        ])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("list_notes", &Args {}).await
    }
}

pub async fn invoke_update_note(id: &str, raw_text: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    { let _ = (id, raw_text); Ok(()) }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { id: &'a str, raw_text: &'a str }
        invoke_unit("update_note", &Args { id, raw_text }).await
    }
}

pub async fn invoke_search_notes(query: &str) -> Result<Vec<NoteListItem>, String> {
    #[cfg(feature = "mock")]
    {
        let notes = invoke_list_notes().await?;
        Ok(notes.into_iter().filter(|n| n.raw_text.to_lowercase().contains(&query.to_lowercase())).collect())
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { query: &'a str }
        invoke("search_notes", &Args { query }).await
    }
}

pub async fn invoke_process_note_llm(note_id: &str) -> Result<LlmResult, String> {
    #[cfg(feature = "mock")]
    {
        let _ = note_id;
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
        struct Args<'a> { note_id: &'a str }
        invoke("process_note_llm", &Args { note_id }).await
    }
}

// --- Routines ---

pub async fn invoke_list_routine_groups() -> Result<Vec<RoutineGroup>, String> {
    #[cfg(feature = "mock")]
    {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(vec![
            RoutineGroup {
                id: "rg1".into(),
                name: "Morning Ritual".into(),
                frequency: "daily".into(),
                time_of_day: "morning".into(),
                created_at: now.clone(),
                updated_at: now.clone(),
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

pub async fn invoke_list_routine_items(group_id: &str) -> Result<Vec<RoutineItem>, String> {
    #[cfg(feature = "mock")]
    {
        let gid = group_id.to_string();
        Ok(vec![
            RoutineItem { id: "i1".into(), group_id: gid.clone(), name: "Glass of water".into(), estimated_duration_min: 1, order_num: 0 },
            RoutineItem { id: "i2".into(), group_id: gid.clone(), name: "Meditation".into(), estimated_duration_min: 10, order_num: 1 },
        ])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { group_id: &'a str }
        invoke("list_routine_items", &Args { group_id }).await
    }
}

pub async fn invoke_get_completions_for_date(group_id: &str, date: &str) -> Result<Vec<CompletionEntry>, String> {
    #[cfg(feature = "mock")]
    {
        let _ = (group_id, date);
        Ok(vec![
            CompletionEntry { id: "c1".into(), item_id: "i1".into(), group_id: group_id.to_string(), date: date.to_string(), skipped: false, reason: None },
        ])
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { group_id: &'a str, date: &'a str }
        invoke("get_completions_for_date", &Args { group_id, date }).await
    }
}

pub async fn invoke_get_routine_history(group_id: &str, days: u32) -> Result<Vec<CompletionEntry>, String> {
    #[cfg(feature = "mock")]
    { let _ = (group_id, days); Ok(vec![]) }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { group_id: &'a str, days: u32 }
        invoke("get_routine_history", &Args { group_id, days }).await
    }
}

// --- Sync & Other ---

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
    { Ok(SyncStatus { pulled: 5, pushed: 2 }) }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args {}
        invoke("trigger_sync", &Args {}).await
    }
}

pub async fn invoke_update_server_url(server_url: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    { let _ = server_url; Ok(()) }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { server_url: &'a str }
        invoke_unit("update_server_url", &Args { server_url }).await
    }
}

// --- Timezone ---

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
    { let _ = timezone; Ok(()) }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { timezone: &'a str }
        invoke_unit("update_timezone", &Args { timezone }).await
    }
}

pub async fn invoke_get_note(id: &str) -> Result<NoteListItem, String> {
    let notes = invoke_list_notes().await?;
    notes.into_iter().find(|n| n.id == id).ok_or_else(|| "Not found".to_string())
}

pub async fn invoke_create_routine_group(name: &str, frequency: &str, time_of_day: &str) -> Result<RoutineGroup, String> {
    #[cfg(feature = "mock")]
    { 
        let now = chrono::Utc::now().to_rfc3339();
        Ok(RoutineGroup { 
            id: "new-mock".into(), 
            name: name.into(), 
            frequency: frequency.into(), 
            time_of_day: time_of_day.into(),
            created_at: now.clone(),
            updated_at: now,
        }) 
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { name: &'a str, frequency: &'a str, time_of_day: &'a str }
        invoke("create_routine_group", &Args { name, frequency, time_of_day }).await
    }
}

pub async fn invoke_add_routine_item(group_id: &str, name: &str, duration_min: u32, order_num: u32) -> Result<RoutineItem, String> {
    #[cfg(feature = "mock")]
    { 
        Ok(RoutineItem { 
            id: "new-item".into(), 
            group_id: group_id.to_string(), 
            name: name.into(), 
            estimated_duration_min: duration_min as i64, 
            order_num: order_num as i64 
        }) 
    }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { group_id: &'a str, name: &'a str, duration_min: u32, order_num: u32 }
        invoke("add_routine_item", &Args { group_id, name, duration_min, order_num }).await
    }
}

pub async fn invoke_complete_routine_item(item_id: &str, group_id: &str, date: &str) -> Result<(), String> {
    #[cfg(feature = "mock")]
    { let _ = (item_id, group_id, date); Ok(()) }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { item_id: &'a str, group_id: &'a str, date: &'a str }
        invoke_unit("complete_routine_item", &Args { item_id, group_id, date }).await
    }
}

pub async fn invoke_skip_routine_item(item_id: &str, group_id: &str, date: &str, reason: Option<&str>) -> Result<(), String> {
    #[cfg(feature = "mock")]
    { let _ = (item_id, group_id, date, reason); Ok(()) }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { item_id: &'a str, group_id: &'a str, date: &'a str, reason: Option<&'a str> }
        invoke_unit("skip_routine_item", &Args { item_id, group_id, date, reason }).await
    }
}

pub async fn invoke_modify_routine_group(group_id: &str, changes: &serde_json::Value, justification: Option<&str>) -> Result<(), String> {
    #[cfg(feature = "mock")]
    { let _ = (group_id, changes, justification); Ok(()) }
    #[cfg(not(feature = "mock"))]
    {
        #[derive(serde::Serialize)]
        struct Args<'a> { group_id: &'a str, changes: &'a serde_json::Value, justification: Option<&'a str> }
        invoke_unit("modify_routine_group", &Args { group_id, changes, justification }).await
    }
}
