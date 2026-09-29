//! The document archive — everything filed, and what anyone has worked out
//! about it.
//!
//! Two views. The list answers *find me the document about X*; the detail view
//! answers *is this value right*, and that second question is why the layout is
//! what it is.
//!
//! ⛔ **A field is only editable beside the document it came from.** The detail
//! view renders the document and its fields together, and correction is offered
//! nowhere else. That is not a layout preference: a correction lands
//! `verified: true`, and the only thing that makes the flag honest is that the
//! person could see what the document actually says. A "fix these values" queue
//! divorced from the documents would turn the flag into a lie at scale.

use dioxus::prelude::*;

use crate::bridge;
use crate::components::attachment_viewer::{AttachmentViewer, document_meta};
use crate::components::tag_editor::TagChipEditor;
use crate::types::{DocumentField, DocumentItem};
use crate::use_page_back;

/// The MIME type an archived email carries. ⛔ Must match `archive::mime_for`'s
/// `"eml"` arm — the archive writes it and this filters on it, and nothing else
/// connects the two.
const MAIL_MIME: &str = "message/rfc822";

/// The field key carrying a document's tag set. ⛔ Must match
/// `events::DOCUMENT_TAGS_KEY` — this crate cannot see it, and the consequence of
/// a mismatch is that `FieldPanel` starts offering the joined tag value as an
/// editable text row again.
const TAGS_FIELD_KEY: &str = "tags";

/// Rows per list call.
///
/// ⚠️ The page size is the caller's, deliberately: a short page is how this
/// screen knows it has reached the end, and that inference only holds if it
/// chose the number. The command clamps it to its own ceiling.
const DOC_PAGE: u32 = 50;

#[derive(Clone, PartialEq)]
enum View {
    List,
    Detail(String),
    /// The purge queue for one tag group, optionally narrowed to what is past
    /// that tag's retention (RFC3339).
    ///
    /// ⛔ A nested view on this page, never the assistant inbox: approval lives
    /// with the domain the thing belongs to, and these are archive documents.
    /// ⛔ The cutoff travels from `RetentionGroup::cutoff` untouched — re-deriving
    /// it here would let the screen and the server disagree about the set.
    Purge(String, Option<String>),
}

#[component]
pub fn ArchivePage() -> Element {
    let mut view = use_signal(|| View::List);
    let mut search = use_signal(String::new);
    let mut kind = use_signal(String::new);
    // ⚠️ Scoping to mail filters on `mime_type`, not `kind`. `kind` comes from
    // field extraction, which has not run for mail and may never — a kind-based
    // mail filter would return nothing and look like an empty archive.
    let mut mail_only = use_signal(|| false);
    let mut tag = use_signal(String::new);
    let mut retention_open = use_signal(|| false);
    let mut reload = use_signal(|| 0u32);
    // The badge's set, reachable. ⛔ Without this the count on the nav and the
    // assistant's reminder row name a queue with no way in — the list has no
    // other way to tell a counted document from the rest, and past one page they
    // are not on screen at all.
    let mut unverified_only = use_signal(|| false);

    // A reminder row that named this tab asked for its queue, not its root.
    let mut nav_intent = crate::use_nav_intent();
    use_effect(move || {
        // Snapshot before any .set() — a read guard held across a write to the
        // same signal deadlocks (the pattern `pending_share` documents).
        let intent = *nav_intent.read();
        if intent == Some(crate::NavIntent::ArchiveUnverified) {
            nav_intent.set(None);
            unverified_only.set(true);
            view.set(View::List);
        }
    });

    // Hardware back pops the detail view before it leaves the tab.
    use_page_back(
        move || match *view.read() {
            View::List => 0,
            View::Detail(_) | View::Purge(_, _) => 1,
        },
        move || view.set(View::List),
    );

    // Pages past the first, appended as the user asks for them. ⛔ Reset whenever
    // a filter moves, or rows fetched under the old one survive into the new
    // result — see the effect below.
    let mut extra_pages: Signal<Vec<DocumentItem>> = use_signal(Vec::new);
    let mut loading_more = use_signal(|| false);
    let mut page_error: Signal<Option<String>> = use_signal(|| None);
    let mut exhausted = use_signal(|| false);

    let documents = use_resource(move || {
        let q = search.read().clone();
        let k = kind.read().clone();
        let m = if *mail_only.read() {
            MAIL_MIME.to_string()
        } else {
            String::new()
        };
        let t = tag.read().clone();
        let u = *unverified_only.read();
        let _ = reload.read();
        async move {
            bridge::invoke_list_documents(
                Some(q),
                Some(k),
                Some(m),
                Some(t),
                Some(u),
                Some(DOC_PAGE),
                Some(0),
            )
            .await
        }
    });

    // Every filter the resource above keys on, read again here so a change to any
    // of them drops the accumulated pages. ⚠️ Reading them in the resource's
    // closure does not do this — that closure re-runs to fetch page 0, it does
    // not know page 1..n exist.
    use_effect(move || {
        let _ = (
            search.read().clone(),
            kind.read().clone(),
            tag.read().clone(),
            *mail_only.read(),
            *unverified_only.read(),
            *reload.read(),
        );
        extra_pages.write().clear();
        exhausted.set(false);
        page_error.set(None);
    });

    let kinds = use_resource(move || async move { bridge::invoke_document_kinds().await });
    // Re-read on `reload` so tagging a document from the detail view adds its new
    // tag to this menu; otherwise the tag just applied is unselectable until the
    // tab is left and re-entered.
    let tags = use_resource(move || {
        let _ = reload.read();
        async move { bridge::invoke_document_tags().await }
    });

    rsx! {
        div { class: "h-full overflow-y-auto",
            match view.read().clone() {
                View::Purge(tag, cutoff) => rsx! {
                    PurgeGroup {
                        tag: tag,
                        archived_before: cutoff,
                        on_back: move |_| view.set(View::List),
                        on_purged: move |_| {
                            reload += 1;
                            view.set(View::List);
                        },
                    }
                },
                View::Detail(id) => rsx! {
                    DocumentDetail {
                        document_id: id,
                        on_back: move |_| view.set(View::List),
                        on_corrected: move |_| reload += 1,
                        on_open: move |id: String| view.set(View::Detail(id)),
                    }
                },
                View::List => rsx! {
                    div { class: "p-4 space-y-4 max-w-5xl mx-auto",
                        h1 { class: "text-lg font-semibold text-obsidian-text", "Archive" }

                        div { class: "flex flex-wrap gap-2",
                            input {
                                r#type: "search",
                                placeholder: "Search names, titles, and what's inside…",
                                value: "{search}",
                                oninput: move |e| search.set(e.value()),
                                class: "flex-1 min-w-[12rem] px-3 py-2 text-sm rounded-lg bg-obsidian-sidebar/60 \
                                        border border-obsidian-border/10 text-obsidian-text \
                                        placeholder:text-obsidian-text-muted/60 focus:outline-none \
                                        focus:border-obsidian-accent/40",
                            }
                            select {
                                value: "{kind}",
                                onchange: move |e| kind.set(e.value()),
                                class: "px-3 py-2 text-sm rounded-lg bg-obsidian-sidebar/60 \
                                        border border-obsidian-border/10 text-obsidian-text \
                                        focus:outline-none focus:border-obsidian-accent/40",
                                option { value: "", "All kinds" }
                                if let Some(Ok(list)) = kinds.read().as_ref() {
                                    for k in list.iter() {
                                        option { key: "{k}", value: "{k}", "{humanise(k)}" }
                                    }
                                }
                            }
                            // A menu rather than a text box. Tags are exact-match,
                            // so a typed "reciept" returns an empty archive and
                            // looks like a missing document rather than a typo.
                            select {
                                value: "{tag}",
                                onchange: move |e| tag.set(e.value()),
                                class: "px-3 py-2 text-sm rounded-lg bg-obsidian-sidebar/60 \
                                        border border-obsidian-border/10 text-obsidian-text \
                                        focus:outline-none focus:border-obsidian-accent/40",
                                option { value: "", "All tags" }
                                if let Some(Ok(list)) = tags.read().as_ref() {
                                    for t in list.iter() {
                                        option { key: "{t}", value: "{t}", "{t}" }
                                    }
                                }
                            }
                            // ⛔ Only offered once a tag is chosen, and it names
                            // that tag. A bare "Purge…" with no group in view
                            // would be an irreversible action whose scope the
                            // user has to infer.
                            if !tag.read().is_empty() {
                                button {
                                    onclick: move |_| {
                                        let t = tag.read().clone();
                                        view.set(View::Purge(t, None));
                                    },
                                    class: "px-3 py-2 text-sm rounded-lg border \
                                            border-red-500/30 bg-red-950/20 text-red-300 \
                                            hover:bg-red-950/40",
                                    "Purge \"{tag}\"…"
                                }
                            }
                            // Retention is a policy surface, not a filter, so it
                            // is behind a toggle: the list's job is finding a
                            // document, and a rules editor sitting open above it
                            // competes with that on every visit.
                            button {
                                onclick: move |_| {
                                    let next = !*retention_open.read();
                                    retention_open.set(next);
                                },
                                class: if *retention_open.read() {
                                    "px-3 py-2 text-sm rounded-lg border border-obsidian-accent/50 \
                                     bg-obsidian-accent/15 text-obsidian-text"
                                } else {
                                    "px-3 py-2 text-sm rounded-lg border border-obsidian-border/10 \
                                     bg-obsidian-sidebar/60 text-obsidian-text-muted \
                                     hover:text-obsidian-text"
                                },
                                "Retention"
                            }
                            // ⚠️ Its own control rather than an entry in the kind
                            // dropdown: mail has no `kind`, so listing it there
                            // would put a value in a menu built from a different
                            // column and quietly return nothing.
                            button {
                                onclick: move |_| {
                                    let next = !*mail_only.read();
                                    mail_only.set(next);
                                },
                                class: if *mail_only.read() {
                                    "px-3 py-2 text-sm rounded-lg border border-obsidian-accent/50 \
                                     bg-obsidian-accent/15 text-obsidian-text"
                                } else {
                                    "px-3 py-2 text-sm rounded-lg border border-obsidian-border/10 \
                                     bg-obsidian-sidebar/60 text-obsidian-text-muted \
                                     hover:text-obsidian-text"
                                },
                                "Mail only"
                            }
                            // The nav badge's set. ⛔ A filter, never a queue of
                            // its own: the module header's rule is that a value
                            // is corrected beside its document, and narrowing the
                            // list still opens the detail view to do it.
                            button {
                                onclick: move |_| {
                                    let next = !*unverified_only.read();
                                    unverified_only.set(next);
                                },
                                class: if *unverified_only.read() {
                                    "px-3 py-2 text-sm rounded-lg border border-amber-500/50 \
                                     bg-amber-500/15 text-obsidian-text"
                                } else {
                                    "px-3 py-2 text-sm rounded-lg border border-obsidian-border/10 \
                                     bg-obsidian-sidebar/60 text-obsidian-text-muted \
                                     hover:text-obsidian-text"
                                },
                                "Unchecked only"
                            }
                        }

                        if *retention_open.read() {
                            RetentionPanel {
                                on_review: move |(t, cutoff): (String, String)| {
                                    view.set(View::Purge(t, Some(cutoff)));
                                },
                            }
                        }

                        match documents.read().as_ref() {
                            None => rsx! {
                                p { class: "text-sm text-obsidian-text-muted", "Loading the archive…" }
                            },
                            Some(Err(e)) => rsx! {
                                div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded text-sm text-red-300",
                                    "Couldn't read the archive: {e}"
                                }
                            },
                            Some(Ok(docs)) if docs.is_empty() => rsx! {
                                // ⚠️ Every filter counts, not just the search box.
                                // A narrowing that is not listed here reports "the
                                // archive is empty" for a filter that simply
                                // matched nothing — the two readings this
                                // component exists to keep apart. `mail_only` was
                                // missing for the same reason `tag` would have been.
                                EmptyState {
                                    searching: !search.read().is_empty()
                                        || !kind.read().is_empty()
                                        || !tag.read().is_empty()
                                        || *mail_only.read()
                                        || *unverified_only.read(),
                                }
                            },
                            Some(Ok(docs)) => {
                                let first_len = docs.len() as u32;
                                let extra = extra_pages.read().clone();
                                let shown = docs.len() + extra.len();
                                // The first page filling exactly is the only
                                // evidence there is more; a short page is the end.
                                // ⚠️ `exhausted` then records what a later page
                                // actually returned, so this never has to guess
                                // twice.
                                let more_possible = first_len == DOC_PAGE && !*exhausted.read();
                                rsx! {
                                    ul { class: "space-y-1.5",
                                        for doc in docs.iter().chain(extra.iter()) {
                                            DocumentCard {
                                                key: "{doc.document_id}",
                                                doc: doc.clone(),
                                                on_open: move |id| view.set(View::Detail(id)),
                                            }
                                        }
                                    }

                                    if let Some(msg) = page_error.read().clone() {
                                        p { class: "text-[11px] text-red-300", "Couldn't load more: {msg}" }
                                    }

                                    // ⛔ The count is always stated once there is
                                    // more than a page. Newest-first ordering makes
                                    // a silent cut-off read as "nothing older
                                    // exists", which is the failure this replaces.
                                    if more_possible {
                                        div { class: "flex items-center gap-3 pt-1",
                                            button {
                                                disabled: *loading_more.read(),
                                                onclick: move |_| {
                                                    if *loading_more.peek() { return; }
                                                    loading_more.set(true);
                                                    page_error.set(None);
                                                    let q = search.peek().clone();
                                                    let k = kind.peek().clone();
                                                    let m = if *mail_only.peek() {
                                                        MAIL_MIME.to_string()
                                                    } else {
                                                        String::new()
                                                    };
                                                    let t = tag.peek().clone();
                                                    let u = *unverified_only.peek();
                                                    // `peek`, not `read`: this runs
                                                    // inside an event handler, and a
                                                    // tracked read here would
                                                    // subscribe the handler's scope
                                                    // to every filter it touches.
                                                    let next_offset = shown as u32;
                                                    spawn(async move {
                                                        let got = bridge::invoke_list_documents(
                                                            Some(q), Some(k), Some(m), Some(t),
                                                            Some(u), Some(DOC_PAGE), Some(next_offset),
                                                        ).await;
                                                        match got {
                                                            Ok(rows) => {
                                                                if (rows.len() as u32) < DOC_PAGE {
                                                                    exhausted.set(true);
                                                                }
                                                                extra_pages.write().extend(rows);
                                                            }
                                                            Err(e) => page_error.set(Some(e)),
                                                        }
                                                        loading_more.set(false);
                                                    });
                                                },
                                                class: "px-3 py-2 text-sm rounded-lg border \
                                                        border-obsidian-border/10 bg-obsidian-sidebar/60 \
                                                        text-obsidian-text hover:border-obsidian-accent/40 \
                                                        disabled:opacity-50",
                                                if *loading_more.read() { "Loading…" } else { "Load more" }
                                            }
                                            span { class: "text-[11px] text-obsidian-text-muted",
                                                "{shown} shown"
                                            }
                                        }
                                    } else if shown > docs.len() {
                                        p { class: "pt-1 text-[11px] text-obsidian-text-muted",
                                            "{shown} shown — that's everything."
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// ⚠️ Distinguishes "nothing filed" from "nothing matched". They look identical
/// and mean opposite things — one is a setup step, the other a bad search.
#[component]
fn EmptyState(searching: bool) -> Element {
    rsx! {
        div { class: "p-6 text-center space-y-1",
            p { class: "text-sm text-obsidian-text",
                if searching { "Nothing matches that." } else { "The archive is empty." }
            }
            p { class: "text-xs text-obsidian-text-muted",
                if searching {
                    "Search covers filenames, titles, and the text inside a document."
                } else {
                    "Documents appear here as they're captured, emailed in, or imported."
                }
            }
        }
    }
}

#[component]
fn DocumentCard(doc: DocumentItem, on_open: EventHandler<String>) -> Element {
    let id = doc.document_id.clone();
    let unverified = doc.unverified_count();

    rsx! {
        li {
            button {
                onclick: move |_| on_open.call(id.clone()),
                class: "w-full text-left p-3 rounded-lg bg-obsidian-sidebar/40 border \
                        border-obsidian-border/5 hover:border-obsidian-accent/30 transition-colors",
                div { class: "flex items-start justify-between gap-3",
                    div { class: "min-w-0 space-y-0.5",
                        div { class: "text-sm text-obsidian-text truncate", "{doc.display_name()}" }
                        div { class: "text-[11px] text-obsidian-text-muted flex flex-wrap gap-x-2",
                            if let Some(k) = &doc.kind {
                                span { class: "text-obsidian-accent", "{humanise(k)}" }
                            }
                            if let Some(d) = &doc.document_date {
                                span { "{d}" }
                            }
                            if let Some(s) = &doc.ingest_source {
                                span { "via {s}" }
                            }
                            // ⚠️ Surfaced because a document the archive cannot
                            // read is findable by name only — invisible
                            // otherwise, and the user can do something about it.
                            if !doc.is_searchable_by_content() {
                                span { class: "text-amber-400/80", "no text" }
                            }
                        }
                        // On the row rather than the detail view only: a tag is
                        // how the user finds this again, so scanning the list is
                        // when they need to see whether it carries one.
                        if let Some(tags) = doc.tags.as_ref().filter(|t| !t.is_empty()) {
                            div { class: "flex flex-wrap gap-1 pt-0.5",
                                for t in tags.iter() {
                                    span {
                                        key: "{t}",
                                        class: "px-1.5 py-0.5 rounded text-[10px] \
                                                bg-obsidian-accent/10 text-obsidian-accent/90 \
                                                border border-obsidian-accent/20",
                                        "{t}"
                                    }
                                }
                            }
                        }
                    }
                    if unverified > 0 {
                        span {
                            class: "shrink-0 px-1.5 py-0.5 rounded text-[10px] bg-amber-500/10 \
                                    text-amber-300 border border-amber-500/20",
                            title: "Values nothing has checked",
                            "{unverified} unchecked"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DocumentDetail(
    document_id: String,
    on_back: EventHandler<()>,
    on_corrected: EventHandler<()>,
    /// Open another document — an email's attachment, which is a document in
    /// its own right and so gets the same detail view rather than a preview
    /// nested inside its parent.
    on_open: EventHandler<String>,
) -> Element {
    let mut reload = use_signal(|| 0u32);
    let id_for_fetch = document_id.clone();
    let doc = use_resource(move || {
        let id = id_for_fetch.clone();
        let _ = reload.read();
        async move { bridge::invoke_get_document(&id).await }
    });

    rsx! {
        div { class: "p-4 space-y-4 max-w-5xl mx-auto",
            button {
                onclick: move |_| on_back.call(()),
                class: "text-xs text-obsidian-text-muted hover:text-obsidian-text",
                "← Archive"
            }

            match doc.read().as_ref() {
                None => rsx! {
                    p { class: "text-sm text-obsidian-text-muted", "Loading…" }
                },
                Some(Err(e)) => rsx! {
                    div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded text-sm text-red-300",
                        "Couldn't open this document: {e}"
                    }
                },
                Some(Ok(None)) => rsx! {
                    p { class: "text-sm text-obsidian-text-muted",
                        "This document is no longer in the archive."
                    }
                },
                Some(Ok(Some(d))) => {
                    let d = d.clone();
                    let meta = document_meta(&d);
                    rsx! {
                        h1 { class: "text-lg font-semibold text-obsidian-text", "{d.display_name()}" }

                        // The reverse of the email view's attachment list. Without
                        // it the parent link is navigable one way only, and a
                        // statement PDF reads as though it had been filed on its
                        // own — which is exactly what `parent_document_id` exists
                        // to record that it was not.
                        if let Some(parent) = d.parent_document_id.clone() {
                            ParentLink { parent_id: parent, on_open: on_open }
                        }

                        // ⛔ The document and its fields, side by side at md+ and
                        // stacked below. Editing a value requires seeing the
                        // document; this grid is that requirement, in layout.
                        div { class: "grid md:grid-cols-2 gap-4 items-start",
                            // min-w-0 on both children is load-bearing. A grid item
                            // defaults to min-width:auto, so it refuses to shrink
                            // below its content and the viewers below — each of
                            // which sets overflow-auto expecting to scroll itself —
                            // widen the track instead. The page then pans sideways.
                            div { class: "min-w-0",
                                // ⛔ An email renders from its stored text, never
                                // from its bytes. The bytes are raw MIME — quoted-
                                // printable and base64 parts — so the byte viewer
                                // would show scaffolding, and parsing MIME again in
                                // wasm would duplicate what ingest already did.
                                // ⛔ Checked before the viewer, never after. A
                                // purged document still carries its `sha256`, so
                                // the viewer would fetch, get a 404 and render
                                // "couldn't load attachment" — which is what a
                                // *broken* archive looks like. A 404 cannot tell
                                // deliberate removal from a missing file; the
                                // column can, and this is the only place that
                                // distinction reaches the reader.
                                if d.purged.unwrap_or(false) {
                                    div { class: "p-3 rounded border border-obsidian-border/10 text-xs text-obsidian-text-muted space-y-1",
                                        p { class: "text-obsidian-text", "This document was purged." }
                                        p { "Its file has been deleted and cannot be recovered." }
                                        // What it was still shows in the panel
                                        // beside this, which is the whole reason a
                                        // purge keeps the labels.
                                    }
                                } else if d.is_email() {
                                    EmailView { document_id: d.document_id.clone(), on_open: on_open }
                                } else {
                                    match meta {
                                        Some(meta) => rsx! { AttachmentViewer { meta: meta } },
                                        None => rsx! {
                                            div { class: "p-3 rounded border border-obsidian-border/10 text-xs text-obsidian-text-muted",
                                                "The file for this entry hasn't arrived on this device yet."
                                            }
                                        },
                                    }
                                }
                            }
                            div { class: "min-w-0 space-y-4",
                                // ⚠️ No tag editor on a purged document: tags exist
                                // to find a document again, and this one is gone.
                                if !d.purged.unwrap_or(false) {
                                    TagPanel {
                                        doc: d.clone(),
                                        on_saved: move |_| {
                                            reload += 1;
                                            on_corrected.call(());
                                        },
                                    }
                                }
                                FieldPanel {
                                    doc: d.clone(),
                                    on_saved: move |_| {
                                        reload += 1;
                                        on_corrected.call(());
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Split the archive's stored email text into headers and body.
///
/// ⚠️ Splits on the **first blank line**, which is what `archive::derive_text`
/// writes and what RFC 5322 defines. ⛔ Returning the whole text as a body when
/// no blank line is found is deliberate: showing everything unsplit is a worse
/// layout, while showing nothing would hide a message that is genuinely there.
fn split_email_text(text: &str) -> (Vec<(String, String)>, String) {
    let normalized = text.replace("\r\n", "\n");
    let Some((head, body)) = normalized.split_once("\n\n") else {
        return (Vec::new(), normalized);
    };
    let headers = head
        .lines()
        .filter_map(|line| {
            let (k, v) = line.split_once(':')?;
            let v = v.trim();
            // ⚠️ A header with no value is dropped rather than rendered as a
            // dangling label — `Subject:` with nothing after it is common in
            // automated mail.
            (!v.is_empty()).then(|| (k.trim().to_string(), v.to_string()))
        })
        .collect();
    (headers, body.to_string())
}

/// "Arrived inside …", on a document that came in as an attachment.
///
/// ⚠️ Fetches the parent only to name it. Rendering a bare "arrived inside an
/// email" would need no call and tell the reader nothing they could act on —
/// the useful fact is *which* message, and the way back to it.
#[component]
fn ParentLink(parent_id: String, on_open: EventHandler<String>) -> Element {
    let id_for_fetch = parent_id.clone();
    let parent = use_resource(move || {
        let id = id_for_fetch.clone();
        async move { bridge::invoke_get_document(&id).await }
    });

    rsx! {
        // ⛔ Renders nothing until the parent is known, and nothing at all if the
        // lookup fails. A half-resolved "arrived inside …" with no name is worse
        // than staying quiet, and this is decoration on someone else's page.
        match parent.read().as_ref() {
            Some(Ok(Some(p))) => {
                let name = p.display_name();
                let target = parent_id.clone();
                rsx! {
                    button {
                        onclick: move |_| on_open.call(target.clone()),
                        class: "text-xs text-obsidian-text-muted hover:text-obsidian-text underline \
                                decoration-dotted underline-offset-2",
                        "Arrived inside {name}"
                    }
                }
            }
            _ => rsx! {},
        }
    }
}

/// An archived email: its headers, its body, and the documents that came with it.
///
/// ⛔ **Renders the archive's extracted text, never the raw bytes.** The bytes
/// are raw MIME, so the byte viewer would show base64 payloads and boundary
/// markers; re-parsing MIME in wasm would duplicate work ingest already did and
/// give a second implementation to disagree with the first.
#[component]
fn EmailView(document_id: String, on_open: EventHandler<String>) -> Element {
    let id_for_text = document_id.clone();
    let text = use_resource(move || {
        let id = id_for_text.clone();
        async move { bridge::invoke_get_document_text(&id).await }
    });
    let id_for_children = document_id.clone();
    let children = use_resource(move || {
        let id = id_for_children.clone();
        async move { bridge::invoke_document_children(&id).await }
    });

    rsx! {
        div { class: "space-y-3",
            match text.read().as_ref() {
                None => rsx! {
                    p { class: "text-sm text-obsidian-text-muted", "Loading the message…" }
                },
                Some(Err(e)) => rsx! {
                    div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded text-sm text-red-300",
                        "Couldn't read this message: {e}"
                    }
                },
                // ⚠️ Distinguished from an empty body on purpose: "we have no
                // text for this" and "this message said nothing" are different
                // facts, and only the first is a reason to go looking.
                Some(Ok(None)) => rsx! {
                    div { class: "p-3 rounded border border-obsidian-border/10 text-xs text-obsidian-text-muted",
                        "No text was extracted from this message."
                    }
                },
                Some(Ok(Some(raw))) => {
                    let (headers, body) = split_email_text(raw);
                    rsx! {
                        div { class: "rounded border border-obsidian-border/10 overflow-hidden",
                            if !headers.is_empty() {
                                dl { class: "px-3 py-2 space-y-1 bg-obsidian-sidebar/40 border-b border-obsidian-border/10",
                                    for (k, v) in headers {
                                        div { class: "flex gap-2 text-xs",
                                            dt { class: "shrink-0 w-16 text-obsidian-text-muted", "{k}" }
                                            dd { class: "text-obsidian-text break-words min-w-0", "{v}" }
                                        }
                                    }
                                }
                            }
                            pre {
                                class: "px-3 py-3 text-xs text-obsidian-text whitespace-pre-wrap \
                                        break-words max-h-[28rem] overflow-y-auto",
                                "{body}"
                            }
                        }
                    }
                }
            }

            // The attachments, as the separate documents they already are.
            match children.read().as_ref() {
                Some(Ok(kids)) if !kids.is_empty() => rsx! {
                    div { class: "space-y-1",
                        h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                            "Arrived with this message"
                        }
                        for kid in kids.clone() {
                            button {
                                key: "{kid.document_id}",
                                onclick: move |_| on_open.call(kid.document_id.clone()),
                                class: "w-full text-left px-3 py-2 rounded border border-obsidian-border/10 \
                                        hover:border-obsidian-accent/40 text-xs text-obsidian-text",
                                "{kid.display_name()}"
                            }
                        }
                    }
                },
                _ => rsx! {},
            }
        }
    }
}

/// The purge queue for one tag group: every document listed, each sparable, one
/// confirm.
///
/// ⛔ **This shape is the ruling, not a layout choice** (user, 2026-09-27).
/// Strict per-item confirmation was refused as unusable at backfill scale, and
/// approving a *rule* was refused because it grants autonomy over an irreversible
/// action. What makes this satisfy the autonomy rule is that nothing goes without
/// the person having seen it listed — so if the list is ever truncated, the
/// confirm must cover only what was listed, and say so.
/// Retention rules, and what they have caught.
///
/// ⛔ Proposes only. "Review" opens the same purge preview and confirm every purge
/// goes through — retention never removes anything itself, which is the standing
/// ruling, not a limitation of this screen.
#[component]
fn RetentionPanel(on_review: EventHandler<(String, String)>) -> Element {
    let mut reload = use_signal(|| 0u32);
    let mut chosen = use_signal(String::new);
    let mut days = use_signal(String::new);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut saving = use_signal(|| false);

    let tags = use_resource(move || async move { bridge::invoke_document_tags().await });
    let rules = use_resource(move || {
        let _ = reload.read();
        async move { bridge::invoke_list_document_retention().await }
    });
    let groups = use_resource(move || {
        let _ = reload.read();
        async move { bridge::invoke_list_retention_candidates().await }
    });

    // `None` clears the rule. Both paths go through here so the two buttons cannot
    // drift apart on what "saved" means.
    let mut apply = move |keep_days: Option<u32>| {
        let tag = chosen.read().clone();
        if tag.is_empty() {
            error.set(Some("pick a tag first".into()));
            return;
        }
        if *saving.read() {
            return;
        }
        saving.set(true);
        spawn(async move {
            match bridge::invoke_set_document_retention(&tag, keep_days).await {
                Ok(()) => {
                    error.set(None);
                    reload += 1;
                }
                Err(e) => error.set(Some(e)),
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "p-3 rounded-lg border border-obsidian-border/10 bg-obsidian-sidebar/30 space-y-3",
            div { class: "space-y-1",
                h2 { class: "text-sm font-semibold text-obsidian-text", "Retention" }
                // ⛔ The sentence this surface exists to carry. An empty rules list
                // is not "nothing configured yet", it is "everything is kept", and
                // a tag with no rule keeps its documents even when another tag on
                // them says to let go.
                p { class: "text-[11px] text-obsidian-text-muted",
                    "A tag with no rule is kept forever, and a document is only ever proposed when \
                     every tag on it has one — the longest of them decides."
                }
            }

            div { class: "flex flex-wrap items-center gap-2",
                select {
                    value: "{chosen}",
                    onchange: move |e| chosen.set(e.value()),
                    class: "px-2 py-1.5 text-sm rounded bg-obsidian-bg border \
                            border-obsidian-border/10 text-obsidian-text focus:outline-none",
                    option { value: "", "Pick a tag…" }
                    if let Some(Ok(list)) = tags.read().as_ref() {
                        for t in list.iter() {
                            option { key: "{t}", value: "{t}", "{t}" }
                        }
                    }
                }
                span { class: "text-[11px] text-obsidian-text-muted", "keep for" }
                input {
                    r#type: "number",
                    min: "1",
                    placeholder: "days",
                    value: "{days}",
                    oninput: move |e| days.set(e.value()),
                    class: "w-24 px-2 py-1.5 text-sm rounded bg-obsidian-bg border \
                            border-obsidian-border/10 text-obsidian-text focus:outline-none",
                }
                button {
                    onclick: move |_| {
                        match days.read().trim().parse::<u32>() {
                            Ok(n) if n > 0 => apply(Some(n)),
                            // ⛔ Zero is refused rather than read as "purge now" —
                            // the most destructive reading of a typo.
                            _ => error.set(Some("give a number of days, at least 1".into())),
                        }
                    },
                    disabled: *saving.read(),
                    class: "px-2 py-1.5 text-[11px] rounded bg-obsidian-accent text-black \
                            font-medium disabled:opacity-50",
                    "Save"
                }
                button {
                    onclick: move |_| apply(None),
                    disabled: *saving.read(),
                    class: "px-2 py-1.5 text-[11px] rounded border border-obsidian-border/20 \
                            text-obsidian-text-muted hover:text-obsidian-text disabled:opacity-50",
                    "Keep forever"
                }
            }
            if let Some(msg) = error.read().clone() {
                p { class: "text-[11px] text-red-300", "{msg}" }
            }

            match rules.read().as_ref() {
                Some(Ok(list)) if !list.is_empty() => rsx! {
                    div { class: "flex flex-wrap gap-1.5",
                        for rule in list.iter() {
                            span {
                                key: "{rule.tag}",
                                class: "px-2 py-0.5 text-[10px] rounded-full bg-obsidian-bg \
                                        border border-obsidian-border/10 text-obsidian-text-muted",
                                "{rule.tag} · {rule.keep_days}d"
                            }
                        }
                    }
                },
                Some(Ok(_)) => rsx! {
                    p { class: "text-[11px] text-obsidian-text-muted", "No rules yet, so nothing is proposed." }
                },
                Some(Err(e)) => rsx! {
                    p { class: "text-[11px] text-red-300", "Couldn't read the rules: {e}" }
                },
                None => rsx! {
                    p { class: "text-[11px] text-obsidian-text-muted", "Reading the rules…" }
                },
            }

            match groups.read().as_ref() {
                Some(Ok(list)) if !list.is_empty() => rsx! {
                    div { class: "space-y-1.5",
                        h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                            "Past their retention"
                        }
                        for group in list.iter() {
                            div {
                                key: "{group.tag}",
                                class: "flex items-center justify-between gap-2 p-2 rounded \
                                        border border-obsidian-border/10 text-xs",
                                div {
                                    p { class: "text-obsidian-text", "{group.tag} · {group.count} document(s)" }
                                    p { class: "text-[10px] text-obsidian-text-muted",
                                        "kept {group.keep_days} days"
                                        if let Some(oldest) = group.oldest_archived_at.as_deref() {
                                            ", oldest {oldest}"
                                        }
                                    }
                                }
                                // ⛔ "Review", not "Purge". What this opens is the
                                // preview: every document listed, each sparable,
                                // one confirm.
                                button {
                                    onclick: {
                                        let tag = group.tag.clone();
                                        let cutoff = group.cutoff.clone();
                                        move |_| on_review.call((tag.clone(), cutoff.clone()))
                                    },
                                    class: "px-2 py-1 text-[11px] rounded border border-obsidian-accent/40 \
                                            text-obsidian-accent hover:bg-obsidian-accent/10 shrink-0",
                                    "Review {group.count}…"
                                }
                            }
                        }
                    }
                },
                Some(Ok(_)) => rsx! {
                    p { class: "text-[11px] text-obsidian-text-muted", "Nothing is past its retention." }
                },
                Some(Err(e)) => rsx! {
                    p { class: "text-[11px] text-red-300", "Couldn't work out what is due: {e}" }
                },
                None => rsx! {
                    p { class: "text-[11px] text-obsidian-text-muted", "Checking what is due…" }
                },
            }
        }
    }
}

#[component]
fn PurgeGroup(
    tag: String,
    // Set for a retention group: only documents archived before this are in it.
    archived_before: Option<String>,
    on_back: EventHandler<()>,
    on_purged: EventHandler<()>,
) -> Element {
    // Ids the user has unchecked. Absent means going — the default is that the
    // group the user chose to purge is purged.
    let mut spared: Signal<std::collections::HashSet<String>> =
        use_signal(std::collections::HashSet::new);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut working = use_signal(|| false);
    let mut report: Signal<Option<crate::types::PurgeReport>> = use_signal(|| None);

    let for_load = tag.clone();
    let cutoff_for_load = archived_before.clone();
    let preview = use_resource(move || {
        let t = for_load.clone();
        let before = cutoff_for_load.clone();
        async move { bridge::invoke_preview_document_purge(&t, before.as_deref()).await }
    });

    rsx! {
        div { class: "p-4 space-y-4 max-w-3xl mx-auto",
            button {
                onclick: move |_| on_back.call(()),
                class: "text-xs text-obsidian-text-muted hover:text-obsidian-text",
                "← Archive"
            }

            match preview.read().as_ref() {
                None => rsx! {
                    p { class: "text-sm text-obsidian-text-muted", "Working out what this would remove…" }
                },
                Some(Err(e)) => rsx! {
                    div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded text-sm text-red-300",
                        "Couldn't read the group: {e}"
                    }
                },
                Some(Ok(p)) if p.items.is_empty() => rsx! {
                    p { class: "text-sm text-obsidian-text", "Nothing is tagged \"{tag}\" any more." }
                },
                Some(Ok(p)) => {
                    let going: Vec<String> = p
                        .items
                        .iter()
                        .filter(|i| !spared.read().contains(&i.document_id))
                        .map(|i| i.document_id.clone())
                        .collect();
                    // ⚠️ Recomputed from what is actually checked, not taken from
                    // the preview totals — otherwise sparing a row would leave the
                    // button promising bytes that are no longer going.
                    let bytes_going: u64 = p
                        .items
                        .iter()
                        .filter(|i| !i.bytes_shared && !spared.read().contains(&i.document_id))
                        .map(|i| i.size.unwrap_or(0).max(0) as u64)
                        .sum();
                    let token = p.token.clone();
                    let group = p.group.clone();
                    let count = going.len();

                    rsx! {
                        h1 { class: "text-lg font-semibold text-obsidian-text",
                            "Purge \"{p.group}\""
                        }
                        div { class: "text-xs text-obsidian-text-muted space-y-1",
                            p {
                                "{p.total} {crate::types::plural(p.total, \"document\")} · frees {crate::types::human_bytes(p.bytes_reclaimable)}"
                            }
                            // ⚠️ Stated separately rather than netted off: "frees
                            // 31 MB" and "frees 0.3 MB of the 31 MB you selected"
                            // are different answers, and shared bytes make the
                            // second one the true one.
                            if p.bytes_shared > 0 {
                                p { class: "text-amber-400/80",
                                    "{crate::types::human_bytes(p.bytes_shared)} stays — those files are also \
                                     used by something else."
                                }
                            }
                            if p.is_truncated() {
                                p { class: "text-amber-400/80",
                                    "⚠ Showing the first {p.listed}. Only these are purged — \
                                     run it again for the rest."
                                }
                            }
                            p { class: "text-red-300/80", "This cannot be undone." }
                        }

                        ul { class: "space-y-1",
                            for item in p.items.iter() {
                                {
                                    let id = item.document_id.clone();
                                    let is_spared = spared.read().contains(&id);
                                    rsx! {
                                        li {
                                            key: "{item.document_id}",
                                            class: "flex items-start gap-2 p-2 rounded border border-obsidian-border/5",
                                            input {
                                                r#type: "checkbox",
                                                checked: !is_spared,
                                                onchange: move |_| {
                                                    let mut set = spared.write();
                                                    if !set.remove(&id) {
                                                        set.insert(id.clone());
                                                    }
                                                },
                                                class: "mt-0.5",
                                            }
                                            div { class: "min-w-0 flex-1",
                                                div {
                                                    class: if is_spared {
                                                        "text-sm text-obsidian-text-muted line-through truncate"
                                                    } else {
                                                        "text-sm text-obsidian-text truncate"
                                                    },
                                                    "{item.label}"
                                                }
                                                div { class: "text-[11px] text-obsidian-text-muted flex flex-wrap gap-x-2",
                                                    // ⚠️ The date, not the stored
                                                    // timestamp. This list is
                                                    // scanned to decide what goes,
                                                    // and `…T09:00:00Z` is noise in
                                                    // every row of it.
                                                    if let Some(d) = &item.archived_at {
                                                        span { "{crate::types::day_of(d)}" }
                                                    }
                                                    if let Some(src) = &item.ingest_source {
                                                        span { "via {src}" }
                                                    }
                                                    if let Some(sz) = item.size {
                                                        span { "{crate::types::human_bytes(sz.max(0) as u64)}" }
                                                    }
                                                    // Per item, because "purging
                                                    // this frees nothing" is a
                                                    // reason to spare it.
                                                    if item.bytes_shared {
                                                        span { class: "text-amber-400/80", "file kept — shared" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(msg) = error.read().clone() {
                            div { class: "p-3 bg-red-950/30 border border-red-500/30 rounded text-sm text-red-300",
                                "{msg}"
                            }
                        }
                        if let Some(r) = report.read().clone() {
                            div { class: "p-3 rounded border border-obsidian-border/10 text-xs text-obsidian-text-muted",
                                "Purged {r.purged} of {r.selected} · freed {crate::types::human_bytes(r.bytes_deleted)}"
                                if r.blobs_retained_shared > 0 {
                                    " · {r.blobs_retained_shared} files kept, still in use"
                                }
                                if r.failed > 0 {
                                    span { class: "text-red-300", " · {r.failed} failed" }
                                }
                            }
                        }

                        button {
                            disabled: count == 0 || *working.read(),
                            onclick: move |_| {
                                let ids = going.clone();
                                let token = token.clone();
                                let group = group.clone();
                                working.set(true);
                                spawn(async move {
                                    match bridge::invoke_confirm_document_purge(
                                        &token, ids, Some(group),
                                    ).await {
                                        Ok(r) => {
                                            report.set(Some(r));
                                            error.set(None);
                                            on_purged.call(());
                                        }
                                        // ⚠️ Left on screen rather than bounced
                                        // back: a purge that half-failed is
                                        // something the user has to see, and the
                                        // report above says what did go.
                                        Err(e) => error.set(Some(e)),
                                    }
                                    working.set(false);
                                });
                            },
                            class: "px-3 py-2 text-sm rounded-lg border border-red-500/40 \
                                    bg-red-950/30 text-red-200 hover:bg-red-950/50 \
                                    disabled:opacity-40",
                            // ⚠️ Read it back before changing it. This is the last
                            // thing a person sees before an irreversible action,
                            // so it states the count and what it actually frees —
                            // and "frees nothing" is a real and common answer when
                            // every file in the group is shared.
                            if *working.read() {
                                "Purging…"
                            } else if count == 0 {
                                "Nothing selected"
                            } else if bytes_going == 0 {
                                "Purge {count} {crate::types::plural(count, \"document\")} · frees nothing"
                            } else {
                                "Purge {count} {crate::types::plural(count, \"document\")} · frees {crate::types::human_bytes(bytes_going)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Write a document's tag set.
///
/// A free function rather than a closure in [`TagPanel`] because both the add and
/// the remove handler need it, and a closure capturing the panel's signals is not
/// `Copy` — it can be moved into one handler or the other, never both.
///
/// ⚠️ Always the whole set. The fold overwrites the `tags` key and can never
/// remove one, so a delta would leave a tag the person took off still on the row.
#[allow(clippy::too_many_arguments)]
fn save_tag_set(
    document_id: String,
    next: Vec<String>,
    mut saving: Signal<bool>,
    mut error: Signal<Option<String>>,
    sync_epoch: Signal<u64>,
    on_saved: EventHandler<()>,
) {
    if *saving.read() {
        return;
    }
    saving.set(true);
    spawn(async move {
        match bridge::invoke_set_document_tags(&document_id, next).await {
            Ok(()) => {
                error.set(None);
                crate::sync_refresh::bump_sync_epoch(sync_epoch);
                on_saved.call(());
            }
            Err(e) => error.set(Some(e)),
        }
        saving.set(false);
    });
}

/// The tags a person has put on this document.
///
/// Its own panel rather than a row in [`FieldPanel`], because a tag is not a
/// reading of the document: the fields panel shows what something *claimed* and
/// whether anything checked it, and a tag has no such claim to display. It also
/// writes the whole set at once, which no field row does.
#[component]
fn TagPanel(doc: DocumentItem, on_saved: EventHandler<()>) -> Element {
    // Not `mut` here: `save_tag_set` takes them by value and owns the mutation.
    let error: Signal<Option<String>> = use_signal(|| None);
    let saving = use_signal(|| false);
    let current = doc.tags.clone().unwrap_or_default();
    let id = doc.document_id.clone();
    // At the top, never inside the save closure: it is a hook, and the badge and
    // the assistant's reminder row read a count this component is not.
    let sync_epoch = crate::sync_refresh::use_sync_epoch();

    let for_add = current.clone();
    let for_remove = current.clone();
    let id_for_add = id.clone();
    let id_for_remove = id;

    rsx! {
        div { class: "space-y-2",
            h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                "Tags"
            }
            div { class: "p-2 rounded border border-obsidian-border/10 text-xs",
                // ⛔ `sanitize: false`. The shared editor's sanitizer strips `:`,
                // which is what a `key:value` tag is built on; the backend
                // normalizes instead.
                TagChipEditor {
                    tags: current.clone(),
                    sanitize: false,
                    on_add: move |t: String| {
                        let mut next = for_add.clone();
                        next.push(t);
                        save_tag_set(
                            id_for_add.clone(), next, saving, error, sync_epoch, on_saved,
                        );
                    },
                    on_remove: move |idx: usize| {
                        let mut next = for_remove.clone();
                        if idx < next.len() {
                            next.remove(idx);
                            save_tag_set(
                                id_for_remove.clone(), next, saving, error, sync_epoch, on_saved,
                            );
                        }
                    },
                }
            }
            if let Some(msg) = error.read().clone() {
                // ⚠️ The backend rejects a whole set rather than dropping one bad
                // tag, so this has to be visible: silently keeping the old set
                // would look like a tag that saved and then vanished.
                p { class: "text-[11px] text-red-300", "Couldn't save tags: {msg}" }
            }
        }
    }
}

/// Record that a person read the document and these values are right.
///
/// Goes through the correction path with the value unchanged, which is what lands
/// `human` / `verified: true`. A later reader separates a confirm from a
/// correction by comparing with the model's own event, so neither needs a flag.
fn confirm_fields(
    document_id: String,
    fields: Vec<(String, String)>,
    mut saving: Signal<bool>,
    mut error: Signal<Option<String>>,
    sync_epoch: Signal<u64>,
    on_saved: EventHandler<()>,
) {
    if *saving.read() {
        return;
    }
    saving.set(true);
    spawn(async move {
        // One event per key, never one batched event: fields fold by key, and
        // each value has to stay separately attributable to the person who
        // checked it. See `human_correction` in `core/src/document_fields.rs`.
        let mut confirmed = 0usize;
        let mut failure = None;
        for (key, value) in fields {
            match bridge::invoke_correct_document_field(&document_id, &key, &value).await {
                Ok(()) => confirmed += 1,
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        // Refreshed whenever anything landed, even when a later key failed: those
        // events are real, and the unverified count the badge reads has moved.
        if confirmed > 0 {
            crate::sync_refresh::bump_sync_epoch(sync_epoch);
            on_saved.call(());
        }
        error.set(failure);
        saving.set(false);
    });
}

#[component]
fn FieldPanel(doc: DocumentItem, on_saved: EventHandler<()>) -> Element {
    // ⛔ `tags` is a folded field like any other, so it arrives here too — and it
    // must not be offered as a free-text row. Its value is the whole set joined by
    // commas, and editing it as text would write tags nothing normalized, which
    // the filter then cannot match. `TagPanel` owns that key.
    let fields: Vec<DocumentField> = doc
        .fields
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|f| f.key != TAGS_FIELD_KEY)
        .collect();

    // ⛔ A purged document cannot be checked against anything — its bytes are
    // gone — so confirming or correcting a value here would write `verified: true`
    // with no oracle behind it. Same argument that hides the tag editor above.
    let purged = doc.purged.unwrap_or(false);
    let unchecked: Vec<(String, String)> = fields
        .iter()
        .filter(|f| !f.verified)
        .map(|f| (f.key.clone(), f.value.clone()))
        .collect();

    let error: Signal<Option<String>> = use_signal(|| None);
    let saving = use_signal(|| false);
    let sync_epoch = crate::sync_refresh::use_sync_epoch();
    let id_for_all = doc.document_id.clone();
    // Cloned for the closure so the count stays readable in the label beside it.
    let unchecked_for_all = unchecked.clone();

    rsx! {
        div { class: "space-y-2",
            div { class: "flex items-baseline justify-between gap-2",
                h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                    "What we know"
                }
                // Offered from two unchecked values up: below that the row's own
                // control is the shorter path, and this would read as a second way
                // to do the same thing.
                if !purged && unchecked.len() > 1 {
                    button {
                        onclick: move |_| {
                            confirm_fields(
                                id_for_all.clone(),
                                unchecked_for_all.clone(),
                                saving,
                                error,
                                sync_epoch,
                                on_saved,
                            );
                        },
                        disabled: *saving.read(),
                        class: "px-2 py-0.5 text-[10px] rounded border border-obsidian-accent/40 \
                                text-obsidian-accent hover:bg-obsidian-accent/10 disabled:opacity-50",
                        if *saving.read() { "Confirming…" } else { "All {unchecked.len()} look right" }
                    }
                }
            }
            if let Some(msg) = error.read().clone() {
                p { class: "text-[11px] text-red-300", "Couldn't confirm: {msg}" }
            }
            if fields.is_empty() {
                div { class: "p-3 rounded border border-obsidian-border/10 text-xs text-obsidian-text-muted space-y-1",
                    p { "Nothing has read this document yet." }
                    // ⚠️ Says why rather than looking broken. Reading a scan
                    // needs a model, and that runs as a scheduled batch.
                    p { class: "text-obsidian-text-muted/70",
                        "Statements are read when they're filed; everything else is read on the next pass."
                    }
                }
            }
            for field in fields.iter() {
                FieldRow {
                    key: "{field.key}",
                    document_id: doc.document_id.clone(),
                    field: field.clone(),
                    read_only: purged,
                    on_saved: move |_| on_saved.call(()),
                }
            }
        }
    }
}

#[component]
fn FieldRow(
    document_id: String,
    field: DocumentField,
    /// Set for a purged document: the value stays readable, but there is nothing
    /// left to check it against. See [`FieldPanel`].
    read_only: bool,
    on_saved: EventHandler<()>,
) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| field.value.clone());
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut saving = use_signal(|| false);
    // A correction lands `verified: true`, which drops this document out of the
    // unverified count the nav badge and the assistant's reminder row both read.
    // Neither is this component, so neither learns without the nudge.
    let sync_epoch = crate::sync_refresh::use_sync_epoch();

    let field_for_save = field.clone();
    let id_for_save = document_id.clone();
    let field_for_confirm = field.clone();
    let id_for_confirm = document_id.clone();
    let save = move |_| {
        if *saving.read() {
            return;
        }
        let id = id_for_save.clone();
        let key = field_for_save.key.clone();
        let value = draft.read().clone();
        saving.set(true);
        spawn(async move {
            match bridge::invoke_correct_document_field(&id, &key, &value).await {
                Ok(()) => {
                    editing.set(false);
                    error.set(None);
                    crate::sync_refresh::bump_sync_epoch(sync_epoch);
                    on_saved.call(());
                }
                Err(e) => error.set(Some(e)),
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "p-2.5 rounded-lg bg-obsidian-sidebar/40 border border-obsidian-border/5 space-y-1",
            div { class: "flex items-center justify-between gap-2",
                span { class: "text-[11px] text-obsidian-text-muted", "{humanise(&field.key)}" }
                div { class: "flex items-center gap-1.5 shrink-0",
                    // ⛔ Provenance is always shown, never only on hover. A value
                    // a model guessed and one the bank's own figures confirm look
                    // identical otherwise.
                    span { class: "text-[10px] text-obsidian-text-muted/70", "{field.origin()}" }
                    if field.verified {
                        span { class: "text-[10px] text-emerald-400/80", title: "Checked against the document's own figures", "checked" }
                    } else {
                        span { class: "text-[10px] text-amber-400/80", title: "Nothing has checked this value", "unchecked" }
                    }
                }
            }

            if *editing.read() {
                div { class: "space-y-1.5",
                    input {
                        value: "{draft}",
                        oninput: move |e| draft.set(e.value()),
                        class: "w-full px-2 py-1 text-sm rounded bg-obsidian-bg border \
                                border-obsidian-accent/40 text-obsidian-text focus:outline-none",
                    }
                    div { class: "flex gap-1.5",
                        button {
                            onclick: save,
                            disabled: *saving.read(),
                            class: "px-2 py-1 text-[11px] rounded bg-obsidian-accent text-black \
                                    font-medium disabled:opacity-50",
                            if *saving.read() { "Saving…" } else { "Save" }
                        }
                        button {
                            onclick: move |_| {
                                draft.set(field.value.clone());
                                editing.set(false);
                                error.set(None);
                            },
                            class: "px-2 py-1 text-[11px] rounded text-obsidian-text-muted hover:text-obsidian-text",
                            "Cancel"
                        }
                    }
                }
            } else if read_only {
                p { class: "text-sm text-obsidian-text", "{field.value}" }
            } else {
                div { class: "flex items-start justify-between gap-2",
                    button {
                        onclick: move |_| editing.set(true),
                        class: "flex-1 text-left text-sm text-obsidian-text hover:text-obsidian-accent",
                        "{field.value}"
                    }
                    // Only on an unchecked value: confirming one already checked
                    // would write a second event saying what the first said.
                    if !field.verified {
                        button {
                            onclick: move |_| {
                                confirm_fields(
                                    id_for_confirm.clone(),
                                    vec![(
                                        field_for_confirm.key.clone(),
                                        field_for_confirm.value.clone(),
                                    )],
                                    saving,
                                    error,
                                    sync_epoch,
                                    on_saved,
                                );
                            },
                            disabled: *saving.read(),
                            class: "px-2 py-0.5 text-[10px] rounded border border-obsidian-accent/40 \
                                    text-obsidian-accent hover:bg-obsidian-accent/10 shrink-0 \
                                    disabled:opacity-50",
                            if *saving.read() { "…" } else { "Looks right" }
                        }
                    }
                }
            }
            if let Some(msg) = error.read().clone() {
                p { class: "text-[11px] text-red-300", "{msg}" }
            }
        }
    }
}

/// `notice_of_assessment` → `Notice of assessment`.
///
/// ⚠️ Display only. The stored key is what folds and what filters match, so ⛔
/// nothing may round-trip through this.
fn humanise(key: &str) -> String {
    let spaced = key.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> DocumentItem {
        DocumentItem {
            document_id: "d1".into(),
            sha256: Some("a".repeat(64)),
            filename: Some("stmt.csv".into()),
            mime_type: Some("text/csv".into()),
            size: Some(100),
            archived_at: None,
            ingest_source: None,
            text_source: Some("extracted".into()),
            kind: None,
            title: None,
            document_date: None,
            tags: None,
            purged: None,
            fields: None,
            parent_document_id: None,
        }
    }

    /// ⛔ Mail is recognised by MIME, never by `kind`. Field extraction has not
    /// run for mail and may never, so a `kind`-based check would report every
    /// archived email as an ordinary file and route it to the byte viewer —
    /// which would render raw MIME at the user.
    #[test]
    fn an_email_is_recognised_by_its_type_not_by_a_kind() {
        let mut d = doc();
        d.mime_type = Some("message/rfc822".into());
        assert!(d.is_email());
        assert_eq!(d.kind, None, "the fixture must not lean on a kind");

        d.mime_type = Some("application/pdf".into());
        assert!(!d.is_email());
    }

    #[test]
    fn an_email_splits_into_headers_and_body_at_the_blank_line() {
        let (headers, body) =
            split_email_text("From: a@b.test\r\nSubject: Hello\r\n\r\nThe body.\r\nSecond line.");
        assert_eq!(
            headers,
            vec![
                ("From".to_string(), "a@b.test".to_string()),
                ("Subject".to_string(), "Hello".to_string()),
            ]
        );
        assert_eq!(body, "The body.\nSecond line.", "CRLF must be normalised");
    }

    /// ⚠️ A colon inside the subject must not split the header a second time.
    #[test]
    fn a_header_value_containing_a_colon_survives_intact() {
        let (headers, _) = split_email_text("Subject: Re: your order: shipped\n\nbody");
        assert_eq!(headers[0].1, "Re: your order: shipped");
    }

    /// ⛔ Text with no blank line still renders. Returning nothing would hide a
    /// message that is genuinely there because its shape was unexpected.
    #[test]
    fn text_with_no_header_block_is_all_body() {
        let (headers, body) = split_email_text("just a bare line");
        assert!(headers.is_empty());
        assert_eq!(body, "just a bare line");
    }

    /// An empty-valued header is dropped rather than rendered as a bare label —
    /// `Subject:` with nothing after it is common in automated mail.
    #[test]
    fn an_empty_header_value_is_dropped_not_rendered_blank() {
        let (headers, _) = split_email_text("From: a@b.test\nSubject:\n\nbody");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, "From");
    }

    #[test]
    fn a_key_reads_as_words_without_changing_the_key() {
        assert_eq!(humanise("notice_of_assessment"), "Notice of assessment");
        assert_eq!(humanise("period_start"), "Period start");
        assert_eq!(humanise(""), "");
    }

    #[test]
    fn a_document_with_no_file_yet_has_nothing_to_render() {
        // ⚠️ Not a defensive branch: fields can fold before the archive event
        // that carries the hash, so this row shape is reachable in normal sync.
        let mut d = doc();
        d.sha256 = None;
        assert!(document_meta(&d).is_none());
    }

    #[test]
    fn a_document_missing_its_type_still_gets_a_viewer() {
        let mut d = doc();
        d.mime_type = None;
        d.filename = None;
        let meta = document_meta(&d).expect("a hash is all the viewer needs");
        assert_eq!(meta.mime_type, "application/octet-stream");
        assert!(
            !meta.filename.is_empty(),
            "⛔ an unnamed document must still be openable"
        );
    }

    #[test]
    fn a_nameless_document_is_still_listed() {
        let mut d = doc();
        d.filename = None;
        assert!(
            d.display_name().contains("d1"),
            "falls back to the id rather than rendering a blank row"
        );
    }

    #[test]
    fn a_scan_with_no_text_is_flagged_as_unsearchable() {
        let mut d = doc();
        d.text_source = Some("none".into());
        assert!(!d.is_searchable_by_content());
        d.text_source = None;
        assert!(!d.is_searchable_by_content());
        d.text_source = Some("transcribed".into());
        assert!(d.is_searchable_by_content());
    }
}
