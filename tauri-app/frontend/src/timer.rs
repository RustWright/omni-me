//! Browser-async sleep helper. Wraps `setTimeout` in a `JsFuture` so it can be
//! awaited from a Dioxus `spawn`. The non-wasm fallback exists so `cargo check`
//! from the parent workspace doesn't fail — the frontend is only ever run as
//! wasm in practice.

/// How long the editor must be quiet before an auto-save fires. Matches
/// Cycle 2's "1s local debounce" decision (project.md: Obsidian-equivalent).
/// Shared between journal and generic notes so the cadence stays consistent.
pub const AUTOSAVE_DEBOUNCE_MS: i32 = 1000;

#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: i32) {
    if let Some(window) = web_sys::window() {
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: i32) {
    let _ = ms;
}
