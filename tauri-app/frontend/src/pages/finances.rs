use dioxus::prelude::*;

use crate::bridge;
use crate::types::{
    AttachmentRef, DraftTransactionView, ExtractedDraft, PendingBatchView, PendingShareCapture,
    PostingInput, TransactionFormDraft, TransactionView, TxnFilter,
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
    /// List of pending auto-import batches awaiting review (Phase 3.10.6).
    BatchList,
    /// Per-batch review screen. Selected `batch_id` is carried in the
    /// `selected_batch_id` signal alongside the view enum (the enum stays
    /// `Copy` so this variant can't carry the String inline).
    BatchReview,
    /// Committed-transactions browse screen (Phase 4.1).
    TransactionList,
    /// Single-transaction detail screen with attachment viewer (Phase 4.2).
    /// Selected `txn_id` rides along in `selected_txn_id` (variant stays Copy).
    TransactionDetail,
}

/// Top-level Finances page. Umbrella for capture flows (Phase 3), transactions
/// surface (Phase 4), workflows (Phase 5), and import (Phase 6).
#[component]
pub fn FinancesPage() -> Element {
    let mut view = use_signal(|| FinancesView::Home);
    let mut pending_draft: Signal<Option<ExtractedDraft>> = use_signal(|| None);
    let mut selected_batch_id: Signal<Option<String>> = use_signal(|| None);
    let mut selected_txn_id: Signal<Option<String>> = use_signal(|| None);
    // Pending-batch count, refreshed every time the user lands on Home. A
    // separate signal (not derived from listing the batches inline) keeps the
    // Home banner cheap — one COUNT query instead of a full SELECT every
    // navigation.
    let mut pending_batch_count: Signal<u64> = use_signal(|| 0);
    let _refresh_count_resource = use_resource(move || {
        let in_home = matches!(*view.read(), FinancesView::Home);
        async move {
            if !in_home {
                return;
            }
            if let Ok(batches) = bridge::invoke_list_pending_batches().await {
                pending_batch_count.set(batches.len() as u64);
            }
        }
    });

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
                        pending_count: *pending_batch_count.read(),
                        on_open_photo: move |_| view.set(FinancesView::Capture(DocumentKind::Photo)),
                        on_open_pdf: move |_| view.set(FinancesView::Capture(DocumentKind::Pdf)),
                        on_open_email: move |_| view.set(FinancesView::Email),
                        on_open_manual: move |_| {
                            pending_draft.set(None);
                            view.set(FinancesView::TransactionForm);
                        },
                        on_open_batches: move |_| view.set(FinancesView::BatchList),
                        on_open_transactions: move |_| view.set(FinancesView::TransactionList),
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
                FinancesView::BatchList => rsx! {
                    BatchListView {
                        on_back: move |_| view.set(FinancesView::Home),
                        on_open_batch: move |batch_id: String| {
                            selected_batch_id.set(Some(batch_id));
                            view.set(FinancesView::BatchReview);
                        },
                    }
                },
                FinancesView::BatchReview => {
                    let bid = selected_batch_id.read().clone();
                    match bid {
                        Some(batch_id) => rsx! {
                            BatchReviewView {
                                batch_id: batch_id,
                                on_done: move |_| {
                                    selected_batch_id.set(None);
                                    view.set(FinancesView::BatchList);
                                },
                            }
                        },
                        None => rsx! {
                            // Shouldn't happen — BatchReview is only set
                            // alongside selected_batch_id, but if it does the
                            // user can navigate back to the list cleanly.
                            div {
                                class: "text-obsidian-text-muted text-sm",
                                "No batch selected. "
                                button {
                                    class: "underline",
                                    onclick: move |_| view.set(FinancesView::BatchList),
                                    "Back to list"
                                }
                            }
                        },
                    }
                }
                FinancesView::TransactionList => rsx! {
                    TransactionListView {
                        on_back: move |_| view.set(FinancesView::Home),
                        on_open_txn: move |txn_id: String| {
                            selected_txn_id.set(Some(txn_id));
                            view.set(FinancesView::TransactionDetail);
                        },
                    }
                },
                FinancesView::TransactionDetail => {
                    let tid = selected_txn_id.read().clone();
                    match tid {
                        Some(txn_id) => rsx! {
                            TransactionDetailView {
                                txn_id: txn_id,
                                on_back: move |_| {
                                    selected_txn_id.set(None);
                                    view.set(FinancesView::TransactionList);
                                },
                            }
                        },
                        None => rsx! {
                            div {
                                class: "text-obsidian-text-muted text-sm",
                                "No transaction selected. "
                                button {
                                    class: "underline",
                                    onclick: move |_| view.set(FinancesView::TransactionList),
                                    "Back to list"
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn HomeView(
    pending_count: u64,
    on_open_photo: EventHandler<()>,
    on_open_pdf: EventHandler<()>,
    on_open_email: EventHandler<()>,
    on_open_manual: EventHandler<()>,
    on_open_batches: EventHandler<()>,
    on_open_transactions: EventHandler<()>,
) -> Element {
    rsx! {
        h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent mb-8", "Finances" }

        if pending_count > 0 {
            button {
                class: "w-full mb-6 px-4 py-3 bg-obsidian-accent/10 border border-obsidian-accent/40 rounded-lg flex items-center justify-between hover:bg-obsidian-accent/15 transition-colors",
                onclick: move |_| on_open_batches.call(()),
                div { class: "flex items-center gap-3",
                    span { class: "inline-flex items-center justify-center w-8 h-8 bg-obsidian-accent text-black rounded-full text-sm font-bold",
                        "{pending_count}"
                    }
                    div { class: "text-left",
                        div { class: "text-sm font-semibold text-obsidian-text",
                            if pending_count == 1 {
                                "1 auto-imported batch awaiting review"
                            } else {
                                "{pending_count} auto-imported batches awaiting review"
                            }
                        }
                        div { class: "text-xs text-obsidian-text-muted",
                            "Tap to accept, skip, or dismiss."
                        }
                    }
                }
                svg { class: "w-5 h-5 text-obsidian-text-muted",
                    fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                        d: "M9 5l7 7-7 7"
                    }
                }
            }
        }

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

        // --- Recent transactions section ---
        div { class: "border-b border-white/5 pb-2 mb-4",
            h2 { class: "text-lg font-bold text-obsidian-text", "Recent" }
        }
        button {
            class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
            onclick: move |_| on_open_transactions.call(()),
            div {
                div { class: "text-sm font-semibold text-obsidian-text", "View transactions" }
                div { class: "text-xs text-obsidian-text-muted mt-1",
                    "Browse everything you've recorded."
                }
            }
            svg { class: "w-5 h-5 text-obsidian-text-muted",
                fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                    d: "M9 5l7 7-7 7"
                }
            }
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

// =============================================================================
// Auto-import batch review (Phase 3.10.6)
//
// BatchListView: lists pending batches (one row per source/dedup) with quick
// metadata. Click → BatchReviewView.
//
// BatchReviewView: per-row accept toggle; if any row carries a commodity in
// MANUAL_FX_CURRENCIES (i.e., NGN today), prompts for the manual base→quote
// rate before Commit. Commit fans out to TransactionRecorded × N + optional
// ExchangeRateRecorded + AutoImportBatchCommitted via `commit_batch`.
// =============================================================================

/// Currencies that need a user-supplied FX rate because Frankfurter doesn't
/// cover them. Mirror of `core::fx::MANUAL_FX_CURRENCIES`; kept in sync via
/// the cross-crate type round-trip test in `core/src/fx.rs`. UI-side dup is
/// pragmatic — making this WASM-shared would pull `core` into the frontend.
const MANUAL_FX_CURRENCIES_UI: &[&str] = &["NGN"];

fn batch_needs_manual_fx(batch: &PendingBatchView) -> Option<String> {
    for draft in &batch.draft_postings {
        for posting in &draft.postings {
            for commodity in MANUAL_FX_CURRENCIES_UI {
                if posting.commodity.eq_ignore_ascii_case(commodity) {
                    return Some((*commodity).to_string());
                }
            }
        }
    }
    None
}

#[component]
fn BatchListView(
    on_back: EventHandler<()>,
    on_open_batch: EventHandler<String>,
) -> Element {
    let mut batches: Signal<Option<Result<Vec<PendingBatchView>, String>>> =
        use_signal(|| None);

    use_effect(move || {
        spawn(async move {
            batches.set(Some(bridge::invoke_list_pending_batches().await));
        });
    });

    rsx! {
        div { class: "flex items-center justify-between mb-6",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Auto-import review"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        match batches.read().clone() {
            None => rsx! {
                div { class: "text-obsidian-text-muted text-sm", "Loading pending batches…" }
            },
            Some(Err(msg)) => rsx! {
                div { class: "p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                    "Failed to load pending batches: {msg}"
                }
            },
            Some(Ok(rows)) if rows.is_empty() => rsx! {
                div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                    "No pending auto-import batches. Captured transactions and confirmed batches show up in Recent (Phase 4)."
                }
            },
            Some(Ok(rows)) => rsx! {
                div { class: "space-y-2",
                    for batch in rows {
                        BatchListRow {
                            key: "{batch.batch_id}",
                            batch: batch.clone(),
                            on_open: {
                                let id = batch.batch_id.clone();
                                let handler = on_open_batch;
                                move |_| handler.call(id.clone())
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BatchListRow(batch: PendingBatchView, on_open: EventHandler<()>) -> Element {
    let row_count = batch.draft_postings.len();
    let fx_hint = batch_needs_manual_fx(&batch);
    let source_label = pretty_source(&batch.source);
    let fetched_short = batch
        .fetched_at
        .split('T')
        .next()
        .unwrap_or(&batch.fetched_at)
        .to_string();

    rsx! {
        button {
            class: "w-full p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg flex items-center justify-between hover:border-obsidian-accent/40 transition-colors text-left",
            onclick: move |_| on_open.call(()),
            div { class: "flex-1 min-w-0",
                div { class: "flex items-baseline gap-2 mb-1",
                    span { class: "text-sm font-semibold text-obsidian-text", "{source_label}" }
                    span { class: "text-xs text-obsidian-text-muted", "· {fetched_short}" }
                    if let Some(c) = fx_hint {
                        span { class: "text-xs px-2 py-0.5 bg-amber-500/15 text-amber-300 rounded-full",
                            "needs {c} rate"
                        }
                    }
                }
                div { class: "text-xs text-obsidian-text-muted truncate",
                    if row_count == 1 { "1 transaction" } else { "{row_count} transactions" }
                }
            }
            svg { class: "w-5 h-5 text-obsidian-text-muted shrink-0",
                fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                    d: "M9 5l7 7-7 7"
                }
            }
        }
    }
}

fn pretty_source(source: &str) -> &str {
    match source {
        "wise" => "Wise",
        "wealthsimple" | "wealthsimple-snaptrade" => "WealthSimple",
        "sc_ngn" | "imap-standardchartered-ngn" => "Standard Chartered (NGN)",
        "imap_receipts" | "imap-receipts" | "receipts" => "Email receipts",
        other => other,
    }
}

#[component]
fn BatchReviewView(batch_id: String, on_done: EventHandler<()>) -> Element {
    let batch_id_for_resource = batch_id.clone();
    let mut batch: Signal<Option<Result<PendingBatchView, String>>> = use_signal(|| None);
    let mut accepted: Signal<Vec<bool>> = use_signal(Vec::new);
    let mut fx_rate_input: Signal<String> = use_signal(String::new);
    let mut busy: Signal<bool> = use_signal(|| false);
    let mut feedback: Signal<Option<String>> = use_signal(|| None);

    use_effect(move || {
        let id = batch_id_for_resource.clone();
        spawn(async move {
            match bridge::invoke_list_pending_batches().await {
                Ok(rows) => match rows.into_iter().find(|b| b.batch_id == id) {
                    Some(found) => {
                        accepted.set(vec![true; found.draft_postings.len()]);
                        batch.set(Some(Ok(found)));
                    }
                    None => batch.set(Some(Err(format!(
                        "Batch {id} no longer pending — it may have been resolved on another device."
                    )))),
                },
                Err(e) => batch.set(Some(Err(e))),
            }
        });
    });

    let current = batch.read().clone();
    match current {
        None => rsx! {
            div { class: "text-obsidian-text-muted text-sm", "Loading batch…" }
        },
        Some(Err(msg)) => rsx! {
            div { class: "space-y-4",
                div { class: "p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                    "{msg}"
                }
                button {
                    class: "text-sm text-obsidian-text-muted hover:text-obsidian-text underline",
                    onclick: move |_| on_done.call(()),
                    "← Back to list"
                }
            }
        },
        Some(Ok(b)) => {
            let manual_fx_commodity = batch_needs_manual_fx(&b);
            let source_label = pretty_source(&b.source).to_string();
            let row_count = b.draft_postings.len();
            let accepted_count = accepted.read().iter().filter(|x| **x).count();
            let metadata_pretty = b.source_metadata.as_ref().and_then(|v| {
                serde_json::to_string_pretty(v).ok()
            });

            rsx! {
                div { class: "flex items-center justify-between mb-4",
                    div {
                        h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                            "{source_label}"
                        }
                        p { class: "text-xs text-obsidian-text-muted mt-1",
                            "Fetched {b.fetched_at} · {row_count} draft transactions"
                        }
                    }
                    button {
                        class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                        onclick: move |_| on_done.call(()),
                        "← List"
                    }
                }

                if let Some(meta_str) = metadata_pretty {
                    details { class: "mb-4 text-xs text-obsidian-text-muted",
                        summary { class: "cursor-pointer hover:text-obsidian-text", "Source metadata" }
                        pre { class: "mt-2 p-3 bg-obsidian-sidebar/60 rounded border border-white/5 overflow-x-auto",
                            "{meta_str}"
                        }
                    }
                }

                div { class: "space-y-2 mb-6",
                    for (idx, draft) in b.draft_postings.iter().enumerate() {
                        DraftRow {
                            key: "{draft.external_id}",
                            idx: idx,
                            draft: draft.clone(),
                            accepted: accepted.read().get(idx).copied().unwrap_or(true),
                            on_toggle: move |_| {
                                let mut current_accepted = accepted.write();
                                if let Some(slot) = current_accepted.get_mut(idx) {
                                    *slot = !*slot;
                                }
                            },
                        }
                    }
                }

                if let Some(commodity) = manual_fx_commodity.clone() {
                    div { class: "mb-6 p-4 bg-amber-500/10 border border-amber-500/30 rounded-lg",
                        label { class: "block text-sm font-semibold text-amber-200 mb-2",
                            "Manual FX rate (1 {commodity} = ? CAD)"
                        }
                        p { class: "text-xs text-obsidian-text-muted mb-3",
                            "Required because {commodity} is outside the daily-rate provider's coverage. The rate is recorded as an hledger P directive at the batch's effective date."
                        }
                        input {
                            r#type: "text",
                            inputmode: "decimal",
                            placeholder: "e.g., 0.00088",
                            class: "w-full px-3 py-2 bg-obsidian-bg border border-white/10 rounded text-obsidian-text",
                            value: "{fx_rate_input.read()}",
                            oninput: move |evt| fx_rate_input.set(evt.value()),
                        }
                    }
                }

                div { class: "flex gap-3 items-center",
                    button {
                        class: "flex-1 px-4 py-3 bg-obsidian-accent text-black font-semibold rounded-lg hover:bg-obsidian-accent/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *busy.read() || accepted_count == 0
                            || (manual_fx_commodity.is_some() && fx_rate_input.read().trim().is_empty()),
                        onclick: {
                            let batch_id = b.batch_id.clone();
                            let manual_fx_commodity = manual_fx_commodity.clone();
                            move |_| {
                                let batch_id = batch_id.clone();
                                let manual_fx_commodity = manual_fx_commodity.clone();
                                let accepted_indices: Vec<usize> = accepted
                                    .read()
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, on)| if *on { Some(i) } else { None })
                                    .collect();
                                let rate = fx_rate_input.read().trim().to_string();
                                spawn(async move {
                                    busy.set(true);
                                    feedback.set(None);
                                    let (fx_rate, fx_commodity) = match manual_fx_commodity {
                                        Some(c) if !rate.is_empty() => (Some(rate), Some(c)),
                                        _ => (None, None),
                                    };
                                    let res = bridge::invoke_commit_batch(
                                        &batch_id,
                                        accepted_indices,
                                        fx_rate,
                                        fx_commodity,
                                    )
                                    .await;
                                    busy.set(false);
                                    match res {
                                        Ok(_) => on_done.call(()),
                                        Err(e) => feedback.set(Some(format!("Commit failed: {e}"))),
                                    }
                                });
                            }
                        },
                        if *busy.read() {
                            "Committing…"
                        } else if accepted_count == row_count {
                            "Commit all {accepted_count}"
                        } else {
                            "Commit {accepted_count} of {row_count}"
                        }
                    }
                    button {
                        class: "px-4 py-3 bg-obsidian-sidebar border border-white/10 text-obsidian-text-muted rounded-lg hover:text-obsidian-text hover:border-red-500/40 hover:text-red-300 transition-colors disabled:opacity-50",
                        disabled: *busy.read(),
                        onclick: {
                            let batch_id = b.batch_id.clone();
                            move |_| {
                                let batch_id = batch_id.clone();
                                spawn(async move {
                                    busy.set(true);
                                    feedback.set(None);
                                    let res = bridge::invoke_dismiss_batch(&batch_id, None).await;
                                    busy.set(false);
                                    match res {
                                        Ok(()) => on_done.call(()),
                                        Err(e) => feedback.set(Some(format!("Dismiss failed: {e}"))),
                                    }
                                });
                            }
                        },
                        "Dismiss"
                    }
                }

                if let Some(msg) = feedback.read().clone() {
                    div { class: "mt-4 p-3 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                        "{msg}"
                    }
                }
            }
        }
    }
}

#[component]
fn DraftRow(
    idx: usize,
    draft: DraftTransactionView,
    accepted: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let border = if accepted {
        "border-obsidian-accent/40"
    } else {
        "border-white/5 opacity-60"
    };
    rsx! {
        div { class: "p-3 bg-obsidian-sidebar/60 border {border} rounded-lg",
            div { class: "flex items-start gap-3",
                input {
                    r#type: "checkbox",
                    class: "mt-1",
                    checked: accepted,
                    onchange: move |_| on_toggle.call(()),
                }
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-baseline gap-2 mb-1",
                        span { class: "text-xs text-obsidian-text-muted font-mono", "#{idx + 1}" }
                        span { class: "text-sm font-medium text-obsidian-text", "{draft.date}" }
                    }
                    div { class: "text-sm text-obsidian-text truncate mb-2", "{draft.description}" }
                    div { class: "space-y-1",
                        for posting in &draft.postings {
                            div { class: "flex justify-between gap-3 text-xs",
                                span { class: "text-obsidian-text-muted font-mono truncate", "{posting.account}" }
                                span { class: "text-obsidian-text font-mono shrink-0",
                                    "{posting.amount} {posting.commodity}"
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
// Transaction list (Phase 4.1)
//
// Browse-first slice: paginated `list_transactions` reads, no filters yet,
// rows are non-clickable (4.2 lands the detail view). Filter chips +
// click-through both layer on top of this shell without restructuring.
// =============================================================================

const TXN_PAGE_SIZE: u32 = 50;

/// Minimal posting view extracted from a `TransactionView.postings` JSON
/// array. The full backend `Posting` carries `fx_rate` + `tags` too; rows
/// only need the at-a-glance fields. Detail view (4.2) will re-decode the
/// full shape.
#[derive(Debug, Clone)]
struct PostingRowView {
    account: String,
    amount: String,
    commodity: String,
}

/// Decode `TransactionView.postings` (serde_json::Value, FLEXIBLE on the
/// backend) into a Vec of display rows. Postings missing required fields
/// are silently dropped — `record_transaction` validates upstream so this
/// is defensive only.
fn posting_views(value: &serde_json::Value) -> Vec<PostingRowView> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(PostingRowView {
                        account: p.get("account")?.as_str()?.to_string(),
                        amount: p.get("amount")?.as_str()?.to_string(),
                        commodity: p.get("commodity")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[component]
fn TransactionListView(
    on_back: EventHandler<()>,
    on_open_txn: EventHandler<String>,
) -> Element {
    let mut transactions: Signal<Vec<TransactionView>> = use_signal(Vec::new);
    let mut loading: Signal<bool> = use_signal(|| true);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut has_more: Signal<bool> = use_signal(|| false);
    let mut offset: Signal<u32> = use_signal(|| 0);
    // `active_filter` is what the current page reflects. `draft_filter` is
    // what the FilterBar inputs hold; only Apply copies draft → active.
    // Keeping them separate means typing in a field doesn't fire queries
    // and accidental edits don't invalidate the current page until Apply.
    let mut active_filter: Signal<TxnFilter> = use_signal(TxnFilter::default);
    let draft_filter: Signal<TxnFilter> = use_signal(TxnFilter::default);

    // Load (or re-load) the first page whenever active_filter changes.
    // Re-deriving via use_effect keeps the dependency wiring honest — the
    // effect re-runs on any signal read inside it.
    use_effect(move || {
        let filter = active_filter.read().clone();
        spawn(async move {
            loading.set(true);
            error.set(None);
            match bridge::invoke_list_transactions(filter, TXN_PAGE_SIZE, 0).await {
                Ok(rows) => {
                    has_more.set(rows.len() as u32 == TXN_PAGE_SIZE);
                    offset.set(rows.len() as u32);
                    transactions.set(rows);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

    let load_more = move |_| {
        if *loading.read() {
            return;
        }
        let current_offset = *offset.read();
        let filter = active_filter.read().clone();
        spawn(async move {
            loading.set(true);
            match bridge::invoke_list_transactions(filter, TXN_PAGE_SIZE, current_offset).await {
                Ok(rows) => {
                    has_more.set(rows.len() as u32 == TXN_PAGE_SIZE);
                    let mut all = transactions.read().clone();
                    all.extend(rows);
                    offset.set(all.len() as u32);
                    transactions.set(all);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    let rows = transactions.read().clone();
    let is_loading = *loading.read();
    let err_msg = error.read().clone();
    let show_empty = rows.is_empty() && !is_loading && err_msg.is_none();
    let filter_active = !active_filter.read().is_empty();

    rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Transactions"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← Back"
            }
        }

        FilterBar {
            draft: draft_filter,
            on_apply: move |applied: TxnFilter| {
                active_filter.set(applied);
            },
            on_clear: move |_| {
                draft_filter.clone().set(TxnFilter::default());
                active_filter.set(TxnFilter::default());
            },
            filter_active: filter_active,
        }

        if let Some(msg) = err_msg {
            div { class: "mb-4 p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "Failed to load transactions: {msg}"
            }
        }

        if show_empty {
            div { class: "p-6 bg-obsidian-sidebar/60 border border-white/5 rounded-lg text-center text-obsidian-text-muted text-sm",
                if filter_active {
                    "No transactions match these filters."
                } else {
                    "No transactions yet. Capture one from the Finances home."
                }
            }
        } else {
            div { class: "space-y-2",
                for txn in rows {
                    TransactionListRow {
                        key: "{txn.id}",
                        txn: txn.clone(),
                        on_click: {
                            let id = txn.id.clone();
                            let handler = on_open_txn;
                            move |_| handler.call(id.clone())
                        },
                    }
                }
            }

            if is_loading {
                div { class: "mt-4 p-4 text-center text-obsidian-text-muted text-sm",
                    "Loading…"
                }
            } else if *has_more.read() {
                div { class: "mt-4 flex justify-center",
                    button {
                        class: "px-4 py-2 bg-obsidian-sidebar border border-white/10 text-obsidian-text-muted text-sm rounded-md hover:border-obsidian-accent/40 hover:text-obsidian-text transition-colors",
                        onclick: load_more,
                        "Load more"
                    }
                }
            }
        }
    }
}

/// Filter inputs above the list. `draft` is held by the parent so typing
/// doesn't fire requests; only Apply copies into the parent's `active_filter`
/// signal. Clear resets both the inputs and the active filter.
#[component]
fn FilterBar(
    draft: Signal<TxnFilter>,
    on_apply: EventHandler<TxnFilter>,
    on_clear: EventHandler<()>,
    filter_active: bool,
) -> Element {
    let input_class = "px-2 py-1.5 bg-obsidian-sidebar border border-white/10 rounded-md text-obsidian-text text-xs outline-none focus:border-obsidian-accent placeholder-obsidian-text-muted";
    let date_val_from = draft.read().date_from.clone().unwrap_or_default();
    let date_val_to = draft.read().date_to.clone().unwrap_or_default();
    let account_val = draft.read().account.clone().unwrap_or_default();
    let category_val = draft.read().category.clone().unwrap_or_default();
    let tag_val = draft.read().tag.clone().unwrap_or_default();

    rsx! {
        div { class: "mb-4 p-3 bg-obsidian-sidebar/40 border border-white/5 rounded-lg",
            div { class: "flex flex-wrap items-end gap-2",
                label { class: "flex flex-col gap-1",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "From" }
                    input {
                        r#type: "date",
                        class: "{input_class}",
                        value: "{date_val_from}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.date_from = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                label { class: "flex flex-col gap-1",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "To" }
                    input {
                        r#type: "date",
                        class: "{input_class}",
                        value: "{date_val_to}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.date_to = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                label { class: "flex flex-col gap-1 flex-1 min-w-[140px]",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "Account contains" }
                    input {
                        r#type: "text",
                        placeholder: "e.g. Groceries",
                        class: "{input_class}",
                        value: "{account_val}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.account = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                label { class: "flex flex-col gap-1 flex-1 min-w-[120px]",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "Category" }
                    input {
                        r#type: "text",
                        placeholder: "exact",
                        class: "{input_class}",
                        value: "{category_val}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.category = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                label { class: "flex flex-col gap-1 flex-1 min-w-[120px]",
                    span { class: "text-[10px] text-obsidian-text-muted uppercase tracking-widest", "Tag" }
                    input {
                        r#type: "text",
                        placeholder: "exact",
                        class: "{input_class}",
                        value: "{tag_val}",
                        oninput: move |e| {
                            let v = e.value();
                            let mut next = draft.read().clone();
                            next.tag = if v.is_empty() { None } else { Some(v) };
                            draft.clone().set(next);
                        },
                    }
                }
                div { class: "flex gap-2 items-center",
                    button {
                        class: "px-3 py-1.5 bg-obsidian-accent text-black text-xs font-semibold rounded-md hover:opacity-90 transition-opacity",
                        r#type: "button",
                        onclick: move |_| on_apply.call(draft.read().clone()),
                        "Apply"
                    }
                    if filter_active {
                        button {
                            class: "px-3 py-1.5 text-xs text-obsidian-text-muted hover:text-obsidian-text underline",
                            r#type: "button",
                            onclick: move |_| on_clear.call(()),
                            "Clear"
                        }
                    }
                }
            }
        }
    }
}

/// One row in the transaction list. Ledger-style: every posting line is
/// shown so the user sees exactly where money moved. Header carries
/// description + all four state signals (category, tags, cleared,
/// attachment) and wraps naturally on narrow screens so the description
/// never truncates. Clicking anywhere on the row routes to the detail view.
#[component]
fn TransactionListRow(txn: TransactionView, on_click: EventHandler<()>) -> Element {
    let postings = posting_views(&txn.postings);
    let has_attachment = txn.attachment.is_some();
    rsx! {
        button {
            r#type: "button",
            class: "block w-full text-left p-3 bg-obsidian-sidebar/60 border border-white/5 rounded-lg hover:border-obsidian-accent/40 transition-colors",
            onclick: move |_| on_click.call(()),
            // Header — flex-wrap so trailing chips/icons spill to a second
            // line on mobile rather than pushing description off-screen.
            div { class: "flex flex-wrap items-center gap-x-2 gap-y-1 mb-2",
                span { class: "text-xs text-obsidian-text-muted font-mono shrink-0",
                    "{txn.date}"
                }
                span { class: "text-sm text-obsidian-text", "{txn.description}" }
                if let Some(cat) = txn.category.clone() {
                    span { class: "text-[10px] px-2 py-0.5 bg-obsidian-accent/15 text-obsidian-accent rounded-full font-medium",
                        "{cat}"
                    }
                }
                for tag in txn.tags_top.iter().cloned() {
                    span { class: "text-[10px] px-2 py-0.5 bg-white/5 text-obsidian-text-muted rounded-full font-mono",
                        "#{tag}"
                    }
                }
                if txn.cleared {
                    span {
                        class: "text-xs text-emerald-400",
                        title: "Reconciled against a statement",
                        "✓"
                    }
                }
                if has_attachment {
                    svg {
                        class: "w-3.5 h-3.5 text-obsidian-text-muted",
                        fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        title { "Has attachment" }
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                            d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13"
                        }
                    }
                }
            }
            // Postings — one row per leg, account left-truncated, amount
            // right-aligned. Mono font keeps decimals visually stacked.
            div { class: "space-y-0.5 pl-3",
                for posting in postings {
                    div { class: "flex justify-between gap-3 text-xs font-mono",
                        span { class: "text-obsidian-text-muted truncate",
                            "{posting.account}"
                        }
                        span { class: "text-obsidian-text shrink-0",
                            "{posting.amount} {posting.commodity}"
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// Transaction detail view (Phase 4.2)
//
// Read-only view of one transaction's full payload plus an attachment viewer.
// Edit (category + tag) lands in Phase 4.3. Attachment rendering uses
// `URL.createObjectURL(Blob)` so large PDFs don't bloat the DOM via base64
// data URIs; the URL is revoked when the component unmounts via a guard.
// =============================================================================

/// Owns a blob: URL for an attachment and revokes it on Drop so the WebView
/// doesn't accumulate orphaned object URLs across navigations. Holding this
/// inside a `Signal<Option<ObjectUrlGuard>>` ties the URL's lifetime to the
/// detail view component.
struct ObjectUrlGuard(String);

impl ObjectUrlGuard {
    fn from_bytes(bytes: &[u8], mime: &str) -> Result<Self, String> {
        use wasm_bindgen::JsCast;
        let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        arr.copy_from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&arr.buffer());
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(
            parts.unchecked_ref(),
            &opts,
        )
        .map_err(|e| format!("blob construct: {e:?}"))?;
        let url = web_sys::Url::create_object_url_with_blob(&blob)
            .map_err(|e| format!("object url: {e:?}"))?;
        Ok(Self(url))
    }

    fn url(&self) -> &str {
        &self.0
    }
}

impl Drop for ObjectUrlGuard {
    fn drop(&mut self) {
        let _ = web_sys::Url::revoke_object_url(&self.0);
    }
}

/// What kind of inline rendering the attachment supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttachmentRender {
    Image,
    Pdf,
    Other,
}

fn classify_attachment(mime: &str) -> AttachmentRender {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("image/") {
        AttachmentRender::Image
    } else if mime == "application/pdf" || mime == "application/x-pdf" {
        AttachmentRender::Pdf
    } else {
        AttachmentRender::Other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttachmentMeta {
    sha256: String,
    filename: String,
    mime_type: String,
    size: u64,
}

/// Pull the four `AttachmentRef` fields out of the serde_json::Value the
/// backend ships. Returns None when the field is null or any required key
/// is missing — defensive only; `record_transaction` validates upstream.
fn extract_attachment_meta(value: &serde_json::Value) -> Option<AttachmentMeta> {
    Some(AttachmentMeta {
        sha256: value.get("sha256")?.as_str()?.to_string(),
        filename: value.get("filename")?.as_str()?.to_string(),
        mime_type: value.get("mime_type")?.as_str()?.to_string(),
        size: value.get("size")?.as_u64().unwrap_or(0),
    })
}

#[component]
fn TransactionDetailView(txn_id: String, on_back: EventHandler<()>) -> Element {
    let mut txn: Signal<Option<Result<TransactionView, String>>> = use_signal(|| None);
    let txn_id_for_load = txn_id.clone();

    use_effect(move || {
        let id = txn_id_for_load.clone();
        spawn(async move {
            let res = bridge::invoke_get_transaction(&id).await;
            let mapped = match res {
                Ok(Some(t)) => Ok(t),
                Ok(None) => Err(format!(
                    "Transaction {id} not found. It may have been deleted or merged on another device."
                )),
                Err(e) => Err(e),
            };
            txn.set(Some(mapped));
        });
    });

    let header = rsx! {
        div { class: "flex items-center justify-between mb-4",
            h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent",
                "Transaction"
            }
            button {
                class: "text-sm text-obsidian-text-muted hover:text-obsidian-text",
                onclick: move |_| on_back.call(()),
                "← List"
            }
        }
    };

    match txn.read().clone() {
        None => rsx! {
            {header}
            div { class: "text-obsidian-text-muted text-sm", "Loading…" }
        },
        Some(Err(msg)) => rsx! {
            {header}
            div { class: "p-4 bg-red-950/30 border border-red-500/30 rounded-lg text-sm text-red-300",
                "{msg}"
            }
        },
        Some(Ok(t)) => rsx! {
            {header}
            TransactionDetailBody { txn: t }
        },
    }
}

#[component]
fn TransactionDetailBody(txn: TransactionView) -> Element {
    let postings = posting_views(&txn.postings);
    let attachment_meta = txn.attachment.as_ref().and_then(extract_attachment_meta);

    rsx! {
        div { class: "space-y-6",
            // --- Metadata block ---
            div { class: "p-4 bg-obsidian-sidebar/60 border border-white/10 rounded-lg",
                div { class: "flex flex-wrap items-baseline gap-x-3 gap-y-1 mb-3",
                    span { class: "text-sm text-obsidian-text-muted font-mono",
                        "{txn.date}"
                    }
                    h2 { class: "text-lg font-semibold text-obsidian-text",
                        "{txn.description}"
                    }
                }
                div { class: "flex flex-wrap gap-2",
                    if let Some(cat) = txn.category.clone() {
                        span { class: "text-xs px-2 py-0.5 bg-obsidian-accent/15 text-obsidian-accent rounded-full font-medium",
                            "{cat}"
                        }
                    }
                    for tag in txn.tags_top.iter().cloned() {
                        span { class: "text-xs px-2 py-0.5 bg-white/5 text-obsidian-text-muted rounded-full font-mono",
                            "#{tag}"
                        }
                    }
                    if txn.cleared {
                        span { class: "text-xs px-2 py-0.5 bg-emerald-500/15 text-emerald-300 rounded-full",
                            "✓ Cleared"
                            if let Some(date) = txn.cleared_date.clone() {
                                " · {date}"
                            }
                        }
                    }
                }
                if let Some(src) = txn.statement_source.clone() {
                    div { class: "mt-3 text-xs text-obsidian-text-muted",
                        "Statement: "
                        span { class: "font-mono", "{src}" }
                    }
                }
            }

            // --- Postings ---
            div {
                h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2",
                    "Postings"
                }
                div { class: "space-y-1 p-3 bg-obsidian-sidebar/40 border border-white/5 rounded-lg",
                    for posting in postings {
                        div { class: "flex justify-between gap-3 text-sm font-mono",
                            span { class: "text-obsidian-text-muted truncate",
                                "{posting.account}"
                            }
                            span { class: "text-obsidian-text shrink-0",
                                "{posting.amount} {posting.commodity}"
                            }
                        }
                    }
                }
            }

            // --- Attachment ---
            if let Some(meta) = attachment_meta {
                AttachmentViewer { meta: meta }
            }
        }
    }
}

#[component]
fn AttachmentViewer(meta: AttachmentMeta) -> Element {
    let mut url_guard: Signal<Option<ObjectUrlGuard>> = use_signal(|| None);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let sha256 = meta.sha256.clone();
    let mime = meta.mime_type.clone();
    use_effect(move || {
        let sha = sha256.clone();
        let mime = mime.clone();
        spawn(async move {
            match bridge::invoke_fetch_attachment(&sha).await {
                Ok(bytes) => match ObjectUrlGuard::from_bytes(&bytes, &mime) {
                    Ok(guard) => url_guard.set(Some(guard)),
                    Err(e) => error.set(Some(e)),
                },
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let render = classify_attachment(&meta.mime_type);
    let size_kb = (meta.size as f64 / 1024.0).round() as u64;

    rsx! {
        div {
            h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2",
                "Attachment"
            }
            div { class: "p-3 bg-obsidian-sidebar/40 border border-white/5 rounded-lg space-y-3",
                div { class: "flex items-center gap-2 text-xs text-obsidian-text-muted",
                    svg { class: "w-3.5 h-3.5 text-obsidian-accent",
                        fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2",
                            d: "M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13"
                        }
                    }
                    span { class: "font-mono text-obsidian-text", "{meta.filename}" }
                    span { " · {size_kb} KB · {meta.mime_type}" }
                }

                if let Some(msg) = error.read().clone() {
                    div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded text-xs text-red-300",
                        "Couldn't load attachment: {msg}"
                    }
                } else {
                    match url_guard.read().as_ref().map(|g| g.url().to_string()) {
                        None => rsx! {
                            div { class: "text-xs text-obsidian-text-muted", "Loading attachment…" }
                        },
                        Some(url) => match render {
                            AttachmentRender::Image => rsx! {
                                img {
                                    src: "{url}",
                                    alt: "{meta.filename}",
                                    class: "max-w-full max-h-[600px] rounded border border-white/10",
                                }
                            },
                            AttachmentRender::Pdf => rsx! {
                                iframe {
                                    src: "{url}",
                                    class: "w-full h-[600px] rounded border border-white/10 bg-white",
                                    title: "{meta.filename}",
                                }
                            },
                            AttachmentRender::Other => rsx! {
                                a {
                                    href: "{url}",
                                    download: "{meta.filename}",
                                    class: "inline-block px-3 py-1.5 text-xs bg-obsidian-accent text-black font-medium rounded hover:opacity-90",
                                    "Download {meta.filename}"
                                }
                            },
                        },
                    }
                }
            }
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

    // --- Phase 4.2 attachment-render classifier --------------------------

    #[test]
    fn classify_attachment_routes_images_to_image_render() {
        assert_eq!(classify_attachment("image/jpeg"), AttachmentRender::Image);
        assert_eq!(classify_attachment("image/png"), AttachmentRender::Image);
        assert_eq!(classify_attachment("IMAGE/HEIC"), AttachmentRender::Image);
    }

    #[test]
    fn classify_attachment_routes_pdf_variants_to_pdf_render() {
        assert_eq!(classify_attachment("application/pdf"), AttachmentRender::Pdf);
        assert_eq!(classify_attachment("APPLICATION/PDF"), AttachmentRender::Pdf);
        assert_eq!(classify_attachment("application/x-pdf"), AttachmentRender::Pdf);
    }

    #[test]
    fn classify_attachment_falls_back_to_other_for_unknown_mime() {
        assert_eq!(classify_attachment("text/plain"), AttachmentRender::Other);
        assert_eq!(classify_attachment("application/zip"), AttachmentRender::Other);
        assert_eq!(classify_attachment(""), AttachmentRender::Other);
    }

    #[test]
    fn extract_attachment_meta_decodes_complete_ref() {
        let v = serde_json::json!({
            "sha256": "abc123",
            "filename": "receipt.jpg",
            "mime_type": "image/jpeg",
            "size": 1024,
        });
        let meta = extract_attachment_meta(&v).unwrap();
        assert_eq!(meta.sha256, "abc123");
        assert_eq!(meta.filename, "receipt.jpg");
        assert_eq!(meta.mime_type, "image/jpeg");
        assert_eq!(meta.size, 1024);
    }

    #[test]
    fn extract_attachment_meta_returns_none_when_required_field_missing() {
        let v = serde_json::json!({
            "filename": "x.pdf",
            "mime_type": "application/pdf",
            "size": 5000,
        });
        assert!(extract_attachment_meta(&v).is_none(), "missing sha256 should fail");
    }
}
