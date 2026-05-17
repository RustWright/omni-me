use dioxus::prelude::*;

use crate::bridge;
use crate::types::ExtractedDraft;

/// Which sub-view is currently active inside Finances. Simple two-state
/// today (tile grid vs. photo capture); will grow as 3.2-3.6 land and
/// each capture method gets its own sub-view.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FinancesView {
    Home,
    PhotoCapture,
}

/// Top-level Finances page. Umbrella for capture flows (Phase 3), transactions
/// surface (Phase 4), workflows (Phase 5), and import (Phase 6). Currently
/// renders the entry-tile grid and routes into the Photo capture sub-view —
/// PDF / Email / Manual tiles remain disabled placeholders until 3.2-3.5.
#[component]
pub fn FinancesPage() -> Element {
    let mut view = use_signal(|| FinancesView::Home);

    rsx! {
        div { class: "max-w-3xl mx-auto w-full animate-in fade-in duration-300",

            match *view.read() {
                FinancesView::Home => rsx! { HomeView { on_open_photo: move |_| view.set(FinancesView::PhotoCapture) } },
                FinancesView::PhotoCapture => rsx! { PhotoCapture { on_done: move |_| view.set(FinancesView::Home) } },
            }
        }
    }
}

#[component]
fn HomeView(on_open_photo: EventHandler<()>) -> Element {
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
                CaptureTile { label: "PDF",    icon_path: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z", enabled: false, on_click: move |_| {} }
                CaptureTile { label: "Email",  icon_path: "M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z", enabled: false, on_click: move |_| {} }
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
// Photo capture sub-view (Phase 3.1)
//
// Flow: tap "Photo" tile → file picker (mobile triggers rear camera) → user
// picks a photo → bytes ship to the server's /documents/extract endpoint →
// Gemini returns a structured ExtractedDraft → we show the postings preview.
// The "Save → TransactionRecorded" path lands in 3.6 (confirm-draft screen);
// for 3.1 we just render the draft read-only with a "Looks good?" affordance.
// =============================================================================

#[component]
fn PhotoCapture(on_done: EventHandler<()>) -> Element {
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

    // `mut` is the Dioxus convention even though Signal has interior
    // mutability — the borrow-checker still wants it for `.set()` calls.
    let mut state: Signal<CaptureState> = use_signal(|| CaptureState::Idle);

    // File picker handler. Reads bytes via Dioxus 0.7's `FileData::read_bytes`
    // and kicks off the extract round trip.
    let on_file_picked = move |evt: Event<FormData>| {
        let files = evt.files();
        let Some(file) = files.into_iter().next() else {
            return;
        };
        let mime = file
            .content_type()
            .unwrap_or_else(|| "image/jpeg".to_string());

        // Drive the signal — this updates the runtime AND triggers a re-render
        // of every component (including ours) reading `state`.
        state.set(CaptureState::Working);

        // `state` (the Signal handle) is `Copy`, so the `move` below copies the
        // handle into the async block — both the outer closure and the async
        // task can call `.set()` on the same underlying reactive slot.
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

            // `invoke_extract_document` takes `bytes` by value (moves it). If
            // we want them for one-tap retry on failure, we have to clone the
            // bytes BEFORE the call — once `bytes` is gone, it's gone. Same
            // logic for `mime` (we still own it after the `&mime` borrow, but
            // constructing the Error variant moves it).
            let retry_bytes = bytes.clone();
            let retry_mime = mime.clone();

            match bridge::invoke_extract_document(bytes, &mime, "receipt").await {
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
                h1 { class: "text-xl font-bold text-obsidian-accent", "Photo capture" }
            }

            // File picker. `capture="environment"` is a mobile hint that
            // makes Android open the rear-facing camera by default.
            label { class: "block",
                span { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block",
                    "Pick a photo or take one"
                }
                input {
                    class: "block w-full text-sm text-obsidian-text file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:bg-obsidian-accent file:text-white file:font-medium hover:file:opacity-90 cursor-pointer",
                    r#type: "file",
                    accept: "image/*",
                    "capture": "environment",
                    onchange: on_file_picked,
                }
            }

            // State-dependent body. Three of the four arms delegate to the
            // pure-display `render_*` helpers; the Error arm composes the
            // error display with a conditional Retry button (which needs the
            // `state` signal in scope to mutate it on click).
            div {
                {
                    match &*state.read() {
                        CaptureState::Idle => render_idle(),
                        CaptureState::Working => render_working(),
                        CaptureState::Draft(d) => render_draft(d),
                        CaptureState::Error { msg, retry_bytes } => {
                            let retry = retry_bytes.clone();
                            rsx! {
                                {render_error(msg)}
                                if let Some((bytes, mime)) = retry {
                                    div { class: "mt-3",
                                        button {
                                            class: "px-4 py-2 bg-obsidian-accent text-white text-sm font-medium rounded-md hover:opacity-90",
                                            onclick: move |_| {
                                                // Double-clone idiom: outer captures by move (taking ownership
                                                // of the originals from the match arm), inner clones for the
                                                // async block. Keeps the onclick callable as FnMut.
                                                let bytes = bytes.clone();
                                                let mime = mime.clone();
                                                state.set(CaptureState::Working);
                                                spawn(async move {
                                                    let retry_bytes = bytes.clone();
                                                    let retry_mime = mime.clone();
                                                    match bridge::invoke_extract_document(bytes, &mime, "receipt").await {
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

// --- Render helpers ----------------------------------------------------------

fn render_idle() -> Element {
    rsx! {
        p { class: "text-sm text-obsidian-text-muted",
            "Pick a photo above to start. Mobile devices open the camera; desktop opens a file picker."
        }
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
            p { class: "text-xs text-obsidian-text-muted", "Pick another photo above to retry." }
        }
    }
}
