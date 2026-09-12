//! Rendering a stored document inline, from bytes already on the device.
//!
//! ⛔ **Every branch renders client-side.** `core::attachments` keeps a bounded
//! cache so a captured document stays viewable offline; a viewer that reached
//! for the server would defeat the thing the cache exists for.
//!
//! ⛔ **No download link, in any branch.** A download is what the old fallback
//! did for every type it did not understand, and on Android WebView it silently
//! does nothing — so the user sees a button that looks like an answer and is
//! not one. An honest "no preview, here is what this is" is worth more.

use dioxus::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

use crate::bridge;

/// `<script src>` for the bundled pdf.js build. ⚠️ The worker sits beside it and
/// `pdfview.js` names it by path, so **both** files must reach **both** copy
/// targets (`web/public` and the Android assets) — `package.json`'s
/// `copy:editor:{dev,release}` do that.
const PDF_BUNDLE_SRC: &str = "/assets/js/pdfview.bundle.js";

/// DOM id the PDF canvases are rendered into.
const PDF_CONTAINER_ID: &str = "pdf-container";

/// Owns a `blob:` URL and revokes it on Drop so the WebView does not accumulate
/// orphaned object URLs across navigations. Held inside a signal, the URL's
/// lifetime is tied to the component showing it.
pub struct ObjectUrlGuard(String);

impl ObjectUrlGuard {
    pub fn from_bytes(bytes: &[u8], mime: &str) -> Result<Self, String> {
        use wasm_bindgen::JsCast;
        let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        arr.copy_from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&arr.buffer());
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(mime);
        let blob =
            web_sys::Blob::new_with_u8_array_sequence_and_options(parts.unchecked_ref(), &opts)
                .map_err(|e| format!("blob construct: {e:?}"))?;
        let url = web_sys::Url::create_object_url_with_blob(&blob)
            .map_err(|e| format!("object url: {e:?}"))?;
        Ok(Self(url))
    }

    pub fn url(&self) -> &str {
        &self.0
    }
}

impl Drop for ObjectUrlGuard {
    fn drop(&mut self) {
        let _ = web_sys::Url::revoke_object_url(&self.0);
    }
}

/// How a document should be rendered.
///
/// ⚠️ [`AttachmentRender::Unpreviewable`] carries its own reason rather than
/// being a bare fallback. "No preview available" tells the user nothing they can
/// act on; naming the format tells them whether to open it elsewhere or whether
/// something is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentRender {
    Image,
    Pdf,
    /// Rendered as a table of the file's own rows. ⛔ Never the statement
    /// parser's interpretation of them — correcting a misread value means seeing
    /// what the file actually says.
    Csv,
    Text,
    Unpreviewable(&'static str),
}

/// ⛔ **HEIC is unpreviewable, and this is the fix for a live bug rather than a
/// limitation.** It used to route to the `<img>` branch, with a test asserting
/// that it did, while `core::extraction::media::prepare_image` refuses HEIC and
/// neither Chrome nor Android WebView decodes it — so the classification passed
/// and the render was blank. Transcoding on ingest is the other answer and stays
/// open; it needs a libheif dependency in core, and nothing in the corpus is
/// HEIC today. Saying so plainly beats a blank frame either way.
const HEIC_REASON: &str = "HEIC images cannot be decoded by this app's WebView";

fn extension(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Decide how to render, from the declared type and the filename together.
///
/// ⚠️ **The filename is consulted, and it has to be.** Bulk ingest labels plenty
/// of files `application/octet-stream` — `archive::mime_for` only recovers a type
/// for extensions it knows — so classifying on the MIME alone would drop real
/// CSVs into the unpreviewable branch. ⛔ The declared type still wins when it
/// says something specific; the extension is consulted only when it does not.
pub fn classify_attachment(mime: &str, filename: &str) -> AttachmentRender {
    let mime = mime.to_ascii_lowercase();

    if mime == "image/heic" || mime == "image/heif" {
        return AttachmentRender::Unpreviewable(HEIC_REASON);
    }
    if mime == "application/pdf" || mime == "application/x-pdf" {
        return AttachmentRender::Pdf;
    }
    if mime == "text/csv" || mime == "application/csv" {
        return AttachmentRender::Csv;
    }
    if mime.starts_with("image/") {
        return AttachmentRender::Image;
    }
    if mime.starts_with("text/") || mime == "application/json" {
        return AttachmentRender::Text;
    }

    // Nothing specific was declared. Fall back to what the name says.
    match extension(filename).as_str() {
        "heic" | "heif" => AttachmentRender::Unpreviewable(HEIC_REASON),
        "pdf" => AttachmentRender::Pdf,
        "csv" | "tsv" => AttachmentRender::Csv,
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "svg" => AttachmentRender::Image,
        "txt" | "md" | "json" | "log" | "ledger" => AttachmentRender::Text,
        _ => AttachmentRender::Unpreviewable("no preview for this format"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentMeta {
    pub sha256: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

/// Pull the four `AttachmentRef` fields out of the `serde_json::Value` the
/// backend ships. `None` when the field is null or a required key is missing —
/// defensive only; `record_transaction` validates upstream.
pub fn extract_attachment_meta(value: &serde_json::Value) -> Option<AttachmentMeta> {
    Some(AttachmentMeta {
        sha256: value.get("sha256")?.as_str()?.to_string(),
        filename: value.get("filename")?.as_str()?.to_string(),
        mime_type: value.get("mime_type")?.as_str()?.to_string(),
        size: value.get("size")?.as_u64().unwrap_or(0),
    })
}

/// Rows past this are not rendered.
///
/// ⚠️ A statement export runs to a few hundred rows and a transaction dump to
/// tens of thousands; one DOM node per cell is what makes the difference matter.
/// ⛔ Truncation is always stated in the UI — a table that silently stops is a
/// table someone will read as the whole file.
const MAX_CSV_ROWS: usize = 500;

/// Split one CSV line on commas, honouring double-quoted cells.
///
/// ⚠️ Deliberately its own small function rather than a shared one: the frontend
/// is a separate workspace from `omni-me-core`, and this reads *raw* rows for
/// display, which is a different job from parsing a statement into transactions.
/// ⛔ It must never grow interpretation — no type inference, no number
/// formatting, no header semantics.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                // An escaped quote inside a quoted cell: `""` is one `"`.
                cell.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => cells.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
    }
    cells.push(cell);
    cells
}

/// Every non-empty line of a CSV, split into cells, plus how many were dropped.
fn csv_rows(text: &str) -> (Vec<Vec<String>>, usize) {
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let shown = all.len().min(MAX_CSV_ROWS);
    let rows = all[..shown].iter().map(|l| split_csv_line(l)).collect();
    (rows, all.len() - shown)
}

/// What the fetch produced, in the shape the chosen branch needs.
///
/// ⚠️ Text branches decode instead of making an object URL, and image/PDF
/// branches do the reverse. Keeping both would hold every document twice.
enum Loaded {
    Url(ObjectUrlGuard),
    Text(String),
}

#[component]
pub fn AttachmentViewer(meta: AttachmentMeta) -> Element {
    let mut loaded: Signal<Option<Loaded>> = use_signal(|| None);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let render = classify_attachment(&meta.mime_type, &meta.filename);

    let sha256 = meta.sha256.clone();
    let mime = meta.mime_type.clone();
    let wants_text = matches!(render, AttachmentRender::Csv | AttachmentRender::Text);
    let needs_bytes = !matches!(render, AttachmentRender::Unpreviewable(_));

    use_effect(move || {
        if !needs_bytes {
            return;
        }
        let sha = sha256.clone();
        let mime = mime.clone();
        spawn(async move {
            match bridge::invoke_fetch_attachment(&sha).await {
                Ok(bytes) => {
                    if wants_text {
                        // Lossy on purpose, matching ingest: one bad byte in a
                        // statement export should not cost the whole preview.
                        loaded.set(Some(Loaded::Text(
                            String::from_utf8_lossy(&bytes).into_owned(),
                        )));
                    } else {
                        match ObjectUrlGuard::from_bytes(&bytes, &mime) {
                            Ok(guard) => loaded.set(Some(Loaded::Url(guard))),
                            Err(e) => error.set(Some(e)),
                        }
                    }
                }
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let size_kb = (meta.size as f64 / 1024.0).round() as u64;

    rsx! {
        div {
            h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2",
                "Attachment"
            }
            div { class: "p-3 bg-obsidian-sidebar/40 border border-obsidian-border/5 rounded-lg space-y-3",
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
                } else if let AttachmentRender::Unpreviewable(reason) = render {
                    UnpreviewableNotice { filename: meta.filename.clone(), reason: reason }
                } else {
                    match loaded.read().as_ref() {
                        None => rsx! {
                            div { class: "text-xs text-obsidian-text-muted", "Loading attachment…" }
                        },
                        Some(Loaded::Url(guard)) => {
                            let url = guard.url().to_string();
                            match render {
                                AttachmentRender::Pdf => rsx! { PdfView { url: url.clone() } },
                                _ => rsx! {
                                    img {
                                        src: "{url}",
                                        alt: "{meta.filename}",
                                        class: "max-w-full max-h-[600px] rounded border border-obsidian-border/10",
                                    }
                                },
                            }
                        }
                        Some(Loaded::Text(text)) => match render {
                            AttachmentRender::Csv => rsx! { CsvTable { text: text.clone() } },
                            _ => rsx! {
                                pre {
                                    class: "max-h-[600px] overflow-auto p-3 rounded border border-obsidian-border/10 \
                                            bg-obsidian-bg text-[11px] font-mono text-obsidian-text whitespace-pre-wrap",
                                    "{text}"
                                }
                            },
                        },
                    }
                }
            }
        }
    }
}

/// A PDF, rendered page by page onto canvases by pdf.js.
///
/// ⛔ **Not an `<iframe>`, and that is the whole point.** An iframe pointed at a
/// `blob:` URL renders on desktop webkit and shows *nothing* on Android WebView,
/// which has no built-in PDF renderer — so the corpus's most common document
/// type failed silently on the device most likely to read it. Canvases render
/// identically everywhere.
#[component]
fn PdfView(url: String) -> Element {
    let mut status: Signal<Option<Result<(u32, u32), String>>> = use_signal(|| None);

    use_effect(move || {
        let url = url.clone();
        spawn(async move {
            // ~10s: this bundle is loaded on demand rather than warmed at app
            // mount, so it is fetched while the page is already interactive.
            if !bridge::ensure_js_bundle(PDF_BUNDLE_SRC, "renderPdf", 100).await {
                status.set(Some(Err("the PDF viewer could not be loaded".into())));
                return;
            }
            match render_pdf(&url).await {
                Ok(counts) => status.set(Some(Ok(counts))),
                Err(e) => status.set(Some(Err(e))),
            }
        });
    });

    rsx! {
        div { class: "space-y-2",
            // ⚠️ Always present, never behind an `if`: the effect resolves the
            // container by id, so a conditionally-rendered node would not exist
            // yet when the render call goes looking for it.
            div {
                id: PDF_CONTAINER_ID,
                class: "max-h-[600px] overflow-auto rounded border border-obsidian-border/10 bg-white/95",
            }
            match status.read().as_ref() {
                None => rsx! {
                    p { class: "text-xs text-obsidian-text-muted", "Rendering…" }
                },
                Some(Err(e)) => rsx! {
                    p { class: "text-xs text-red-300", "{e}" }
                },
                // ⛔ Truncation is stated. A document that appears to stop at
                // page 30 otherwise reads as a complete document.
                Some(Ok((pages, rendered))) if rendered < pages => rsx! {
                    p { class: "text-[10px] text-obsidian-text-muted",
                        "Showing the first {rendered} of {pages} pages"
                    }
                },
                Some(Ok((pages, _))) => rsx! {
                    p { class: "text-[10px] text-obsidian-text-muted", "{pages} pages" }
                },
            }
        }
    }
}

/// Call `window.renderPdf(containerId, url)` and read back `{pages, rendered}`.
///
/// ⚠️ Reached through `Reflect` rather than a `#[wasm_bindgen]` extern: the
/// bundle is loaded on demand, so the global is genuinely absent until it lands
/// and a direct extern binding would trap instead of returning an error we can
/// show.
async fn render_pdf(url: &str) -> Result<(u32, u32), String> {
    let window = web_sys::window().ok_or("no window")?;
    let func = js_sys::Reflect::get(&window, &JsValue::from_str("renderPdf"))
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
        .ok_or("renderPdf is not available")?;

    let promise = func
        .call2(
            &JsValue::NULL,
            &JsValue::from_str(PDF_CONTAINER_ID),
            &JsValue::from_str(url),
        )
        .map_err(|e| format!("could not render this PDF: {e:?}"))?;

    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("could not render this PDF: {e:?}"))?;

    let read = |key: &str| -> u32 {
        js_sys::Reflect::get(&result, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u32
    };
    Ok((read("pages"), read("rendered")))
}

/// The file's own rows, as they are written.
#[component]
fn CsvTable(text: String) -> Element {
    let (rows, dropped) = csv_rows(&text);

    if rows.is_empty() {
        return rsx! {
            div { class: "text-xs text-obsidian-text-muted", "This file has no rows." }
        };
    }

    rsx! {
        div { class: "space-y-2",
            div { class: "max-h-[600px] overflow-auto rounded border border-obsidian-border/10",
                table { class: "w-full text-[11px] font-mono border-collapse",
                    tbody {
                        for (i, row) in rows.iter().enumerate() {
                            tr {
                                key: "{i}",
                                class: if i == 0 {
                                    "bg-obsidian-sidebar/60 sticky top-0"
                                } else {
                                    "odd:bg-obsidian-bg/40"
                                },
                                // ⚠️ The line number is the file's, not the
                                // table's — it is what a person types into an
                                // editor to go and look at the row.
                                td { class: "px-2 py-1 text-obsidian-text-muted/50 select-none text-right align-top",
                                    "{i + 1}"
                                }
                                for (j, cell) in row.iter().enumerate() {
                                    td {
                                        key: "{j}",
                                        class: "px-2 py-1 text-obsidian-text align-top whitespace-pre",
                                        "{cell}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if dropped > 0 {
                div { class: "text-[10px] text-obsidian-text-muted",
                    "Showing the first {MAX_CSV_ROWS} rows · {dropped} more not shown"
                }
            }
        }
    }
}

/// ⛔ Says what the file is and why it cannot be shown. No download button.
#[component]
fn UnpreviewableNotice(filename: String, reason: &'static str) -> Element {
    rsx! {
        div { class: "p-3 rounded border border-obsidian-border/10 bg-obsidian-bg/40 space-y-1",
            div { class: "text-xs text-obsidian-text", "No preview on this device" }
            div { class: "text-[11px] text-obsidian-text-muted", "{reason}." }
            div { class: "text-[10px] font-mono text-obsidian-text-muted/70", "{filename}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_type_decides_before_the_filename() {
        assert_eq!(
            classify_attachment("application/pdf", "whatever.csv"),
            AttachmentRender::Pdf
        );
        assert_eq!(
            classify_attachment("APPLICATION/PDF", "scan"),
            AttachmentRender::Pdf
        );
        assert_eq!(
            classify_attachment("image/jpeg", "receipt.pdf"),
            AttachmentRender::Image
        );
    }

    #[test]
    fn a_generic_type_falls_back_to_the_extension() {
        // ⚠️ The case that matters: bulk ingest labels files
        // `application/octet-stream`, and on MIME alone every one of the corpus's
        // CSVs would land in the unpreviewable branch.
        assert_eq!(
            classify_attachment("application/octet-stream", "statement-2026-01.csv"),
            AttachmentRender::Csv
        );
        assert_eq!(
            classify_attachment("application/octet-stream", "notice.pdf"),
            AttachmentRender::Pdf
        );
        assert_eq!(
            classify_attachment("", "photo.JPG"),
            AttachmentRender::Image
        );
    }

    #[test]
    fn heic_is_unpreviewable_by_either_route() {
        // ⛔ Regression guard. This used to classify as `Image`, with a test
        // asserting it, and rendered a blank frame — the WebView cannot decode
        // HEIC and `prepare_image` refuses it too.
        assert_eq!(
            classify_attachment("IMAGE/HEIC", "img.heic"),
            AttachmentRender::Unpreviewable(HEIC_REASON)
        );
        assert_eq!(
            classify_attachment("application/octet-stream", "IMG_0042.HEIF"),
            AttachmentRender::Unpreviewable(HEIC_REASON)
        );
    }

    #[test]
    fn an_unknown_format_says_so_rather_than_offering_a_download() {
        let r = classify_attachment("application/zip", "backup.zip");
        assert!(matches!(r, AttachmentRender::Unpreviewable(_)));
        assert_eq!(
            classify_attachment("", "no-extension"),
            AttachmentRender::Unpreviewable("no preview for this format")
        );
    }

    #[test]
    fn a_complete_attachment_ref_decodes() {
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
    fn a_ref_missing_its_hash_decodes_to_nothing() {
        let v = serde_json::json!({
            "filename": "x.pdf",
            "mime_type": "application/pdf",
            "size": 5000,
        });
        assert!(extract_attachment_meta(&v).is_none());
    }

    #[test]
    fn a_quoted_cell_keeps_its_commas_and_its_quotes() {
        assert_eq!(
            split_csv_line(r#"2026-01-05,"COFFEE, LARGE",-4.50"#),
            vec!["2026-01-05", "COFFEE, LARGE", "-4.50"]
        );
        assert_eq!(
            split_csv_line(r#"a,"say ""hi""",b"#),
            vec!["a", r#"say "hi""#, "b"]
        );
        assert_eq!(split_csv_line("a,,b"), vec!["a", "", "b"]);
    }

    #[test]
    fn rows_are_capped_and_the_remainder_is_reported() {
        let mut csv = String::from("date,amount\n");
        for i in 0..MAX_CSV_ROWS + 20 {
            csv.push_str(&format!("2026-01-01,{i}\n"));
        }
        let (rows, dropped) = csv_rows(&csv);
        assert_eq!(rows.len(), MAX_CSV_ROWS);
        // Header + 520 data rows = 521 lines; 500 shown.
        assert_eq!(
            dropped, 21,
            "a silently truncated table reads as the whole file"
        );
    }

    #[test]
    fn blank_lines_are_dropped_rather_than_rendered_as_empty_rows() {
        let (rows, dropped) = csv_rows("a,b\n\n   \nc,d\n");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
        assert_eq!(dropped, 0);
    }
}
