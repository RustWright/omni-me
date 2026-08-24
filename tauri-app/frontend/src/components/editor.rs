use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::bridge::{js_create_editor, js_destroy_editor};

/// Build the base `{ journalMode, readOnly, initialCursor, entryDate }` options
/// object for `createEditor`. Returned as an `Object` (not `JsValue`) so the
/// caller can attach the `onCursor` callback before forwarding it.
///
/// `entry_date` (YYYY-MM-DD) is the journal day being edited; the editor's
/// reveal-on-select timestamps compare a line's finish date against it so a
/// same-day line shows a bare time and a line finished on another day carries
/// its date (#344). `None`/empty for non-journal surfaces (notes) → the
/// timestamp feature stays off.
fn editor_options(
    journal_mode: bool,
    read_only: bool,
    initial_cursor: usize,
    entry_date: Option<String>,
) -> js_sys::Object {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("journalMode"),
        &JsValue::from_bool(journal_mode),
    );
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("readOnly"),
        &JsValue::from_bool(read_only),
    );
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("initialCursor"),
        &JsValue::from_f64(initial_cursor as f64),
    );
    if let Some(date) = entry_date {
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("entryDate"), &JsValue::from_str(&date));
    }
    obj
}

/// Attach the `onCursor` selection-change callback to an options object (1.8b).
/// Leaks the closure intentionally (same lifetime strategy as `on_change`): the
/// editor lives as long as the page, and a fresh editor is created per mount.
fn attach_cursor_cb(obj: &js_sys::Object, on_cursor: Option<EventHandler<usize>>) {
    let Some(handler) = on_cursor else { return };
    let closure = Closure::wrap(Box::new(move |pos: usize| handler.call(pos)) as Box<dyn Fn(usize)>);
    if let Some(f) = closure.as_ref().dyn_ref::<js_sys::Function>() {
        let _ = js_sys::Reflect::set(obj, &JsValue::from_str("onCursor"), f);
    }
    closure.forget();
}

const EDITOR_CONTAINER_ID: &str = "editor-container";

#[component]
pub fn Editor(
    initial_content: String,
    on_change: EventHandler<String>,
    #[props(default = false)] read_only: bool,
    #[props(default = false)] journal_mode: bool,
    /// The journal day being edited (YYYY-MM-DD), used by the reveal-on-select
    /// line timestamps to decide same-day vs cross-day display (#344). `None`
    /// for non-journal surfaces → the timestamp feature stays off.
    #[props(default)] entry_date: Option<String>,
    /// Caret offset to restore on mount (1.8b). 0 = no restore.
    #[props(default = 0)] initial_cursor: usize,
    /// Fired on every selection change so the page can keep the stored caret
    /// offset current. `None` = the surface doesn't track cursor position.
    #[props(default)] on_cursor: Option<EventHandler<usize>>,
) -> Element {
    let mut editor_ready = use_signal(|| false);

    // Load the CodeMirror bundle, then POLL for `window.createEditor` rather than
    // awaiting `script.onload`. The old release-only onload path could hang
    // forever in an embedded Tauri webview (the script loads, but the awaited
    // onload never resolved), stranding the editor on "Initializing…". Polling is
    // robust across dx serve, embedded release builds, and Android, and dedupes
    // the injected script so repeated mounts don't stack copies. One path for all
    // build modes — the previous `cfg(debug_assertions)` split meant the release
    // path was never exercised until a real desktop webview ran it.
    use_effect(move || {
        let initial = initial_content.clone();
        let entry_date = entry_date.clone();

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
            let setup_script_and_poll_editor = async || -> Option<()> {
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

                // 2. Poll for window.createEditor to be defined. The window must
                // be GENEROUS: on a cold first launch (empty webview cache, the
                // ~1 MB bundle parsed for the first time while the wasm frontend
                // and DB init compete for the main thread) the embedded webkit
                // webview can take well over 5s to define createEditor. The old
                // 5s cap stranded the editor on "Initializing…" on first launch,
                // yet worked on relaunch once webkit had cached the bundle. ~20s
                // covers the cold case; a remount (navigate away + back) re-runs
                // this effect as a backstop.
                let mut attempts = 0;
                const MAX_ATTEMPTS: u8 = 200;
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
                    web_sys::console::error_1(&JsValue::from_str("CodeMirror editor: createEditor still undefined after ~20s — editor.bundle.js likely failed to load (check the Network tab for a 404 or MIME error)."));
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
                let opts = editor_options(journal_mode, read_only, initial_cursor, entry_date.clone());
                attach_cursor_cb(&opts, on_cursor);
                js_create_editor(
                    editor_container_id,
                    &initial,
                    Some(&on_change_fn),
                    opts.into(),
                );

                Some(()) // Indicates success
            };

            if setup_script_and_poll_editor().await.is_some() {
                editor_ready.set(true);
            }
        });
    });

    // Cleanup on unmount
    use_drop(move || {
        js_destroy_editor();
    });

    rsx! {
        // Full-bleed, fill-height writing surface (Phase 5 "editor feel"): no
        // card border/shadow/padding here — the CM theme owns the typography +
        // inset, and the page column supplies the gutter. `flex-1 min-h-0` lets
        // the editor grow to fill the page's flex column so a short note still
        // occupies the whole screen (no fixed 400px island), while a long note
        // grows past the viewport and the page column scrolls.
        div {
            class: "flex-1 flex flex-col w-full min-h-0",

            if !*editor_ready.read() {
                div {
                    class: "p-4 text-obsidian-text-muted text-sm flex items-center justify-center h-full",
                    "Initializing editor environment..."
                }
            }

            div {
                id: EDITOR_CONTAINER_ID,
                class: "flex-1 w-full flex flex-col min-h-0 outline-none text-obsidian-text",
            }
        }
    }
}
