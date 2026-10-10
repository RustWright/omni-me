//! Keep a focused form field above the Android soft keyboard. Edge-to-edge
//! ignores `adjustResize`; `InsetBridge.kt` reports the keyboard height instead.
//! The CodeMirror editor has its own caret logic in `editor.js` and is skipped.

/// Gap kept between the field and the keyboard's top edge, in CSS px.
const MARGIN_PX: f64 = 16.0;

/// Install once at the root.
pub fn install() {
    // The inset event covers the keyboard opening; `focusin` covers moving to
    // another field while it is already open, when no inset change fires.
    crate::bridge::listen_window_event("omni:keyboardinset", reveal_focused);
    crate::bridge::listen_window_event("focusin", reveal_focused);
}

fn reveal_focused() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let kb = keyboard_inset(&window);
    if kb <= 0.0 {
        return;
    }
    let Some(el) = window.document().and_then(|d| d.active_element()) else {
        return;
    };
    if !matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") {
        return;
    }
    if el.closest(".cm-editor").ok().flatten().is_some() {
        return;
    }
    let inner = window
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if el.get_bounding_client_rect().bottom() <= inner - kb - MARGIN_PX {
        return;
    }
    let opts = web_sys::ScrollIntoViewOptions::new();
    opts.set_block(web_sys::ScrollLogicalPosition::Center);
    el.scroll_into_view_with_scroll_into_view_options(&opts);
}

/// `--keyboard-inset-bottom`, which `InsetBridge.kt` sets; 0 when it is hidden.
fn keyboard_inset(window: &web_sys::Window) -> f64 {
    let Some(root) = window.document().and_then(|d| d.document_element()) else {
        return 0.0;
    };
    window
        .get_computed_style(&root)
        .ok()
        .flatten()
        .and_then(|s| s.get_property_value("--keyboard-inset-bottom").ok())
        .and_then(|v| v.trim().trim_end_matches("px").parse::<f64>().ok())
        .unwrap_or(0.0)
}
