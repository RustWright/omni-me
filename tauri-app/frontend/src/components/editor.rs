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

    // --- Dev-Only: Custom JS loading and polling for dx serve quirks ---
    #[cfg(debug_assertions)]
    use_effect(move || {
        let initial = initial_content.clone();
        let on_change = on_change.clone();

        spawn(async move {
            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let document = match window.document() {
                Some(d) => d,
                None => return,
            };

            let script_src = "/assets/js/editor.bundle.js";
            let editor_container_id = EDITOR_CONTAINER_ID;

            // Helper function to use `?` for early returns in Option context
            let mut setup_script_and_poll_editor = async || -> Option<()> {
                // 1. Check for existing script to prevent duplicates on hot-reload
                let existing_script = document.query_selector(&format!("script[src='{}']", script_src))
                    .ok()
                    .flatten();
                
                if existing_script.is_none() {
                    let script = document.create_element("script").ok()?;
                    script.set_attribute("src", script_src).ok()?;
                    script.set_attribute("async", "").ok()?;
                    document.body()?.append_child(&script).ok()?;
                }

                // 2. Poll for window.createEditor to be defined, with a timeout
                let mut attempts = 0;
                const MAX_ATTEMPTS: u8 = 50;
                const POLL_INTERVAL_MS: u32 = 100;

                while attempts < MAX_ATTEMPTS {
                    let create_editor_is_defined = js_sys::Reflect::get(&window, &JsValue::from_str("createEditor"))
                        .ok() // Option<JsValue>
                        .and_then(|val| val.dyn_ref::<js_sys::Function>().map(|_| ())).is_some();

                    if create_editor_is_defined {
                        break;
                    }

                    attempts += 1;
                    let timeout_promise = js_sys::Promise::new(&mut |resolve, _| {
                        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, POLL_INTERVAL_MS as i32);
                    });
                    // Convert JsFuture Result to Option for `?` compatibility
                    wasm_bindgen_futures::JsFuture::from(timeout_promise).await.ok()?;
                }

                if attempts == MAX_ATTEMPTS {
                    web_sys::console::error_1(&JsValue::from_str("CodeMirror editor: createEditor not available after polling."));
                    return None;
                }

                // 3. Setup the JS callback
                let on_change_closure = Closure::wrap(Box::new(move |content: String| {
                    on_change.call(content);
                }) as Box<dyn Fn(String)>);

                let on_change_fn = on_change_closure
                    .as_ref()
                    .dyn_ref::<js_sys::Function>()?
                    .clone();

                on_change_closure.forget(); // Leak memory intentionally

                // 4. Initialize the editor
                js_create_editor(editor_container_id, &initial, Some(&on_change_fn));
                
                Some(()) // Indicates success
            };

            if setup_script_and_poll_editor().await.is_some() {
                editor_ready.set(true);
            }
        });
    });

    // --- Production: Original, stable Tauri environment loading ---
    #[cfg(not(debug_assertions))]
    use_effect(move || {
        let initial = initial_content.clone();
        let on_change = on_change.clone();

        spawn(async move {
            let window = web_sys::window().expect("no window");
            let document = window.document().expect("no document");

            // Create <script> element for the editor bundle (original logic)
            let script = document
                .create_element("script")
                .expect("failed to create script element");
            script
                .set_attribute("src", "/assets/js/editor.bundle.js")
                .expect("failed to set script src");

            // Wait for the script to load via a Promise (original logic)
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

            // Await script load (original logic)
            wasm_bindgen_futures::JsFuture::from(promise)
                .await
                .expect("script load failed");

            // Create a JS callback for onChange (original logic)
            let on_change_closure = Closure::wrap(Box::new(move |content: String| {
                on_change.call(content);
            }) as Box<dyn Fn(String)>);

            let on_change_fn = on_change_closure
                .as_ref()
                .dyn_ref::<js_sys::Function>()
                .expect("closure is not a Function")
                .clone();

            // Leak the closure (original logic)
            on_change_closure.forget();

            // Initialize the editor (original logic)
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
            class: "w-full min-h-[400px] border border-white/10 rounded-xl overflow-hidden bg-obsidian-sidebar/20 shadow-inner flex flex-col",

            if !*editor_ready.read() {
                div {
                    class: "p-4 text-obsidian-text-muted text-sm flex items-center justify-center h-full",
                    "Initializing editor environment..."
                }
            }

            div {
                id: EDITOR_CONTAINER_ID,
                class: "flex-1 w-full outline-none p-4 font-mono text-[14px] text-obsidian-text leading-relaxed",
            }
        }
    }
}
