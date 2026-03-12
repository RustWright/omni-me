use dioxus::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

// --- Tauri IPC bindings ---
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    fn tauri_invoke(cmd: &str, args: JsValue) -> js_sys::Promise;
}

async fn invoke_greet(name: &str) -> Result<String, String> {
    #[derive(Serialize)]
    struct GreetArgs<'a> {
        name: &'a str,
    }
    let args = serde_wasm_bindgen::to_value(&GreetArgs { name }).map_err(|e| e.to_string())?;
    let result = wasm_bindgen_futures::JsFuture::from(tauri_invoke("greet", args))
        .await
        .map_err(|e| format!("{e:?}"))?;
    result.as_string().ok_or_else(|| "Non-string response".into())
}

// --- CodeMirror JS interop (same WebView, direct calls) ---
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = createEditor)]
    fn js_create_editor(element_id: &str, initial_content: &str);

    #[wasm_bindgen(js_name = getEditorContent)]
    fn js_get_editor_content() -> String;

    #[wasm_bindgen(js_name = setEditorContent)]
    fn js_set_editor_content(content: &str);
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut count = use_signal(|| 0);
    let mut ipc_result = use_signal(|| String::from("(not called yet)"));
    let mut editor_content = use_signal(|| String::from("(click 'Read Editor' to see content)"));
    let mut editor_ready = use_signal(|| false);

    // Load CodeMirror bundle and init editor after mount
    use_effect(move || {
        spawn(async move {
            // Dynamically load the editor bundle script
            let document = web_sys::window().unwrap().document().unwrap();
            let script = document.create_element("script").unwrap();
            script.set_attribute("src", "/assets/js/editor.bundle.js").unwrap();

            // Wait for script to load via a promise
            let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                let resolve_clone = resolve.clone();
                let onload = Closure::once_into_js(move || {
                    resolve_clone.call0(&JsValue::NULL).unwrap();
                });
                script.set_attribute("onload", "").unwrap();
                script
                    .dyn_ref::<web_sys::HtmlElement>()
                    .unwrap()
                    .set_onload(Some(onload.unchecked_ref()));
            });

            let body = document.body().unwrap();
            body.append_child(&script).unwrap();

            // Wait for the script to load
            wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();

            // Init the editor
            js_create_editor("editor-container", "# Hello from CodeMirror!\n\nType here...");
            editor_ready.set(true);
        });
    });

    rsx! {
        div {
            style: "font-family: sans-serif; padding: 2rem;",
            h1 { style: "text-align: center;", "Dioxus + Tauri POC" }

            // Counter (P3: Dioxus reactivity)
            div {
                style: "text-align: center; margin-bottom: 1rem;",
                p { "Counter: {count}" }
                button { onclick: move |_| count += 1, "Increment" }
                button { onclick: move |_| count -= 1, style: "margin-left: 0.5rem;", "Decrement" }
            }

            hr { style: "margin: 1rem 0;" }

            // IPC test (P3: Tauri command round-trip)
            div {
                style: "text-align: center; margin-bottom: 1rem;",
                h2 { "IPC Test" }
                button {
                    onclick: move |_| {
                        spawn(async move {
                            match invoke_greet("Dioxus").await {
                                Ok(msg) => ipc_result.set(msg),
                                Err(e) => ipc_result.set(format!("ERROR: {e}")),
                            }
                        });
                    },
                    "Call Rust greet()"
                }
                p { "Result: {ipc_result}" }
            }

            hr { style: "margin: 1rem 0;" }

            // CodeMirror editor (P4)
            h2 { style: "text-align: center;", "CodeMirror Editor" }

            if !editor_ready() {
                p { style: "text-align: center; color: gray;", "Loading editor..." }
            }

            // Editor container — CodeMirror mounts here
            div {
                id: "editor-container",
                style: "border: 1px solid #ccc; min-height: 150px; margin-bottom: 1rem;",
            }

            // Editor controls
            div {
                style: "text-align: center;",
                button {
                    onclick: move |_| {
                        let content = js_get_editor_content();
                        editor_content.set(content);
                    },
                    disabled: !editor_ready(),
                    "Read Editor Content"
                }
                button {
                    onclick: move |_| {
                        js_set_editor_content("# Content set from Dioxus!\n\nThis was injected via WASM → JS interop.");
                    },
                    disabled: !editor_ready(),
                    style: "margin-left: 0.5rem;",
                    "Set Editor Content"
                }
            }
            pre {
                style: "background: #f5f5f5; padding: 1rem; margin-top: 1rem; white-space: pre-wrap;",
                "{editor_content}"
            }
        }
    }
}
