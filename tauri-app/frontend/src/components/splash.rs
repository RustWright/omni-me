use dioxus::prelude::*;

/// Boot splash — the animated enso held over the app until the restored tab is
/// known and its content is genuinely ready. Driven by `main.rs`'s `BootGate`.
///
/// Before this, first paint always rendered `Tab::Journal` with a half-built
/// CodeMirror editor and *then* corrected to the tab the user actually closed
/// on, so the wait was paid twice and its most visible half looked broken.
///
/// **The mark is inlined rather than loaded from `tauri-app/branding/`** — it is
/// four elements, and a splash that has to fetch its own artwork can't paint on
/// the frame it's needed. Keep it in sync with `branding/omni-me-fg.svg`; that
/// file remains the canonical source for every generated icon.
///
/// **Two overlaid `<svg>`s, not one with an animated `<g>`.** Rotating an
/// element *inside* an SVG needs `transform-box`/`transform-origin` handling
/// that differs across engines, and this app ships on webkit2gtk (desktop) and
/// Android Chromium — where such differences have burned us before (see the
/// `select` rules in `input.css`). Rotating the whole `<svg>`, an ordinary HTML
/// element, is unambiguous everywhere.
#[component]
pub fn Splash(
    /// Fade the splash out. The parent keeps the component mounted for the
    /// length of the CSS transition, then drops it.
    fading: bool,
) -> Element {
    let opacity = if fading { "opacity-0" } else { "opacity-100" };

    rsx! {
        div {
            // Above every app layer. The highest thing in the app is the mobile
            // NavDrawer — scrim `z-[140]`, panel `z-[150]` in `nav.rs` — so this
            // must clear 150; being last in the tree is not enough once
            // z-indexes are in play. Inert to input: the app underneath is
            // mid-build, so a stray tap landing as the splash fades must not
            // reach a control the user can't see yet.
            class: "fixed inset-0 z-[200] flex items-center justify-center bg-obsidian-bg pointer-events-none transition-opacity duration-200 {opacity}",
            "aria-hidden": "true",

            // 144px. Compared against 96 and 192 on a 390px-wide viewport: at 96
            // the brush terminals and the open gap are too small to read and it
            // looks like a generic spinner rather than the app's mark; 192
            // dominates a phone screen. Note Tailwind only generates the sizes it
            // finds in `src/**/*.rs`, so changing this needs the class literal
            // changed here, not a computed value.
            div { class: "relative w-36 h-36",
                // The open brush ring + its two terminal dots. Rotating sweeps
                // the gap around, which is what reads as "working".
                svg {
                    class: "absolute inset-0 w-full h-full splash-ring",
                    view_box: "0 0 100 100",
                    path { d: "M75.21,40.32 A27 27 0 1 1 59.68,24.79 L57.69,32.53 A17 17 0 1 0 67.47,42.31 Z" }
                    circle { cx: "71.34", cy: "41.32", r: "3.99" }
                    circle { cx: "58.68", cy: "28.66", r: "3.99" }
                }
                // The off-white core, held still and breathing — the fixed point
                // the ring turns around.
                svg {
                    class: "absolute inset-0 w-full h-full splash-core",
                    view_box: "0 0 100 100",
                    circle { cx: "50", cy: "50", r: "5.0" }
                }
            }
        }
    }
}
