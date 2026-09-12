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
use crate::components::attachment_viewer::{AttachmentMeta, AttachmentViewer};
use crate::types::{DocumentField, DocumentItem};
use crate::use_page_back;

/// The MIME type an archived email carries. ⛔ Must match `archive::mime_for`'s
/// `"eml"` arm — the archive writes it and this filters on it, and nothing else
/// connects the two.
const MAIL_MIME: &str = "message/rfc822";

#[derive(Clone, PartialEq)]
enum View {
    List,
    Detail(String),
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
    let mut reload = use_signal(|| 0u32);

    // Hardware back pops the detail view before it leaves the tab.
    use_page_back(
        move || match *view.read() {
            View::List => 0,
            View::Detail(_) => 1,
        },
        move || view.set(View::List),
    );

    let documents = use_resource(move || {
        let q = search.read().clone();
        let k = kind.read().clone();
        let m = if *mail_only.read() {
            MAIL_MIME.to_string()
        } else {
            String::new()
        };
        let _ = reload.read();
        async move { bridge::invoke_list_documents(Some(q), Some(k), Some(m), None).await }
    });

    let kinds = use_resource(move || async move { bridge::invoke_document_kinds().await });

    rsx! {
        div { class: "h-full overflow-y-auto",
            match view.read().clone() {
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
                                EmptyState { searching: !search.read().is_empty() || !kind.read().is_empty() }
                            },
                            Some(Ok(docs)) => rsx! {
                                ul { class: "space-y-1.5",
                                    for doc in docs.iter() {
                                        DocumentCard {
                                            key: "{doc.document_id}",
                                            doc: doc.clone(),
                                            on_open: move |id| view.set(View::Detail(id)),
                                        }
                                    }
                                }
                            },
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
                    let meta = viewer_meta(&d);
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
                            div {
                                // ⛔ An email renders from its stored text, never
                                // from its bytes. The bytes are raw MIME — quoted-
                                // printable and base64 parts — so the byte viewer
                                // would show scaffolding, and parsing MIME again in
                                // wasm would duplicate what ingest already did.
                                if d.is_email() {
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

/// ⚠️ `None` when the row has no `sha256` — a fields event can fold before the
/// archive event that carries the hash, so a document can legitimately exist
/// with nothing to render yet.
fn viewer_meta(doc: &DocumentItem) -> Option<AttachmentMeta> {
    Some(AttachmentMeta {
        sha256: doc.sha256.clone()?,
        filename: doc.filename.clone().unwrap_or_else(|| doc.display_name()),
        mime_type: doc
            .mime_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_string()),
        size: doc.size.unwrap_or(0).max(0) as u64,
    })
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

#[component]
fn FieldPanel(doc: DocumentItem, on_saved: EventHandler<()>) -> Element {
    let fields = doc.fields.clone().unwrap_or_default();

    rsx! {
        div { class: "space-y-2",
            h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest",
                "What we know"
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
                    on_saved: move |_| on_saved.call(()),
                }
            }
        }
    }
}

#[component]
fn FieldRow(document_id: String, field: DocumentField, on_saved: EventHandler<()>) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| field.value.clone());
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut saving = use_signal(|| false);

    let field_for_save = field.clone();
    let id_for_save = document_id.clone();
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
                    if let Some(msg) = error.read().clone() {
                        p { class: "text-[11px] text-red-300", "{msg}" }
                    }
                }
            } else {
                button {
                    onclick: move |_| editing.set(true),
                    class: "w-full text-left text-sm text-obsidian-text hover:text-obsidian-accent",
                    "{field.value}"
                }
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
        assert!(viewer_meta(&d).is_none());
    }

    #[test]
    fn a_document_missing_its_type_still_gets_a_viewer() {
        let mut d = doc();
        d.mime_type = None;
        d.filename = None;
        let meta = viewer_meta(&d).expect("a hash is all the viewer needs");
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
