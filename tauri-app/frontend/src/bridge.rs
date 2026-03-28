use wasm_bindgen::prelude::*;

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
