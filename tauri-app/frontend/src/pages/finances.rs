use dioxus::prelude::*;

use crate::bridge;
use crate::types::ExtractedDraft;

/// Which kind of file-based capture the user opened. Drives the picker
/// `accept` filter, the camera hint, the title, and whether the hint
/// selector is offered (PDFs require a user pick; photos default to receipt).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DocumentKind {
    Photo,
    Pdf,
}

/// Which sub-view is currently active inside Finances.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FinancesView {
    Home,
    Capture(DocumentKind),
    Email,
}

/// Top-level Finances page. Umbrella for capture flows (Phase 3), transactions
/// surface (Phase 4), workflows (Phase 5), and import (Phase 6).
#[component]
pub fn FinancesPage() -> Element {
    let mut view = use_signal(|| FinancesView::Home);

    rsx! {
        div { class: "max-w-3xl mx-auto w-full animate-in fade-in duration-300",

            match *view.read() {
                FinancesView::Home => rsx! {
                    HomeView {
                        on_open_photo: move |_| view.set(FinancesView::Capture(DocumentKind::Photo)),
                        on_open_pdf: move |_| view.set(FinancesView::Capture(DocumentKind::Pdf)),
                        on_open_email: move |_| view.set(FinancesView::Email),
                    }
                },
                FinancesView::Capture(kind) => rsx! {
                    DocumentCapture {
                        kind: kind,
                        on_done: move |_| view.set(FinancesView::Home),
                    }
                },
                FinancesView::Email => rsx! {
                    EmailCapture { on_done: move |_| view.set(FinancesView::Home) }
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
                CaptureTile { label: "Manual", icon_path: "M12 4v16m8-8H4", enabled: false, on_click: move |_| {} }
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
fn DocumentCapture(kind: DocumentKind, on_done: EventHandler<()>) -> Element {
    #[derive(Debug, Clone)]
    enum CaptureState {
        Idle,
        Working,
        Draft(ExtractedDraft),
        Error {
            msg: String,
            retry_bytes: Option<(Vec<u8>, String)>,
        },
    }

    let (title, accept, prefer_camera, default_hint, show_hint_picker) = match kind {
        DocumentKind::Photo => ("Photo capture", "image/*", true, "receipt", false),
        DocumentKind::Pdf => ("PDF capture", "application/pdf", false, "bank_statement", true),
    };

    let mut state: Signal<CaptureState> = use_signal(|| CaptureState::Idle);
    let mut hint = use_signal(|| default_hint.to_string());

    let on_file_picked = move |evt: Event<FormData>| {
        let files = evt.files();
        let Some(file) = files.into_iter().next() else {
            return;
        };
        let mime = file
            .content_type()
            .unwrap_or_else(|| match kind {
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
                Ok(draft) => state.set(CaptureState::Draft(draft)),
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

            // State-dependent body. The Error arm composes render_error with
            // a conditional Retry button that needs `state` in scope.
            div {
                {
                    match &*state.read() {
                        CaptureState::Idle => render_idle(kind),
                        CaptureState::Working => render_working(),
                        CaptureState::Draft(d) => render_draft(d),
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
                                                        Ok(draft) => state.set(CaptureState::Draft(draft)),
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
    let base = "px-3 py-1.5 rounded-full text-xs font-medium border transition-colors cursor-pointer";
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
fn EmailCapture(on_done: EventHandler<()>) -> Element {
    #[derive(Debug, Clone)]
    enum CaptureState {
        Idle,
        Working,
        Draft(ExtractedDraft),
        Error { msg: String, retry_body: Option<String> },
    }

    let mut state: Signal<CaptureState> = use_signal(|| CaptureState::Idle);
    let mut body = use_signal(String::new);

    // FnMut because state.set requires &mut on the closure. Captured by move
    // into two onclicks (Extract + Retry); the closure is Copy because all
    // captures (Signal handles) are Copy, so each onclick gets its own copy.
    let mut kick_off = move |body_text: String| {
        if body_text.trim().is_empty() {
            return;
        }
        state.set(CaptureState::Working);
        spawn(async move {
            let bytes = body_text.clone().into_bytes();
            let retry_body = body_text;
            match bridge::invoke_extract_document(bytes, "text/plain", "email_body").await {
                Ok(draft) => state.set(CaptureState::Draft(draft)),
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
                        CaptureState::Draft(d) => render_draft(d),
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

// --- Render helpers ----------------------------------------------------------

fn render_idle(kind: DocumentKind) -> Element {
    let msg = match kind {
        DocumentKind::Photo => "Pick a photo above to start. Mobile devices open the camera; desktop opens a file picker.",
        DocumentKind::Pdf => "Pick a PDF above. Choose the document kind first so the extractor uses the right prompt.",
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

fn render_draft(draft: &ExtractedDraft) -> Element {
    rsx! {
        div { class: "p-4 bg-obsidian-sidebar/60 border border-white/5 rounded-lg space-y-3",
            div { class: "flex items-baseline justify-between",
                h3 { class: "text-base font-semibold text-obsidian-text",
                    {draft.description.clone().unwrap_or_else(|| "Untitled".into())}
                }
                span { class: "text-xs text-obsidian-text-muted",
                    "{(draft.confidence * 100.0).round() as i64}% confidence"
                }
            }
            if let Some(date) = &draft.date {
                p { class: "text-xs text-obsidian-text-muted", "Date: {date}" }
            }
            ul { class: "divide-y divide-white/5",
                for posting in &draft.postings {
                    li { class: "py-2 flex items-center justify-between text-sm",
                        span { class: "text-obsidian-text",
                            {posting.account_hint.clone().unwrap_or_else(|| "<account?>".into())}
                        }
                        span { class: "font-mono text-obsidian-text-muted",
                            "{posting.amount} {posting.commodity}"
                        }
                    }
                }
            }
            p { class: "text-xs text-obsidian-text-muted italic",
                "Confirm-draft screen (3.6) lands next — for now, this is read-only."
            }
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
