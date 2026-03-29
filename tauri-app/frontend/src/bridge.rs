use wasm_bindgen::prelude::*;

use crate::types::{
    CompletionEntry, LlmResult, NoteListItem, RoutineGroup, RoutineItem, SyncInfo, SyncStatus,
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

// --- Typed IPC helpers ---

async fn invoke<T: serde::de::DeserializeOwned>(
    cmd: &str,
    args: &impl serde::Serialize,
) -> Result<T, String> {
    let args_js =
        serde_wasm_bindgen::to_value(args).map_err(|e| format!("serialize args: {e}"))?;
    let promise = tauri_invoke(cmd, args_js);
    let result = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("{e:?}"))?;
    serde_wasm_bindgen::from_value(result).map_err(|e| format!("deserialize result: {e}"))
}

async fn invoke_unit(cmd: &str, args: &impl serde::Serialize) -> Result<(), String> {
    let args_js =
        serde_wasm_bindgen::to_value(args).map_err(|e| format!("serialize args: {e}"))?;
    let promise = tauri_invoke(cmd, args_js);
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

// --- Notes ---

pub async fn invoke_create_note(raw_text: &str, date: &str) -> Result<NoteListItem, String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        raw_text: &'a str,
        date: &'a str,
    }
    invoke("create_note", &Args { raw_text, date }).await
}

pub async fn invoke_list_notes() -> Result<Vec<NoteListItem>, String> {
    #[derive(serde::Serialize)]
    struct Args {}
    invoke("list_notes", &Args {}).await
}

pub async fn invoke_get_note(id: &str) -> Result<NoteListItem, String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        id: &'a str,
    }
    invoke("get_note", &Args { id }).await
}

pub async fn invoke_update_note(id: &str, raw_text: &str) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        id: &'a str,
        raw_text: &'a str,
    }
    invoke_unit("update_note", &Args { id, raw_text }).await
}

pub async fn invoke_search_notes(query: &str) -> Result<Vec<NoteListItem>, String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        query: &'a str,
    }
    invoke("search_notes", &Args { query }).await
}

pub async fn invoke_process_note_llm(note_id: &str) -> Result<LlmResult, String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        note_id: &'a str,
    }
    invoke("process_note_llm", &Args { note_id }).await
}

// --- Routines ---

pub async fn invoke_create_routine_group(
    name: &str,
    frequency: &str,
    time_of_day: &str,
) -> Result<RoutineGroup, String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        name: &'a str,
        frequency: &'a str,
        time_of_day: &'a str,
    }
    invoke(
        "create_routine_group",
        &Args {
            name,
            frequency,
            time_of_day,
        },
    )
    .await
}

pub async fn invoke_list_routine_groups() -> Result<Vec<RoutineGroup>, String> {
    #[derive(serde::Serialize)]
    struct Args {}
    invoke("list_routine_groups", &Args {}).await
}

pub async fn invoke_add_routine_item(
    group_id: &str,
    name: &str,
    duration_min: u32,
    order: u32,
) -> Result<RoutineItem, String> {
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

pub async fn invoke_list_routine_items(group_id: &str) -> Result<Vec<RoutineItem>, String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        group_id: &'a str,
    }
    invoke("list_routine_items", &Args { group_id }).await
}

pub async fn invoke_complete_routine_item(
    item_id: &str,
    group_id: &str,
    date: &str,
) -> Result<(), String> {
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

pub async fn invoke_skip_routine_item(
    item_id: &str,
    group_id: &str,
    date: &str,
    reason: Option<&str>,
) -> Result<(), String> {
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

pub async fn invoke_modify_routine_group(
    group_id: &str,
    changes: &serde_json::Value,
    justification: Option<&str>,
) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        group_id: &'a str,
        changes: &'a serde_json::Value,
        justification: Option<&'a str>,
    }
    invoke_unit(
        "modify_routine_group",
        &Args {
            group_id,
            changes,
            justification,
        },
    )
    .await
}

pub async fn invoke_get_completions_for_date(
    group_id: &str,
    date: &str,
) -> Result<Vec<CompletionEntry>, String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        group_id: &'a str,
        date: &'a str,
    }
    invoke("get_completions_for_date", &Args { group_id, date }).await
}

pub async fn invoke_get_routine_history(
    group_id: &str,
    days: u32,
) -> Result<Vec<CompletionEntry>, String> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        group_id: &'a str,
        days: u32,
    }
    invoke("get_routine_history", &Args { group_id, days }).await
}

// --- Sync ---

pub async fn invoke_trigger_sync() -> Result<SyncStatus, String> {
    #[derive(serde::Serialize)]
    struct Args {}
    invoke("trigger_sync", &Args {}).await
}

pub async fn invoke_get_sync_info() -> Result<SyncInfo, String> {
    #[derive(serde::Serialize)]
    struct Args {}
    invoke("get_sync_info", &Args {}).await
}
