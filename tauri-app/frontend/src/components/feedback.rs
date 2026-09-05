//! The capture modal — one sentence, visible context, send.
//!
//! **Mounted only while open**, rather than rendered always and class-toggled
//! the way `NavDrawer` is. The drawer needs the always-rendered form so its
//! slide can animate from off-screen; this needs the opposite, because mounting
//! *is* the snapshot: the description of the screen underneath is taken in
//! `use_hook` at mount and never re-read, so what the user sees listed is
//! exactly what gets sent even if a background sync changes the page beneath the
//! scrim while they are typing.
//!
//! **No category picker, no severity, no triage state.** Those are reading-side
//! concerns, and the reader here is a person at a terminal working through a
//! markdown list. Every field a capture form adds is another reason to close it
//! and carry on being annoyed instead.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::primitives::{Banner, BannerKind, Button, ButtonVariant};
use crate::screen_context::use_screen_report;
use crate::types::AppContext;

/// One line of the "attached automatically" list.
struct ContextLine {
    text: String,
    /// Whether the user can drop this line before sending. Only content that
    /// may quote their own writing is droppable; build identity is not, because
    /// a report that hides which version it came from is not worth filing.
    droppable: bool,
}

#[component]
pub fn FeedbackModal(on_close: EventHandler<()>) -> Element {
    let report_signal = use_screen_report();
    // Snapshot at mount — see the module note. `use_hook` runs exactly once.
    let snapshot = use_hook(|| report_signal.peek().clone());

    // `use_signal` + `use_future`, matching the app shell's runtime-profile
    // fetch, rather than `use_resource`. A resource read through
    // `read_unchecked` does not subscribe the component, so the build line
    // would stay blank whenever the IPC round trip lost the race with first
    // paint — which is the normal case, not the edge one.
    let mut app_ctx = use_signal(|| None::<AppContext>);
    use_future(move || async move {
        if let Ok(ctx) = bridge::invoke_get_app_context().await {
            app_ctx.set(Some(ctx));
        }
    });

    let mut body = use_signal(String::new);
    let mut include_detail = use_signal(|| true);
    let mut sending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut sent_id = use_signal(|| None::<String>);

    let has_body = !body.read().trim().is_empty();
    let is_sending = *sending.read();

    // Context lines, assembled for display. The same values are what `submit`
    // sends, so the list is not a summary of the payload — it IS the payload.
    let ctx_snapshot: Option<AppContext> = app_ctx.read().clone();

    let mut lines: Vec<ContextLine> = Vec::new();
    if !snapshot.screen.is_empty() {
        let where_text = match &snapshot.screen_ref {
            Some(r) => format!("{} · {r}", pretty_screen(&snapshot.screen)),
            None => pretty_screen(&snapshot.screen),
        };
        lines.push(ContextLine {
            text: where_text,
            droppable: false,
        });
    }
    if let Some(c) = &ctx_snapshot {
        let mut build = format!("v{} · {} · {}", c.app_version, c.platform, c.device_id);
        // Name the sandbox, don't just flag it — a test run is usually one of
        // several, and "which data root" is the first thing a reader asks. On a
        // real run the path is noise, so it stays hidden.
        if c.non_production {
            build.push_str(" · SANDBOX ");
            build.push_str(&c.data_dir);
        }
        lines.push(ContextLine {
            text: build,
            droppable: false,
        });
    }
    if let Some(detail) = &snapshot.detail {
        lines.push(ContextLine {
            text: detail.clone(),
            droppable: true,
        });
    }

    let submit = move |_| {
        if !has_body || is_sending {
            return;
        }
        sending.set(true);
        error.set(None);
        let text = body.read().clone();
        let screen = (!snapshot.screen.is_empty()).then(|| snapshot.screen.clone());
        let screen_ref = snapshot.screen_ref.clone();
        // Honour the toggle at send time, not at display time — the user may
        // flip it after reading what the line says.
        let detail = include_detail
            .read()
            .then(|| snapshot.detail.clone())
            .flatten();
        spawn(async move {
            let result = bridge::invoke_submit_feedback(
                &text,
                screen.as_deref(),
                screen_ref.as_deref(),
                detail.as_deref(),
                // Populated once the diagnostic ring buffer exists.
                &[],
            )
            .await;
            match result {
                Ok(id) => sent_id.set(Some(id)),
                Err(e) => error.set(Some(e)),
            }
            sending.set(false);
        });
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[200] bg-black/60 animate-in fade-in duration-150",
            onclick: move |_| on_close.call(()),
        }
        div {
            class: "fixed inset-x-0 bottom-0 md:inset-0 md:m-auto z-[210] w-full md:max-w-lg md:h-fit \
                    bg-obsidian-sidebar border-t md:border border-white/10 md:rounded-xl \
                    px-4 pt-4 flex flex-col gap-4 animate-in fade-in slide-in-from-bottom-4 duration-200",
            // Clear the Android gesture bar; the sheet is bottom-anchored on mobile.
            style: "padding-bottom: calc(1rem + var(--safe-area-inset-bottom));",

            div { class: "flex items-center justify-between",
                h2 { class: "text-base font-bold text-obsidian-text", "Report a problem" }
                button {
                    class: "w-8 h-8 flex items-center justify-center rounded-md text-obsidian-text-muted hover:bg-white/5 hover:text-obsidian-text transition-colors",
                    "aria-label": "Close",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            if let Some(id) = sent_id.read().clone() {
                Banner { kind: BannerKind::Success,
                    div {
                        p { "Report filed. It syncs with everything else." }
                        p { class: "font-mono text-[11px] opacity-70 mt-1", "{id}" }
                    }
                }
                div { class: "flex justify-end",
                    Button { onclick: move |_| on_close.call(()), "Done" }
                }
            } else {
                textarea {
                    class: "w-full min-h-[110px] px-3 py-2 bg-obsidian-bg border border-white/10 rounded-lg \
                            text-obsidian-text text-sm outline-none focus:border-obsidian-accent transition-colors resize-y",
                    placeholder: "What happened?",
                    autofocus: true,
                    disabled: is_sending,
                    value: "{body}",
                    oninput: move |e| body.set(e.value()),
                }

                if !lines.is_empty() {
                    div { class: "space-y-1.5",
                        p { class: "text-[10px] uppercase tracking-[0.15em] text-obsidian-text-muted",
                            "Attached automatically"
                        }
                        for (i , line) in lines.iter().enumerate() {
                            div {
                                key: "{i}",
                                class: "flex items-start gap-2 text-xs text-obsidian-text-muted",
                                span { class: "text-obsidian-accent leading-4", "•" }
                                span {
                                    class: if line.droppable && !*include_detail.read() {
                                        "flex-1 line-through opacity-40 break-words"
                                    } else {
                                        "flex-1 break-words"
                                    },
                                    "{line.text}"
                                }
                                if line.droppable {
                                    button {
                                        class: "shrink-0 px-1.5 rounded text-obsidian-text-muted hover:bg-white/5 hover:text-obsidian-text transition-colors",
                                        "aria-label": "Toggle attaching this",
                                        onclick: move |_| {
                                            let current = *include_detail.read();
                                            include_detail.set(!current);
                                        },
                                        if *include_detail.read() { "✕" } else { "＋" }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(e) = error.read().clone() {
                    Banner { kind: BannerKind::Error, "{e}" }
                }

                div { class: "flex justify-end gap-2 pt-1",
                    Button {
                        variant: ButtonVariant::Secondary,
                        disabled: is_sending,
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    Button {
                        disabled: !has_body || is_sending,
                        onclick: submit,
                        if is_sending { "Sending…" } else { "Send" }
                    }
                }
            }
        }
    }
}

/// Turn a `tab:subview` coordinate into something readable. Unknown shapes fall
/// through unchanged rather than being dropped — a coordinate a describer just
/// introduced is still more useful in the report than a blank.
fn pretty_screen(screen: &str) -> String {
    let (tab, sub) = match screen.split_once(':') {
        Some((t, s)) => (t, Some(s)),
        None => (screen, None),
    };
    let tab_label = match tab {
        "journal" => "Journal",
        "notes" => "Notes",
        "routines" => "Routines",
        "finances" => "Finances",
        "settings" => "Settings",
        other => other,
    };
    match sub {
        Some(s) => format!("{tab_label} → {s}"),
        None => tab_label.to_string(),
    }
}
