use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::bridge::{js_create_editor, js_destroy_editor};

/// The base options for `createEditor`, as an `Object` so the caller can attach `onCursor`.
/// `entry_date` lets a timestamp show a bare time on its own day and a date otherwise; `None`
/// (notes) turns timestamps off.
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
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("entryDate"),
            &JsValue::from_str(&date),
        );
    }
    obj
}

/// Attach the `onCursor` selection-change callback to an options object (1.8b).
/// Leaks the closure intentionally (same lifetime strategy as `on_change`): the
/// editor lives as long as the page, and a fresh editor is created per mount.
fn attach_cursor_cb(obj: &js_sys::Object, on_cursor: Option<EventHandler<usize>>) {
    let Some(handler) = on_cursor else { return };
    let closure =
        Closure::wrap(Box::new(move |pos: usize| handler.call(pos)) as Box<dyn Fn(usize)>);
    if let Some(f) = closure.as_ref().dyn_ref::<js_sys::Function>() {
        let _ = js_sys::Reflect::set(obj, &JsValue::from_str("onCursor"), f);
    }
    closure.forget();
}

const EDITOR_CONTAINER_ID: &str = "editor-container";

/// `<script src>` for the bundled CodeMirror build.
const EDITOR_BUNDLE_SRC: &str = "/assets/js/editor.bundle.js";

/// Inject `editor.bundle.js` (once) and resolve when `window.createEditor` is
/// defined. Returns `false` if it never appeared within the polling window.
///
/// Separate from [`Editor`] so the root can warm the bundle at mount, overlapping the boot
/// read (see `main.rs`); it dedupes, so the Editor's own call finds it loaded.
pub async fn ensure_editor_bundle() -> bool {
    // ~20s of polling: a cold parse of this bundle can exceed 5s (see
    // `ensure_js_bundle`). A remount — navigate away and back — re-runs the
    // Editor effect as a backstop.
    crate::bridge::ensure_js_bundle(EDITOR_BUNDLE_SRC, "createEditor", 200).await
}

#[component]
pub fn Editor(
    initial_content: String,
    on_change: EventHandler<String>,
    #[props(default = false)] read_only: bool,
    #[props(default = false)] journal_mode: bool,
    /// The journal day being edited (YYYY-MM-DD), used by the reveal-on-select
    /// line timestamps to decide same-day vs cross-day display (#344). `None`
    /// for non-journal surfaces → the timestamp feature stays off.
    #[props(default)]
    entry_date: Option<String>,
    /// Caret offset to restore on mount (1.8b). 0 = no restore.
    #[props(default = 0)]
    initial_cursor: usize,
    /// Fired on every selection change so the page can keep the stored caret
    /// offset current. `None` = the surface doesn't track cursor position.
    #[props(default)]
    on_cursor: Option<EventHandler<usize>>,
    /// Fired once, when CodeMirror has actually been created and the surface is
    /// typeable. The boot splash uses this (via `main.rs::use_boot_hold`) to
    /// avoid revealing a half-built editor; it stays `None` for surfaces that
    /// don't gate anything on it.
    #[props(default)]
    on_ready: Option<EventHandler<()>>,
) -> Element {
    let mut editor_ready = use_signal(|| false);

    // Wait for the CodeMirror bundle (see `ensure_editor_bundle` — the root
    // warms it at app mount, so this usually resolves immediately), then create
    // the editor instance for this mount.
    use_effect(move || {
        let initial = initial_content.clone();
        let entry_date = entry_date.clone();

        spawn(async move {
            if !ensure_editor_bundle().await {
                return;
            }

            let create = || -> Option<()> {
                let on_change_closure = Closure::wrap(Box::new(move |content: String| {
                    on_change.call(content);
                }) as Box<dyn Fn(String)>);

                let on_change_fn = on_change_closure
                    .as_ref()
                    .dyn_ref::<js_sys::Function>()?
                    .clone();

                on_change_closure.forget(); // Leak memory intentionally

                let opts =
                    editor_options(journal_mode, read_only, initial_cursor, entry_date.clone());
                attach_cursor_cb(&opts, on_cursor);
                js_create_editor(
                    EDITOR_CONTAINER_ID,
                    &initial,
                    Some(&on_change_fn),
                    opts.into(),
                );

                Some(())
            };

            if create().is_some() {
                editor_ready.set(true);
                if let Some(handler) = on_ready {
                    handler.call(());
                }
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
