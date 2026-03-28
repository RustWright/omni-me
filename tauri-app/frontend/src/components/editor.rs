use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::bridge::{js_create_editor, js_destroy_editor};

const EDITOR_CONTAINER_ID: &str = "editor-container";

#[component]
pub fn Editor(
    initial_content: String,
    on_change: EventHandler<String>,
    #[props(default = false)] read_only: bool,
) -> Element {
    let mut editor_ready = use_signal(|| false);

    // Load the CodeMirror bundle script and initialize the editor
    use_effect(move || {
        let initial = initial_content.clone();

        spawn(async move {
            let window = web_sys::window().expect("no window");
            let document = window.document().expect("no document");

            // Create <script> element for the editor bundle
            let script = document
                .create_element("script")
                .expect("failed to create script element");
            script
                .set_attribute("src", "/assets/js/editor.bundle.js")
                .expect("failed to set script src");

            // Wait for the script to load via a Promise
            let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                let resolve_clone = resolve.clone();
                let onload = Closure::once_into_js(move || {
                    resolve_clone.call0(&JsValue::NULL).unwrap();
                });
                script
                    .dyn_ref::<web_sys::HtmlElement>()
                    .expect("script is not HtmlElement")
                    .set_onload(Some(onload.unchecked_ref()));
            });

            let body = document.body().expect("no body");
            body.append_child(&script).expect("failed to append script");

            // Await script load
            wasm_bindgen_futures::JsFuture::from(promise)
                .await
                .expect("script load failed");

            // Create a JS callback for onChange that debounces and forwards to Dioxus
            let on_change_closure = Closure::wrap(Box::new(move |content: String| {
                on_change.call(content);
            }) as Box<dyn Fn(String)>);

            let on_change_fn = on_change_closure
                .as_ref()
                .dyn_ref::<js_sys::Function>()
                .expect("closure is not a Function")
                .clone();

            // Leak the closure so it stays alive for the lifetime of the editor.
            // Cleanup happens via destroyEditor() which removes the listener.
            on_change_closure.forget();

            // Initialize the editor
            js_create_editor(EDITOR_CONTAINER_ID, &initial, Some(&on_change_fn));

            editor_ready.set(true);
        });
    });

    // Cleanup on unmount
    use_drop(move || {
        js_destroy_editor();
    });

    rsx! {
        div {
            style: "
                width: 100%;
                min-height: 300px;
                border: 1px solid #ddd;
                border-radius: 8px;
                overflow: hidden;
                background: #fff;
            ",

            if !*editor_ready.read() {
                div {
                    style: "
                        padding: 16px;
                        color: #888;
                        font-size: 14px;
                    ",
                    "Loading editor..."
                }
            }

            div {
                id: EDITOR_CONTAINER_ID,
                style: "
                    width: 100%;
                    min-height: 300px;
                ",
            }
        }
    }
}
