use dioxus::prelude::*;

use crate::bridge;
use crate::types::{
    AttachmentRef, ExtractedDraft, PendingShareCapture, PostingInput, TransactionFormDraft,
};

/// Which kind of file-based capture the user opened. Drives the picker
/// `accept` filter, the camera hint, the title, and whether the hint
/// selector is offered (PDFs require a user pick; photos default to receipt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentKind {
    Photo,
    Pdf,
}

/// Classify a `PendingShareCapture` MIME into a `DocumentKind`. Used when an
/// Android share-target intent hands us bytes and we need to decide which
/// capture view (Photo vs PDF) to route into.
///
/// Returns `None` when the MIME is something we don't support as a financial
/// document (e.g., `text/plain` — that flow lives in EmailCapture, not here).
/// `None` short-circuits the share-target routing: the bytes are dropped and
/// the user lands on the regular Finances home so they can pick a flow
/// manually.
///
/// `filename` is included so the implementation can fall back to the file
/// extension when the MIME is generic (Android share apps sometimes hand us
/// `application/octet-stream` for a PDF).
///
/// **Permissive by design.** Anything in `application/*` with a `.pdf` name
/// (legacy `application/x-pdf`, misdeclared `application/zip`, the canonical
/// `application/octet-stream` stripped-MIME case) routes to Pdf. The looseness
/// is safe because Gemini is the actual validator downstream — a wrongly-
/// classified share fails the extraction round-trip with a clean error that
/// surfaces in `CaptureState::Error` one Retry away from recovery. Tightening
/// this classifier costs lost legitimate shares; loosening it costs one
/// recoverable round-trip. Asymmetry favors permissiveness.
fn classify_share_mime(mime: &str, filename: &str) -> Option<DocumentKind> {
    const IMAGE_SUBTYPES: &[&str] = &["jpeg", "jpg", "png", "heic", "heif", "webp"];
    let mime = mime.to_ascii_lowercase();
    if let Some(subtype) = mime.strip_prefix("image/")
        && IMAGE_SUBTYPES.contains(&subtype)
    {
        return Some(DocumentKind::Photo);
    }
    if let Some(subtype) = mime.strip_prefix("application/")
        && (subtype == "pdf" || filename.to_ascii_lowercase().ends_with(".pdf"))
    {
        return Some(DocumentKind::Pdf);
    }
    None
}

/// Which sub-view is currently active inside Finances. The variants stay
/// `Copy`; the pending extracted draft (which is not Copy) lives in a
/// separate signal alongside this enum.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FinancesView {
    Home,
    Capture(DocumentKind),
    Email,
    /// Editable confirm-draft / manual-entry form. The initial state comes
    /// from `pending_draft` — `None` is manual entry, `Some(_)` is the
    /// post-extraction confirm step.
    TransactionForm,
}

/// Top-level Finances page. Umbrella for capture flows (Phase 3), transactions
/// surface (Phase 4), workflows (Phase 5), and import (Phase 6).
#[component]
pub fn FinancesPage() -> Element {
    let mut view = use_signal(|| FinancesView::Home);
    let mut pending_draft: Signal<Option<ExtractedDraft>> = use_signal(|| None);

    // Pending Android share-target intake (Phase 3.3). main.rs sets this
    // signal when MainActivity.kt stashes shared bytes; we route to the
    // matching capture view, hand the bytes to DocumentCapture as a
    // `preloaded` prop, and clear the signal so a Back-then-forward navigation
    // doesn't replay the same share.
    let mut pending_share: Signal<Option<PendingShareCapture>> = use_context();
    use_effect(move || {
        // Snapshot + drop the read guard before any .set() — Dioxus signals
        // hold the read borrow until end-of-expression, so set/read in the
        // same statement deadlocks the borrow checker.
        let snapshot = pending_share.read().clone();
        let Some(capture) = snapshot else { return };
        match classify_share_mime(&capture.mime, &capture.filename) {
            Some(kind) => view.set(FinancesView::Capture(kind)),
            None => {
                // Unsupported MIME (e.g., text/html share) — drop the bytes;
                // user lands on Home and can pick a flow manually.
                pending_share.set(None);
            }
        }
    });

    rsx! {
        div { class: "max-w-3xl mx-auto w-full animate-in fade-in duration-300",

            match *view.read() {
                FinancesView::Home => rsx! {
                    HomeView {
                        on_open_photo: move |_| view.set(FinancesView::Capture(DocumentKind::Photo)),
                        on_open_pdf: move |_| view.set(FinancesView::Capture(DocumentKind::Pdf)),
                        on_open_email: move |_| view.set(FinancesView::Email),
                        on_open_manual: move |_| {
                            pending_draft.set(None);
                            view.set(FinancesView::TransactionForm);
                        },
                    }
                },
                FinancesView::Capture(kind) => rsx! {
                    DocumentCapture {
                        kind: kind,
                        preloaded: pending_share.read().clone(),
                        on_done: move |_| {
                            pending_share.set(None);
                            view.set(FinancesView::Home);
                        },
                        on_extracted: move |draft: ExtractedDraft| {
                            pending_share.set(None);
                            pending_draft.set(Some(draft));
                            view.set(FinancesView::TransactionForm);
                        },
                    }
                },
                FinancesView::Email => rsx! {
                    EmailCapture {
                        on_done: move |_| view.set(FinancesView::Home),
                        on_extracted: move |draft: ExtractedDraft| {
                            pending_draft.set(Some(draft));
                            view.set(FinancesView::TransactionForm);
                        },
                    }
                },
                FinancesView::TransactionForm => rsx! {
                    TransactionForm {
                        initial: pending_draft.read().clone(),
                        on_done: move |_| {
                            pending_draft.set(None);
                            view.set(FinancesView::Home);
                        },
                    }
                },
            }
        }
    }
}

#[component]
fn HomeView(
    on_open_photo: EventHandler<()>,
    on_open_pdf: EventHandler<()>,
    on_open_email: EventHandler<()>,
    on_open_manual: EventHandler<()>,
) -> Element {
    rsx! {
        h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent mb-8", "Finances" }

        // --- Capture Section ---
        div { class: "mb-10 space-y-4",
            div { class: "border-b border-white/5 pb-2 mb-4",
                h2 { class: "text-lg font-bold text-obsidian-text", "Capture a Transaction" }
                p { class: "text-xs text-obsidian-text-muted mt-1",
                    "Snap a receipt, drop a statement, paste an email, or enter manually."
                }
            }

            div { class: "grid grid-cols-2 md:grid-cols-4 gap-3",
                CaptureTile {
                    label: "Photo",
                    icon_path: "M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z M15 13a3 3 0 11-6 0 3 3 0 016 0z",
                    enabled: true,
                    on_click: move |_| on_open_photo.call(()),
                }
                CaptureTile {
                    label: "PDF",
                    icon_path: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z",
                    enabled: true,
                    on_click: move |_| on_open_pdf.call(()),
                }
                CaptureTile {
                    label: "Email",
                    icon_path: "M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z",
                    enabled: true,
                    on_click: move |_| on_open_email.call(()),
                }
                CaptureTile {
                    label: "Manual",
                    icon_path: "M12 4v16m8-8H4",
                    enabled: true,
                    on_click: move |_| on_open_manual.call(()),
                }
            }
        }

        // Placeholder until Phase 4 (transactions list) lands
        div { class: "border-b border-white/5 pb-2 mb-4",
            h2 { class: "text-lg font-bold text-obsidian-text", "Recent" }
        }
        div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
            "Transactions list lands in Phase 4."
        }
    }
}

#[component]
fn CaptureTile(
    label: &'static str,
    icon_path: &'static str,
    enabled: bool,
    on_click: EventHandler<()>,
) -> Element {
    let base = "flex flex-col items-center justify-center gap-2 p-4 bg-obsidian-sidebar border border-white/10 rounded-xl min-h-[96px] transition-colors";
    let interactive = if enabled {
        "text-obsidian-text hover:border-obsidian-accent hover:text-obsidian-accent cursor-pointer"
    } else {
        "text-obsidian-text-muted opacity-50 cursor-not-allowed"
    };
    rsx! {
        button {
            class: "{base} {interactive}",
            disabled: !enabled,
            onclick: move |_| on_click.call(()),
            svg { class: "w-7 h-7", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: icon_path }
            }
            span { class: "text-sm font-medium", "{label}" }
        }
    }
}

// =============================================================================
// Document capture sub-view (Phase 3.1 photos + 3.2 PDFs)
//
// Flow: tile click → file picker → bytes ship to /documents/extract →
// structured ExtractedDraft renders below. PDFs surface a hint selector
// because `core::extraction::route_from_mime` deliberately returns None for
// application/pdf — receipt vs bank-statement vs paystub vs brokerage is
// not auto-derivable from the MIME alone.
// =============================================================================

const PDF_HINTS: &[(&str, &str)] = &[
    ("bank_statement", "Bank statement"),
    ("brokerage_statement", "Brokerage statement"),
    ("paystub", "Paystub"),
    ("receipt", "Receipt"),
];

#[component]
fn DocumentCapture(
    kind: DocumentKind,
    /// Bytes + metadata pre-loaded from an Android share-target SEND intent.
    /// When `Some`, the file picker is hidden in favor of a "Use shared file"
    /// confirm step (Phase 3.3); when `None`, the regular file-picker flow
    /// runs unchanged.
    #[props(default = None)]
    preloaded: Option<PendingShareCapture>,
    on_done: EventHandler<()>,
    on_extracted: EventHandler<ExtractedDraft>,
) -> Element {
    #[derive(Debug, Clone)]
    enum CaptureState {
        Idle,
        Working,
        Error {
            msg: String,
            retry_bytes: Option<(Vec<u8>, String)>,
        },
    }

    let (title, accept, prefer_camera, default_hint, show_hint_picker) = match kind {
        DocumentKind::Photo => ("Photo capture", "image/*", true, "receipt", false),
        DocumentKind::Pdf => (
            "PDF capture",
            "application/pdf",
            false,
            "bank_statement",
            true,
        ),
    };

    let mut state: Signal<CaptureState> = use_signal(|| CaptureState::Idle);
    let mut hint = use_signal(|| default_hint.to_string());

    let on_file_picked = move |evt: Event<FormData>| {
        let files = evt.files();
        let Some(file) = files.into_iter().next() else {
            return;
        };
        let mime = file.content_type().unwrap_or_else(|| match kind {
            DocumentKind::Photo => "image/jpeg".to_string(),
            DocumentKind::Pdf => "application/pdf".to_string(),
        });
        let hint_value = hint.read().clone();

        state.set(CaptureState::Working);

        spawn(async move {
            let bytes = match file.read_bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => {
                    state.set(CaptureState::Error {
                        msg: format!("Couldn't read file: {e}"),
                        retry_bytes: None,
                    });
                    return;
                }
            };

            let retry_bytes = bytes.clone();
            let retry_mime = mime.clone();

            match bridge::invoke_extract_document(bytes, &mime, &hint_value).await {
                Ok(draft) => {
                    // Reset local state so a quick re-open shows the Idle
                    // prompt instead of a stale Working spinner.
                    state.set(CaptureState::Idle);
                    on_extracted.call(draft);
                }
                Err(e) => state.set(CaptureState::Error {
                    msg: format!("Couldn't extract: {e}"),
                    retry_bytes: Some((retry_bytes, retry_mime)),
                }),
            }
        });
    };

    rsx! {
        div { class: "flex flex-col gap-6",

            // Back button + title
            div { class: "flex items-center gap-3 mb-2",
                button {
                    class: "text-obsidian-text-muted hover:text-obsidian-text text-sm flex items-center gap-1",
                    onclick: move |_| on_done.call(()),
                    svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                    }
                    span { "Back" }
                }
                h1 { class: "text-xl font-bold text-obsidian-accent", "{title}" }
            }

            // Hint selector — only renders for PDFs.
            if show_hint_picker {
                fieldset { class: "flex flex-col gap-2",
                    legend { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2",
                        "What kind of PDF?"
                    }
                    div { class: "flex flex-wrap gap-2",
                        for (value, label) in PDF_HINTS.iter().copied() {
                            HintRadio {
                                value: value,
                                label: label,
                                checked: *hint.read() == value,
                                on_select: move |_| hint.set(value.to_string()),
                            }
                        }
                    }
                }
            }

            // Share-target preloaded panel — only renders when the user
            // arrived via an Android SEND intent (Phase 3.3). Surfaces the
            // shared file's metadata so the user can confirm before bytes
            // ship to Gemini; click fires the same extraction path the file
            // picker uses.
            if let Some(capture) = preloaded.clone() {
                div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg space-y-3",
                    div { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                        "Shared file"
                    }
                    div { class: "flex items-center gap-2 text-sm text-obsidian-text",
                        svg { class: "w-4 h-4 text-obsidian-accent", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" }
                        }
                        span { class: "font-mono", "{capture.filename}" }
                        span { class: "text-obsidian-text-muted", " · {capture.size} bytes · {capture.mime}" }
                    }
                    button {
                        class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed",
                        r#type: "button",
                        disabled: matches!(*state.read(), CaptureState::Working),
                        onclick: move |_| {
                            // Mirror on_file_picked's tail: bytes already in
                            // hand, skip the async read.
                            let bytes = capture.bytes.clone();
                            let mime = capture.mime.clone();
                            let hint_value = hint.read().clone();
                            let retry_bytes = bytes.clone();
                            let retry_mime = mime.clone();
                            state.set(CaptureState::Working);
                            spawn(async move {
                                match bridge::invoke_extract_document(bytes, &mime, &hint_value).await {
                                    Ok(draft) => {
                                        state.set(CaptureState::Idle);
                                        on_extracted.call(draft);
                                    }
                                    Err(e) => state.set(CaptureState::Error {
                                        msg: format!("Couldn't extract: {e}"),
                                        retry_bytes: Some((retry_bytes, retry_mime)),
                                    }),
                                }
                            });
                        },
                        "Use shared file"
                    }
                }
            } else {
                // File picker. `capture="environment"` only applies to the photo
                // flow — it tells mobile browsers to default to the rear camera.
                label { class: "block",
                    span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                        match kind {
                            DocumentKind::Photo => "Pick a photo or take one",
                            DocumentKind::Pdf => "Pick a PDF",
                        }
                    }
                    if prefer_camera {
                        input {
                            class: "block w-full text-sm text-obsidian-text file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:bg-obsidian-accent file:text-white file:font-medium hover:file:opacity-90 cursor-pointer",
                            r#type: "file",
                            accept: accept,
                            "capture": "environment",
                            onchange: on_file_picked,
                        }
                    } else {
                        input {
                            class: "block w-full text-sm text-obsidian-text file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:bg-obsidian-accent file:text-white file:font-medium hover:file:opacity-90 cursor-pointer",
                            r#type: "file",
                            accept: accept,
                            onchange: on_file_picked,
                        }
                    }
                }
            }

            // State-dependent body. The Error arm composes render_error with
            // a conditional Retry button that needs `state` in scope.
            div {
                {
                    match &*state.read() {
                        CaptureState::Idle => render_idle(kind),
                        CaptureState::Working => render_working(),
                        CaptureState::Error { msg, retry_bytes } => {
                            let retry = retry_bytes.clone();
                            let hint_value = hint.read().clone();
                            rsx! {
                                {render_error(msg)}
                                if let Some((bytes, mime)) = retry {
                                    div { class: "mt-3",
                                        button {
                                            class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90",
                                            onclick: move |_| {
                                                let bytes = bytes.clone();
                                                let mime = mime.clone();
                                                let hint_value = hint_value.clone();
                                                state.set(CaptureState::Working);
                                                spawn(async move {
                                                    let retry_bytes = bytes.clone();
                                                    let retry_mime = mime.clone();
                                                    match bridge::invoke_extract_document(bytes, &mime, &hint_value).await {
                                                        Ok(draft) => {
                                                            state.set(CaptureState::Idle);
                                                            on_extracted.call(draft);
                                                        }
                                                        Err(e) => state.set(CaptureState::Error {
                                                            msg: format!("Couldn't extract: {e}"),
                                                            retry_bytes: Some((retry_bytes, retry_mime)),
                                                        }),
                                                    }
                                                });
                                            },
                                            "Retry"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HintRadio(
    value: &'static str,
    label: &'static str,
    checked: bool,
    on_select: EventHandler<()>,
) -> Element {
    let base =
        "px-3 py-1.5 rounded-full text-xs font-medium border transition-colors cursor-pointer";
    let style = if checked {
        "bg-obsidian-accent text-white border-obsidian-accent"
    } else {
        "bg-transparent text-obsidian-text-muted border-white/10 hover:border-obsidian-accent hover:text-obsidian-text"
    };
    rsx! {
        button {
            class: "{base} {style}",
            r#type: "button",
            value: value,
            onclick: move |_| on_select.call(()),
            "{label}"
        }
    }
}

// =============================================================================
// Email body capture sub-view (Phase 3.4)
//
// A pasted email body — no file picker. User pastes the body text, clicks
// Extract, and the same /documents/extract endpoint handles it (hint=email_body,
// MIME=text/plain). Shares the state-machine pattern with DocumentCapture but
// the input shape is different enough that DRY-ing further would tangle the
// abstractions; the duplication is the bounded kind.
// =============================================================================

#[component]
fn EmailCapture(on_done: EventHandler<()>, on_extracted: EventHandler<ExtractedDraft>) -> Element {
    #[derive(Debug, Clone)]
    enum CaptureState {
        Idle,
        Working,
        Error {
            msg: String,
            retry_body: Option<String>,
        },
    }

    let mut state: Signal<CaptureState> = use_signal(|| CaptureState::Idle);
    let mut body = use_signal(String::new);

    // FnMut because state.set requires &mut on the closure. Captured by move
    // into two onclicks (Extract + Retry); the closure is Copy because all
    // captures (Signal handles + EventHandler) are Copy.
    let mut kick_off = move |body_text: String| {
        if body_text.trim().is_empty() {
            return;
        }
        state.set(CaptureState::Working);
        spawn(async move {
            let bytes = body_text.clone().into_bytes();
            let retry_body = body_text;
            match bridge::invoke_extract_document(bytes, "text/plain", "email_body").await {
                Ok(draft) => {
                    state.set(CaptureState::Idle);
                    on_extracted.call(draft);
                }
                Err(e) => state.set(CaptureState::Error {
                    msg: format!("Couldn't extract: {e}"),
                    retry_body: Some(retry_body),
                }),
            }
        });
    };

    rsx! {
        div { class: "flex flex-col gap-6",

            // Back button + title
            div { class: "flex items-center gap-3 mb-2",
                button {
                    class: "text-obsidian-text-muted hover:text-obsidian-text text-sm flex items-center gap-1",
                    onclick: move |_| on_done.call(()),
                    svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                    }
                    span { "Back" }
                }
                h1 { class: "text-xl font-bold text-obsidian-accent", "Email capture" }
            }

            // Paste area
            label { class: "block",
                span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                    "Paste the email body"
                }
                textarea {
                    class: "block w-full min-h-[200px] p-3 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted text-sm font-mono outline-none focus:border-obsidian-accent transition-colors resize-y",
                    placeholder: "Paste a receipt confirmation email, a Wise notification, anything with a charge in it…",
                    value: "{body.read()}",
                    oninput: move |e| body.set(e.value().clone()),
                }
            }

            // Extract button
            div { class: "flex justify-end",
                button {
                    class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed",
                    disabled: body.read().trim().is_empty() || matches!(*state.read(), CaptureState::Working),
                    onclick: move |_| {
                        let text = body.read().clone();
                        kick_off(text);
                    },
                    "Extract"
                }
            }

            // State-dependent body
            div {
                {
                    match &*state.read() {
                        CaptureState::Idle => rsx! {
                            p { class: "text-sm text-obsidian-text-muted",
                                "Paste a transaction-bearing email body above, then click Extract."
                            }
                        },
                        CaptureState::Working => render_working(),
                        CaptureState::Error { msg, retry_body } => {
                            let retry = retry_body.clone();
                            rsx! {
                                {render_error(msg)}
                                if let Some(text) = retry {
                                    div { class: "mt-3",
                                        button {
                                            class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90",
                                            onclick: move |_| {
                                                let text = text.clone();
                                                kick_off(text);
                                            },
                                            "Retry"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Transaction form — manual entry + confirm-draft (Phases 3.5 + 3.6)
//
// One form, two entry points:
//   - Manual tile         → initial = None,         empty form
//   - Extracted draft     → initial = Some(draft),  pre-populated
//
// Save → record_transaction Tauri command → TransactionRecorded event.
// =============================================================================

/// One editable posting row. Maps 1-1 to a backend `Posting` once the user
/// hits Save; `amount` stays a String here so the form can stage in-progress
/// text without bailing on every keystroke.
#[derive(Debug, Clone)]
struct PostingRow {
    account: String,
    commodity: String,
    amount: String,
}

impl PostingRow {
    fn empty(default_commodity: &str) -> Self {
        Self {
            account: String::new(),
            commodity: default_commodity.to_string(),
            amount: String::new(),
        }
    }
}

const DEFAULT_COMMODITY: &str = "CAD";

#[component]
fn TransactionForm(initial: Option<ExtractedDraft>, on_done: EventHandler<()>) -> Element {
    // Pre-populate from extracted draft when provided.
    let (init_date, init_desc, init_rows, init_attachment) = match initial {
        Some(d) => {
            let rows: Vec<PostingRow> = if d.postings.is_empty() {
                vec![
                    PostingRow::empty(DEFAULT_COMMODITY),
                    PostingRow::empty(DEFAULT_COMMODITY),
                ]
            } else {
                d.postings
                    .iter()
                    .map(|p| PostingRow {
                        account: p.account_hint.clone().unwrap_or_default(),
                        commodity: p.commodity.clone(),
                        amount: p.amount.clone(),
                    })
                    .collect()
            };
            (
                d.date.unwrap_or_default(),
                d.description.unwrap_or_default(),
                rows,
                d.attachment,
            )
        }
        None => (
            String::new(),
            String::new(),
            vec![
                PostingRow::empty(DEFAULT_COMMODITY),
                PostingRow::empty(DEFAULT_COMMODITY),
            ],
            None,
        ),
    };

    let mut date = use_signal(|| init_date);
    let mut description = use_signal(|| init_desc);
    let mut postings = use_signal(|| init_rows);
    let attachment: Signal<Option<AttachmentRef>> = use_signal(|| init_attachment);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let on_save = move |_| {
        if *saving.read() {
            return;
        }
        // Validate then build a TransactionFormDraft.
        let date_v = date.read().clone();
        let desc_v = description.read().clone();
        let rows = postings.read().clone();

        if date_v.trim().is_empty() {
            error.set(Some("Date is required.".into()));
            return;
        }
        if desc_v.trim().is_empty() {
            error.set(Some("Description is required.".into()));
            return;
        }
        let mut postings_out: Vec<PostingInput> = Vec::with_capacity(rows.len());
        for (i, r) in rows.iter().enumerate() {
            if r.account.trim().is_empty() && r.amount.trim().is_empty() {
                continue;
            }
            if r.account.trim().is_empty() {
                error.set(Some(format!("Row {} needs an account.", i + 1)));
                return;
            }
            if r.amount.trim().is_empty() {
                error.set(Some(format!("Row {} needs an amount.", i + 1)));
                return;
            }
            // Amount stays a String on the wire — the backend's
            // `rust_decimal::serde::str` adapter parses it. We do a quick
            // sanity-check here so a typo doesn't reach the server.
            if r.amount.parse::<f64>().is_err() {
                error.set(Some(format!(
                    "Row {}: '{}' is not a number.",
                    i + 1,
                    r.amount
                )));
                return;
            }
            postings_out.push(PostingInput {
                account: r.account.trim().to_string(),
                commodity: r.commodity.trim().to_string(),
                amount: r.amount.trim().to_string(),
                tags: Vec::new(),
            });
        }
        if postings_out.len() < 2 {
            error.set(Some(
                "At least two postings required (debit + credit).".into(),
            ));
            return;
        }

        error.set(None);
        saving.set(true);
        let submission = TransactionFormDraft {
            date: date_v,
            description: desc_v,
            postings: postings_out,
            attachment: attachment.read().clone(),
        };
        spawn(async move {
            match bridge::invoke_record_transaction(submission).await {
                Ok(()) => {
                    saving.set(false);
                    on_done.call(());
                }
                Err(e) => {
                    saving.set(false);
                    error.set(Some(format!("Save failed: {e}")));
                }
            }
        });
    };

    rsx! {
        div { class: "flex flex-col gap-6",

            // Back button + title
            div { class: "flex items-center gap-3 mb-2",
                button {
                    class: "text-obsidian-text-muted hover:text-obsidian-text text-sm flex items-center gap-1",
                    onclick: move |_| on_done.call(()),
                    svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                    }
                    span { "Cancel" }
                }
                h1 { class: "text-xl font-bold text-obsidian-accent", "Transaction" }
            }

            // Date
            div {
                label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                    "Date"
                }
                input {
                    class: "w-full px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text outline-none focus:border-obsidian-accent",
                    r#type: "date",
                    value: "{date.read()}",
                    oninput: move |e| date.set(e.value().clone()),
                }
            }

            // Description
            div {
                label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                    "Description"
                }
                input {
                    class: "w-full px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text outline-none focus:border-obsidian-accent",
                    r#type: "text",
                    placeholder: "Loblaws — Groceries",
                    value: "{description.read()}",
                    oninput: move |e| description.set(e.value().clone()),
                }
            }

            // Postings list
            div { class: "flex flex-col gap-2",
                div { class: "flex items-center justify-between",
                    span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                        "Postings"
                    }
                    button {
                        class: "text-xs text-obsidian-accent hover:opacity-80",
                        r#type: "button",
                        onclick: move |_| {
                            let mut rows = postings.read().clone();
                            rows.push(PostingRow::empty(DEFAULT_COMMODITY));
                            postings.set(rows);
                        },
                        "+ Add posting"
                    }
                }

                {
                    let rows = postings.read().clone();
                    rsx! {
                        for (idx, row) in rows.into_iter().enumerate() {
                            div { key: "{idx}", class: "flex flex-wrap gap-2 items-center",
                                input {
                                    class: "flex-1 min-w-[200px] px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-sm outline-none focus:border-obsidian-accent",
                                    r#type: "text",
                                    placeholder: "Account (e.g. Expenses:Groceries)",
                                    value: "{row.account}",
                                    oninput: move |e| {
                                        let mut rows = postings.read().clone();
                                        if let Some(r) = rows.get_mut(idx) {
                                            r.account = e.value().clone();
                                        }
                                        postings.set(rows);
                                    },
                                }
                                input {
                                    class: "w-28 px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-sm font-mono outline-none focus:border-obsidian-accent",
                                    r#type: "text",
                                    placeholder: "0.00",
                                    value: "{row.amount}",
                                    oninput: move |e| {
                                        let mut rows = postings.read().clone();
                                        if let Some(r) = rows.get_mut(idx) {
                                            r.amount = e.value().clone();
                                        }
                                        postings.set(rows);
                                    },
                                }
                                input {
                                    class: "w-20 px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-sm font-mono outline-none focus:border-obsidian-accent uppercase",
                                    r#type: "text",
                                    placeholder: "CAD",
                                    value: "{row.commodity}",
                                    oninput: move |e| {
                                        let mut rows = postings.read().clone();
                                        if let Some(r) = rows.get_mut(idx) {
                                            r.commodity = e.value().clone();
                                        }
                                        postings.set(rows);
                                    },
                                }
                                button {
                                    class: "text-xs text-obsidian-text-muted hover:text-red-300 px-2 py-1",
                                    r#type: "button",
                                    disabled: postings.read().len() <= 2,
                                    onclick: move |_| {
                                        let mut rows = postings.read().clone();
                                        if rows.len() > 2 {
                                            rows.remove(idx);
                                            postings.set(rows);
                                        }
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }
            }

            // Attachment indicator — surfaces the fact that bytes were
            // persisted server-side and mirrored to the on-device LRU cache.
            // Thumbnail rendering is Phase 4 (transaction detail) territory.
            if let Some(att) = attachment.read().clone() {
                div { class: "p-3 bg-obsidian-sidebar/60 border border-white/10 rounded-md text-xs text-obsidian-text-muted flex items-center gap-2",
                    svg { class: "w-4 h-4 text-obsidian-accent", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" }
                    }
                    span { "Attachment saved · " span { class: "font-mono", "{att.filename}" } " · {att.size} bytes" }
                }
            }

            // Error display
            if let Some(msg) = error.read().clone() {
                div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded-md text-sm text-red-300",
                    "{msg}"
                }
            }

            // Save
            div { class: "flex justify-end gap-2",
                button {
                    class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed",
                    r#type: "button",
                    disabled: *saving.read(),
                    onclick: on_save,
                    if *saving.read() { "Saving…" } else { "Save transaction" }
                }
            }
        }
    }
}

// --- Render helpers ----------------------------------------------------------

fn render_idle(kind: DocumentKind) -> Element {
    let msg = match kind {
        DocumentKind::Photo => {
            "Pick a photo above to start. Mobile devices open the camera; desktop opens a file picker."
        }
        DocumentKind::Pdf => {
            "Pick a PDF above. Choose the document kind first so the extractor uses the right prompt."
        }
    };
    rsx! {
        p { class: "text-sm text-obsidian-text-muted", "{msg}" }
    }
}

fn render_working() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-4 bg-obsidian-sidebar/60 border border-white/5 rounded-lg",
            div { class: "w-4 h-4 border-2 border-obsidian-accent border-t-transparent rounded-full animate-spin" }
            span { class: "text-sm text-obsidian-text-muted", "Extracting transaction details…" }
        }
    }
}

fn render_error(message: &str) -> Element {
    rsx! {
        div { class: "p-4 bg-red-950/30 border border-red-500/30 rounded-lg space-y-2",
            p { class: "text-sm text-red-300", "Couldn't extract: {message}" }
            p { class: "text-xs text-obsidian-text-muted", "Pick another file above to retry." }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_share_mime_routes_known_image_subtypes() {
        assert_eq!(classify_share_mime("image/jpeg", "r.jpg"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("image/png", "r.png"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("image/heic", "r.heic"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("image/heif", "r.heif"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("image/webp", "r.webp"), Some(DocumentKind::Photo));
    }

    #[test]
    fn classify_share_mime_is_case_insensitive() {
        assert_eq!(classify_share_mime("Image/JPEG", "r.jpg"), Some(DocumentKind::Photo));
        assert_eq!(classify_share_mime("APPLICATION/PDF", "s"), Some(DocumentKind::Pdf));
    }

    #[test]
    fn classify_share_mime_accepts_well_declared_pdf_regardless_of_filename() {
        // application/pdf wins even when filename has no .pdf extension —
        // covers the renamed-PDF case.
        assert_eq!(classify_share_mime("application/pdf", "statement"), Some(DocumentKind::Pdf));
    }

    #[test]
    fn classify_share_mime_rescues_stripped_mime_via_filename() {
        // The canonical Android share-target case: ContentProvider couldn't
        // sniff the MIME, MainActivity.kt fell back to octet-stream, but the
        // filename extension is still intact.
        assert_eq!(
            classify_share_mime("application/octet-stream", "chequing-march.pdf"),
            Some(DocumentKind::Pdf),
        );
    }

    #[test]
    fn classify_share_mime_trusts_legacy_pdf_mime_with_filename() {
        // application/x-pdf is a legacy non-standard PDF MIME; permissiveness
        // here is intentional (see doc-comment on classify_share_mime).
        assert_eq!(
            classify_share_mime("application/x-pdf", "s.pdf"),
            Some(DocumentKind::Pdf),
        );
    }

    #[test]
    fn classify_share_mime_rejects_text_family_even_with_pdf_filename() {
        // text/* shares belong in EmailCapture — refusing here drops the
        // share gracefully and lets the user pick the right flow manually.
        assert_eq!(classify_share_mime("text/html", "r.pdf"), None);
        assert_eq!(classify_share_mime("text/plain", "anything"), None);
    }

    #[test]
    fn classify_share_mime_rejects_unknown_image_subtype() {
        // Image allowlist is intentional — image/tiff or image/svg+xml
        // aren't realistic receipt formats and Gemini's photo extraction
        // is calibrated for the listed subtypes.
        assert_eq!(classify_share_mime("image/tiff", "r.tiff"), None);
        assert_eq!(classify_share_mime("image/svg+xml", "r.svg"), None);
    }
}
