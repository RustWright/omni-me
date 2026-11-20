//! The one icon primitive: `Icon { name: IconName::ChevronRight, class: "w-4 h-4" }`.
//! Paths are heroicons v1 outline; never mix in v2, whose decimal coords draw at a visibly
//! different weight.

use dioxus::prelude::*;

/// The curated icon set. The first block is every distinct path harvested from
/// the existing inline SVGs (so swapping call sites is byte-identical); the
/// second block is additions the finances redesign needs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    // ── Already used inline across the app ──
    ChevronRight,
    ChevronLeft,
    ArrowLeft,
    Link,
    Close,
    Check,
    Search,
    Plus,
    Minus,
    Pencil,
    Menu,
    Refresh,
    Calendar,
    ClipboardCheck,
    DocumentText,
    // ── Additions for the finances IA (Overview · Ledger · Analyze) ──
    ChartBar,
    Inbox,
    Wallet,
    ArrowUp,
    ArrowDown,
    // ── Privacy eye ──
    Eye,
    EyeOff,
}

impl IconName {
    /// The SVG `d` attribute for this icon.
    fn path(self) -> &'static str {
        match self {
            IconName::ChevronRight => "M9 5l7 7-7 7",
            IconName::ChevronLeft => "M15 19l-7-7 7-7",
            IconName::ArrowLeft => "M10 19l-7-7m0 0l7-7m-7 7h18",
            IconName::Link => {
                "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13"
            }
            IconName::Close => "M6 18L18 6M6 6l12 12",
            IconName::Check => "M5 13l4 4L19 7",
            IconName::Search => "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z",
            IconName::Plus => "M12 4v16m8-8H4",
            IconName::Minus => "M20 12H4",
            IconName::Pencil => {
                "M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z"
            }
            IconName::Menu => "M4 6h16M4 12h16M4 18h16",
            IconName::Refresh => {
                "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            }
            IconName::Calendar => {
                "M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
            }
            IconName::ClipboardCheck => {
                "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4"
            }
            IconName::DocumentText => {
                "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            }
            IconName::ChartBar => {
                "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
            }
            IconName::Inbox => {
                "M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4"
            }
            IconName::Wallet => {
                "M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            }
            IconName::ArrowUp => "M5 10l7-7m0 0l7 7m-7-7v18",
            IconName::ArrowDown => "M19 14l-7 7m0 0l-7-7m7 7V3",
            IconName::Eye => {
                "M15 12a3 3 0 11-6 0 3 3 0 016 0zM2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
            }
            IconName::EyeOff => {
                "M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21"
            }
        }
    }
}

/// Render a stroked outline icon. `class` carries the size + color (default
/// `w-5 h-5`, inherits `currentColor` from the parent's text color). `stroke`
/// tunes the path weight (default `"2"`) — the app's large empty-state glyphs
/// use `"1"` and its emphasis check/minus marks use `"3"`, so faithful
/// conversions can preserve those.
#[component]
pub fn Icon(
    name: IconName,
    #[props(default = "w-5 h-5".to_string())] class: String,
    #[props(default = "2".to_string())] stroke: String,
) -> Element {
    rsx! {
        svg {
            class: "{class}",
            fill: "none",
            stroke: "currentColor",
            view_box: "0 0 24 24",
            "aria-hidden": "true",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                stroke_width: "{stroke}",
                d: name.path(),
            }
        }
    }
}
