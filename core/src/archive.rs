//! Document ingest — bytes in, a blob and the events that record them out.
//!
//! Why the archive is three events, how fields fold, and why text travels in
//! the event: `docs/src/archive.md`.
//!
//! ⛔ **No model in here.** The line is the network, not the event type:
//! deterministic work that needs nothing remote runs at ingest, and everything
//! that reaches an endpoint is scheduled and batched elsewhere. So a PDF's text
//! layer is lifted here, and a statement's fields are parsed here — but reading
//! a scan needs a model, and its answer arrives later as
//! `DocumentTextTranscribed`.
//!
//! Doing either remotely at ingest would put a round-trip in the path that files
//! a document: a capture taken offline would fail to archive, and every document
//! filed before the feature existed would stay unreadable anyway, because
//! `DocumentArchived` is written once and never revised.

use std::path::Path;

use crate::blob;
use crate::document_fields;
use crate::events::{DocumentArchivedPayload, NewEvent};
use crate::statement::pdf;

/// Where a document's text came from, as stored in
/// [`DocumentArchivedPayload::text_source`].
///
/// ⚠️ Kept distinct because the two are not equally trustworthy and a reader who
/// cannot tell them apart will trust both the same. Extracted text is what the
/// file says; transcribed text is what a model thinks it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSource {
    /// Lifted from a text layer the file actually carried.
    Extracted,
    /// Read off an image by a model. ⚠️ Never produced here — see the module note.
    Transcribed,
    /// Nothing could read one. An ordinary outcome, not a failure.
    None,
}

impl TextSource {
    pub fn as_str(self) -> &'static str {
        match self {
            TextSource::Extracted => "extracted",
            TextSource::Transcribed => "transcribed",
            TextSource::None => "none",
        }
    }

    /// How far one origin for a document's text outranks another.
    ///
    /// The counterpart to `DocumentField::rank`, and load-bearing for the same
    /// reason: `documents_projection::write_text_if_it_outranks` compares this
    /// before arrival order, so two events racing over one row settle by where
    /// their text came from rather than by which device's clock ran ahead.
    ///
    /// Takes `&str` rather than `Self` because the value it compares against is
    /// read back out of the projection as a column.
    pub fn rank(source: &str) -> u8 {
        match source {
            // What the file itself states, above any reading of it.
            "extracted" => 2,
            // ⚠️ Nothing may rank below `none`, or an un-transcribed scan could
            // not be given text at all — which is the entire point of the
            // transcription pass.
            "none" => 0,
            // `transcribed`, and deliberately anything a later build introduces.
            // ⚠️ An unrecognized source ranks **above** `none` on purpose. The
            // two errors are not symmetric: overwriting real text with `None`
            // makes a document unsearchable on every device and unrecoverable on
            // most, because blobs do not sync and only the capturing device
            // holds the bytes to re-read. Keeping text of uncertain origin costs
            // a less trustworthy search hit. Preservation is the cheaper mistake.
            _ => 1,
        }
    }
}

/// How a document reached the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestSource {
    Scan,
    Upload,
    Email,
    Bulk,
}

impl IngestSource {
    pub fn as_str(self) -> &'static str {
        match self {
            IngestSource::Scan => "scan",
            IngestSource::Upload => "upload",
            IngestSource::Email => "email",
            IngestSource::Bulk => "bulk",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("{0}")]
    Blob(#[from] blob::BlobError),
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("payload could not be serialized: {0}")]
    Payload(#[from] serde_json::Error),
}

/// What one batch run did, in terms that add up.
///
/// ⚠️ `seen == archived + failed`, asserted by [`Self::check_accounting`]. The
/// shape is `auto_import::ParseOutcome`'s and it exists for the same reason: a
/// bare count of successes is unfalsifiable, and a file that fell out of the loop
/// without being classified is invisible. Over a corpus of several hundred, a
/// clean-looking partial import is the failure that costs the most to discover
/// later.
#[derive(Debug, Default)]
pub struct IngestReport {
    /// Files the walk yielded, before anything was attempted.
    pub seen: usize,
    /// Documents filed. ⚠️ **Counted, not `events.len()`** — one file now yields
    /// two events when a parser recognises it, and counting envelopes would make
    /// [`Self::check_accounting`] fail on precisely the runs that went well.
    pub archived: usize,
    /// Everything to append, in the order it happened.
    ///
    /// ⛔ Not one document's worth. Read [`Self::archived`] for a count.
    pub events: Vec<NewEvent>,
    /// `(path, reason)` per file that could not be archived at all.
    pub failed: Vec<(String, String)>,
    /// Archived, but with no text layer — so findable by name and not by content.
    ///
    /// ⚠️ Not a failure and deliberately not counted as one. It is reported
    /// separately because a batch that archives 700 files and can read none of
    /// them is technically a success and practically a problem.
    pub without_text: usize,
    /// Archived with parser-derived fields, so findable by *condition* and not
    /// only by content.
    ///
    /// ⚠️ Reported for `without_text`'s reason, inverted. Format discovery is a
    /// signature match against real exports the fixtures only approximate, so a
    /// run over 276 CSVs that claims none of them has silently degraded to
    /// filename search — and every other number in this report would still look
    /// perfect.
    pub with_parser_fields: usize,
}

impl IngestReport {
    pub fn check_accounting(&self) -> Result<(), String> {
        let accounted = self.archived + self.failed.len();
        if accounted != self.seen {
            return Err(format!(
                "ingest did not account for every file: saw {} but classified {} \
                 ({} archived + {} failed)",
                self.seen,
                accounted,
                self.archived,
                self.failed.len()
            ));
        }
        Ok(())
    }

    /// The first few failures, for a log line that is actionable rather than a
    /// number. ⛔ A count alone tells nobody which files to go and look at.
    pub fn sample_failures(&self, n: usize) -> Vec<&(String, String)> {
        self.failed.iter().take(n).collect()
    }
}

/// Read whatever text the bytes themselves carry.
///
/// Returns `(text, source)`. Whitespace-only output counts as none: `pdftotext`
/// succeeds on a scanned page and hands back a run of newlines, and storing that
/// as a text layer would make the document look searchable while matching
/// nothing.
pub async fn derive_text(bytes: &[u8], mime: &str) -> (Option<String>, TextSource) {
    let text = if is_pdf(mime) {
        match pdf::extract_layout_text(bytes, "").await {
            Ok(t) => Some(t),
            Err(e) => {
                // Not an ingest failure. An encrypted or oversized PDF is still a
                // document worth keeping; it simply arrives without text.
                tracing::debug!(error = %e, "no text layer could be read from a pdf");
                None
            }
        }
    } else if is_email(mime) {
        // ⚠️ **Its headers and body, not its raw source.** An `.eml` is mostly
        // MIME scaffolding and base64 attachment payloads; indexing those bytes
        // makes every message match nothing a person would search for while
        // looking perfectly searchable. Three of the real fixtures carry their
        // receipt *in the body*, so this is the only text those documents have.
        match crate::mime::parse_eml(bytes) {
            Ok(parsed) => Some(format!(
                "From: {}\nSubject: {}\n\n{}",
                parsed.from, parsed.subject, parsed.body_text
            )),
            Err(e) => {
                // Same rule as a PDF with no text layer: still a document.
                tracing::debug!(error = %e, "could not read an email's body");
                None
            }
        }
    } else if mime.starts_with("text/") {
        // Lossy on purpose: a CSV with one bad byte is still worth indexing, and
        // refusing the whole file over it would be the wrong trade.
        Some(String::from_utf8_lossy(bytes).into_owned())
    } else {
        None
    };

    match text {
        Some(t) if !t.trim().is_empty() => (Some(t), TextSource::Extracted),
        _ => (None, TextSource::None),
    }
}

/// Permissive, matching `finances.rs::looks_like_pdf`: a misdeclared type is
/// common enough that refusing on the header alone loses real documents.
fn is_pdf(mime: &str) -> bool {
    let mime = mime.to_ascii_lowercase();
    mime == "application/pdf" || mime == "application/x-pdf"
}

/// ⚠️ Both spellings: `message/rfc822` is what this project files an email
/// under, and `.eml` arriving from a share sheet or a file picker is commonly
/// labelled `application/mbox` or nothing at all — `mime_for` recovers the
/// former from the extension.
fn is_email(mime: &str) -> bool {
    let mime = mime.to_ascii_lowercase();
    mime == "message/rfc822" || mime == "message/global"
}

/// Everything one ingest produced.
///
/// A struct rather than a tuple because the events are no longer one: a file a
/// parser recognises yields its fields too, and a caller that pattern-matched
/// `(event, source)` would go on compiling while dropping them.
#[derive(Debug)]
pub struct Ingested {
    /// Append all of these, in this order.
    ///
    /// `DocumentArchived` first, then a `DocumentFieldsExtracted` if a parser
    /// claimed the file. ⚠️ The projection tolerates either order — it has to,
    /// since sync delivers by the authoring device's clock — but there is no
    /// reason to hand it a puzzle it only has to solve for the cross-device case.
    pub events: Vec<NewEvent>,
    pub document_id: String,
    pub sha256: String,
    pub text_source: TextSource,
    /// Whether a parser recognised the file and produced fields for it.
    pub parsed_fields: bool,
}

/// Store one document's bytes and build the events that record it.
///
/// ⛔ Returns them rather than appending them — see the module note.
///
/// Fields are derived here, and only the parser's: it is deterministic and needs
/// nothing remote. ⛔ **Here rather than in the callers**, because this is the
/// one function that knows a document has just been created — `POST
/// /documents/archive` is the only caller today, and the Tauri app links this
/// crate directly, so a device capture would be the second. A derivation copied
/// into each of them drifts apart the first time one is touched.
/// ⚠️ If a large file ever makes [`document_fields::parser_fields`] block long
/// enough to matter, move the call off this await — never back into the callers.
pub async fn ingest_one(
    blob_dir: &Path,
    bytes: &[u8],
    filename: &str,
    mime: &str,
    source: IngestSource,
    device_id: &str,
    parent_document_id: Option<&str>,
) -> Result<Ingested, IngestError> {
    let sha256 = blob::store(blob_dir, bytes).await?;
    let (text, text_source) = derive_text(bytes, mime).await;

    let payload = DocumentArchivedPayload {
        // ⚠️ A fresh id per ingest, deliberately not the hash. The same file
        // arriving twice from two sources is two archive entries — see
        // `DocumentArchivedPayload::document_id`.
        document_id: ulid::Ulid::new().to_string(),
        sha256: sha256.clone(),
        filename: filename.to_string(),
        mime_type: mime.to_string(),
        size: bytes.len() as u64,
        archived_at: chrono::Utc::now().to_rfc3339(),
        source: source.as_str().to_string(),
        text,
        text_source: text_source.as_str().to_string(),
        parent_document_id: parent_document_id.map(str::to_string),
    };
    let document_id = payload.document_id.clone();

    let mut events = vec![NewEvent::document_archived(device_id, &payload)?];
    let parsed_fields = match document_fields::parser_fields(&document_id, bytes) {
        Some(fields) => {
            events.push(NewEvent::document_fields_extracted(device_id, &fields)?);
            true
        }
        None => false,
    };

    Ok(Ingested {
        events,
        document_id,
        sha256,
        text_source,
        parsed_fields,
    })
}

/// What one email put into the archive.
#[derive(Debug)]
pub struct IngestedEmail {
    /// Every event, message first then its attachments in order.
    pub events: Vec<NewEvent>,
    /// The message's own document. ⚠️ **This is what a draft transaction should
    /// reference**, not an attachment: the batch is per-message, and a reviewer
    /// reaching the message can walk down to whatever came with it, while one
    /// starting at an attachment cannot see the envelope that carried it.
    pub email_document_id: String,
    /// One per `disposition: attachment` part, in the order they appeared.
    pub attachment_document_ids: Vec<String>,
    /// Parts deliberately left uncatalogued — the `cid:` images an HTML body
    /// renders. ⚠️ Reported rather than dropped quietly, because "this email
    /// had 6 parts and the archive shows 1" is otherwise unexplainable, and
    /// because a sender mislabelling a real file as inline would be invisible.
    pub inline_parts_skipped: usize,
}

/// Archive an email as a document, plus one document per real attachment.
///
/// ⛔ **The message is archived WHOLE, raw bytes.** Three of the five real
/// fixtures carry no attachment at all — the receipt *is* the body — so an
/// attachment-only rule files nothing for them. It is also what makes skipping
/// the inline parts lossless: those images are still inside this document,
/// they just do not each get a catalogue entry.
///
/// Attachments become documents in their own right — own text, own parser
/// fields, correctable on their own — linked back by
/// [`DocumentArchivedPayload::parent_document_id`].
///
/// `raw` is the `.eml` as fetched; `parsed` is [`crate::mime::parse_eml`]'s
/// reading of those same bytes. Both are taken because the caller has already
/// parsed it to decide whether the message was interesting at all, and parsing
/// twice to save one argument would be the wrong trade.
///
/// ⛔ Returns events rather than appending them, like every other ingest here.
pub async fn ingest_email(
    blob_dir: &Path,
    raw: &[u8],
    parsed: &crate::mime::ParsedMessage,
    device_id: &str,
) -> Result<IngestedEmail, IngestError> {
    // A subject is the only name a person would recognise, but it is
    // sender-controlled and may be empty or absurd. `.eml` keeps it openable.
    let subject = parsed.subject.trim();
    let filename = if subject.is_empty() {
        "message.eml".to_string()
    } else {
        format!("{}.eml", sanitize_filename(subject))
    };

    let email = ingest_one(
        blob_dir,
        raw,
        &filename,
        "message/rfc822",
        IngestSource::Email,
        device_id,
        None,
    )
    .await?;

    let email_document_id = email.document_id.clone();
    let mut events = email.events;
    let mut attachment_document_ids = Vec::new();

    for att in parsed.real_attachments() {
        let child = ingest_one(
            blob_dir,
            &att.bytes,
            &att.filename,
            &att.content_type,
            IngestSource::Email,
            device_id,
            Some(&email_document_id),
        )
        .await?;
        attachment_document_ids.push(child.document_id);
        events.extend(child.events);
    }

    let inline_parts_skipped = parsed.attachments.len() - attachment_document_ids.len();
    Ok(IngestedEmail {
        events,
        email_document_id,
        attachment_document_ids,
        inline_parts_skipped,
    })
}

/// Make a subject safe to use as a filename without making it unrecognisable.
///
/// ⚠️ Path separators and control characters only. A subject is the one human
/// label the message has, so stripping punctuation or transliterating emoji
/// would trade a real identifier for a tidy one — and `📫 oxio invoice` is an
/// actual fixture.
fn sanitize_filename(subject: &str) -> String {
    const MAX: usize = 120;
    let cleaned: String = subject
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    // Truncate on a char boundary; byte slicing would panic on the emoji.
    cleaned
        .chars()
        .take(MAX)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Ingest every path given, carrying on past the ones that fail.
///
/// ⛔ One unreadable file must not end the run. A backfill over several hundred
/// documents that aborts on the first permission error has done nothing and says
/// nothing about the other six hundred.
pub async fn ingest_paths(
    blob_dir: &Path,
    paths: &[std::path::PathBuf],
    source: IngestSource,
    device_id: &str,
) -> IngestReport {
    let mut report = IngestReport {
        seen: paths.len(),
        ..Default::default()
    };

    for path in paths {
        let shown = path.display().to_string();
        let bytes = match tokio::fs::read(path).await {
            Ok(b) => b,
            Err(e) => {
                report.failed.push((shown, format!("read: {e}")));
                continue;
            }
        };
        let filename = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".to_string());
        let mime = mime_for(path, &bytes);

        match ingest_one(blob_dir, &bytes, &filename, &mime, source, device_id, None).await {
            Ok(ingested) => {
                if ingested.text_source == TextSource::None {
                    report.without_text += 1;
                }
                if ingested.parsed_fields {
                    report.with_parser_fields += 1;
                }
                report.archived += 1;
                report.events.extend(ingested.events);
            }
            Err(e) => report.failed.push((shown, e.to_string())),
        }
    }

    report
}

/// Every file under `root`, recursively, in a stable order.
///
/// ⚠️ **Sorted, and that is not cosmetic.** Directory order is filesystem order,
/// so an interrupted backfill resumed later would visit files in a different
/// sequence and its report would not line up with the first run's. Sorting makes
/// "the same 765 files" mean the same list twice.
///
/// `skip` names extensions to leave out — the finance corpus carries 785
/// generated `.ledger` files beside its documents, and archiving a derived
/// artifact alongside the thing it was derived from is how an archive stops being
/// worth searching. ⛔ Compared lowercased; a `.LEDGER` is the same file.
///
/// Unreadable directories are reported rather than skipped silently: a
/// permission error on one subtree means the backfill covered less than it says.
pub fn walk_dir(root: &Path, skip: &[&str]) -> (Vec<std::path::PathBuf>, Vec<(String, String)>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                errors.push((dir.display().to_string(), format!("read_dir: {e}")));
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let skipped = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .is_some_and(|e| skip.contains(&e.as_str()));
            if !skipped {
                files.push(path);
            }
        }
    }

    files.sort();
    (files, errors)
}

/// Best-effort content type: sniff the bytes, fall back to the extension.
///
/// ⚠️ Sniffing first is deliberate. Extensions in a backup folder are whatever
/// the exporting bank chose, and `infer` reads the magic number — but it knows
/// nothing about CSV, which has no magic number and is a large fraction of the
/// corpus, so the extension has to be the fallback rather than the other way
/// round.
fn mime_for(path: &Path, bytes: &[u8]) -> String {
    if let Some(kind) = infer::get(bytes) {
        return kind.mime_type().to_string();
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("csv") => "text/csv".to_string(),
        Some("txt") => "text/plain".to_string(),
        Some("md") => "text/markdown".to_string(),
        Some("json") => "application/json".to_string(),
        // ⚠️ Load-bearing, not completeness: without it a bulk-ingested `.eml`
        // is `application/octet-stream`, `derive_text` skips its body, and the
        // message is filed searchable by filename only.
        Some("eml") => "message/rfc822".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// The archive event, which is always `events[0]`.
    fn archived_payload(ingested: &Ingested) -> DocumentArchivedPayload {
        assert_eq!(ingested.events[0].event_type, "document_archived");
        serde_json::from_value(ingested.events[0].payload.clone()).unwrap()
    }

    #[tokio::test]
    async fn a_csv_is_archived_with_its_text_readable() {
        let d = dir();
        let csv = b"date,description,amount\n2026-03-01,RENT,-1450.00\n";
        let ingested = ingest_one(
            d.path(),
            csv,
            "chequing.csv",
            "text/csv",
            IngestSource::Bulk,
            "dev",
            None,
        )
        .await
        .unwrap();

        assert_eq!(ingested.text_source, TextSource::Extracted);
        let payload = archived_payload(&ingested);
        assert!(payload.text.unwrap().contains("RENT"));
        assert_eq!(payload.size, csv.len() as u64);
    }

    #[tokio::test]
    async fn a_file_with_no_readable_text_is_still_archived() {
        // The whole reason ingest and extraction are separate events. An image
        // carries no text layer, and refusing it would mean the archive cannot
        // hold a photograph of a document.
        let d = dir();
        let ingested = ingest_one(
            d.path(),
            &[0xFF, 0xD8, 0xFF, 0xE0, 0x00],
            "receipt.jpg",
            "image/jpeg",
            IngestSource::Scan,
            "dev",
            None,
        )
        .await
        .unwrap();

        assert_eq!(ingested.text_source, TextSource::None);
        assert_eq!(ingested.events.len(), 1, "no parser claims a jpeg");
        let payload = archived_payload(&ingested);
        assert!(payload.text.is_none());
        assert_eq!(payload.text_source, "none");
    }

    #[tokio::test]
    async fn whitespace_only_text_counts_as_no_text() {
        // `pdftotext` on a scanned page succeeds and returns newlines. Stored as
        // a text layer, that document would look searchable and match nothing.
        let (text, source) = derive_text(b"   \n\n\t  \n", "text/plain").await;
        assert!(text.is_none());
        assert_eq!(source, TextSource::None);
    }

    #[tokio::test]
    async fn the_same_bytes_from_two_sources_are_two_documents_one_blob() {
        let d = dir();
        let bytes = b"date,amount\n2026-03-01,-10.00\n";
        let a = ingest_one(
            d.path(),
            bytes,
            "s.csv",
            "text/csv",
            IngestSource::Email,
            "dev",
            None,
        )
        .await
        .unwrap();
        let b = ingest_one(
            d.path(),
            bytes,
            "s.csv",
            "text/csv",
            IngestSource::Scan,
            "dev",
            None,
        )
        .await
        .unwrap();

        let pa = archived_payload(&a);
        let pb = archived_payload(&b);
        assert_eq!(pa.sha256, pb.sha256, "identical bytes are one blob");
        assert_ne!(
            pa.document_id, pb.document_id,
            "but two archive entries — they arrived differently and each is a real filing"
        );
        assert_eq!(pa.source, "email");
        assert_eq!(pb.source, "scan");
    }

    #[tokio::test]
    async fn a_batch_carries_on_past_an_unreadable_file_and_accounts_for_it() {
        let d = dir();
        let src = dir();
        let good = src.path().join("statement.csv");
        std::fs::write(&good, b"date,amount\n2026-01-01,5.00\n").unwrap();
        let missing = src.path().join("not-here.pdf");

        let report = ingest_paths(
            d.path(),
            &[good, missing.clone()],
            IngestSource::Bulk,
            "dev",
        )
        .await;

        assert_eq!(report.seen, 2);
        assert_eq!(report.archived, 1);
        assert_eq!(report.failed.len(), 1);
        report.check_accounting().expect("every file classified");
        assert!(report.sample_failures(5)[0].0.contains("not-here.pdf"));
    }

    #[tokio::test]
    async fn a_batch_reports_how_many_arrived_without_text() {
        // A run that archives everything and can read none of it is technically a
        // success; this is the number that says otherwise.
        let d = dir();
        let src = dir();
        let readable = src.path().join("a.csv");
        std::fs::write(&readable, b"x,y\n1,2\n").unwrap();
        let opaque = src.path().join("b.bin");
        std::fs::write(&opaque, [0u8, 1, 2, 3]).unwrap();

        let report = ingest_paths(d.path(), &[readable, opaque], IngestSource::Bulk, "dev").await;

        assert_eq!(report.archived, 2);
        assert_eq!(report.without_text, 1);
        report.check_accounting().unwrap();
    }

    /// A real bank's statement email: 5 inline JPEGs of branding and 1 PDF.
    /// ⚠️ Against the actual fixture, not a hand-built message — the whole risk
    /// here is what a real sender does.
    #[tokio::test]
    async fn an_email_becomes_a_document_with_its_attachment_beneath_it() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".reference/imap poller")
            .join("Your Estatement on 30042026 now available.eml");
        let Ok(raw) = std::fs::read(&path) else {
            eprintln!("fixture missing — skipping");
            return;
        };
        let parsed = crate::mime::parse_eml(&raw).expect("fixture parses");

        let d = dir();
        let out = ingest_email(d.path(), &raw, &parsed, "dev").await.unwrap();

        assert_eq!(out.attachment_document_ids.len(), 1, "one real attachment");
        assert_eq!(
            out.inline_parts_skipped, 5,
            "⛔ the branding images must not each become a document"
        );

        // The message is archived whole, so the skipped images are still in it.
        let email: DocumentArchivedPayload =
            serde_json::from_value(out.events[0].payload.clone()).unwrap();
        assert_eq!(email.mime_type, "message/rfc822");
        assert!(email.parent_document_id.is_none(), "the email is the root");
        assert_eq!(
            email.size as usize,
            raw.len(),
            "⚠️ the RAW message — that is what makes dropping the inline parts lossless"
        );
        assert!(
            email.text.unwrap_or_default().len() > 20,
            "the body is the email's text, so it stays searchable by content"
        );

        // The attachment is a document in its own right, linked back.
        let child = out
            .events
            .iter()
            .filter_map(|e| {
                serde_json::from_value::<DocumentArchivedPayload>(e.payload.clone()).ok()
            })
            .find(|p| p.mime_type.starts_with("application/pdf"))
            .expect("the statement PDF is archived");
        assert_eq!(
            child.parent_document_id.as_deref(),
            Some(out.email_document_id.as_str())
        );
        assert_ne!(
            child.sha256, email.sha256,
            "different bytes, different blob"
        );
        assert_eq!(child.source, "email");
    }

    /// ⚠️ Three of five real fixtures are like this: the receipt IS the body.
    /// Attachment-only archiving would file nothing at all for them.
    #[tokio::test]
    async fn an_email_with_no_attachments_is_still_one_document() {
        let raw = b"From: shop@example.com\r\n\
                    Subject: Thanks, your order is complete\r\n\
                    Content-Type: text/plain\r\n\r\n\
                    Order total CAD 42.10. Thanks!\r\n";
        let parsed = crate::mime::parse_eml(raw).expect("parses");

        let d = dir();
        let out = ingest_email(d.path(), raw, &parsed, "dev").await.unwrap();

        assert!(out.attachment_document_ids.is_empty());
        assert_eq!(out.events.len(), 1, "one document, no children");
        let email: DocumentArchivedPayload =
            serde_json::from_value(out.events[0].payload.clone()).unwrap();
        assert!(email.text.unwrap().contains("42.10"));
        assert_eq!(
            email.filename, "Thanks, your order is complete.eml",
            "named by its subject, so it is recognisable in a list"
        );
    }

    #[test]
    fn a_subject_stays_recognisable_but_cannot_escape_its_directory() {
        assert_eq!(sanitize_filename("a/b\\c"), "a-b-c");
        // ⚠️ An actual fixture is named `📫 oxio invoice available.` — dropping
        // non-ASCII would trade a real identifier for a tidy one.
        assert_eq!(sanitize_filename("📫 oxio invoice"), "📫 oxio invoice");
        assert_eq!(sanitize_filename("  padded  "), "padded");
        // Truncation counts chars, not bytes: byte slicing would panic here.
        let long = "📫".repeat(200);
        assert_eq!(sanitize_filename(&long).chars().count(), 120);
    }

    #[tokio::test]
    async fn a_statement_is_archived_and_parsed_in_one_ingest() {
        let d = dir();
        let csv = b"Date,Amount,Balance\n2026-01-05,-20.00,980.00\n2026-01-09,-30.00,950.00\n";
        let ingested = ingest_one(
            d.path(),
            csv,
            "brokerage.csv",
            "text/csv",
            IngestSource::Bulk,
            "dev",
            None,
        )
        .await
        .unwrap();

        assert!(ingested.parsed_fields);
        assert_eq!(
            ingested
                .events
                .iter()
                .map(|e| e.event_type.as_str())
                .collect::<Vec<_>>(),
            vec!["document_archived", "document_fields_extracted"],
            "the archive event first — they are appended in the order they happened"
        );
        assert!(
            ingested
                .events
                .iter()
                .all(|e| e.aggregate_id == ingested.document_id),
            "both fold onto the row the bytes created, or the fields belong to nothing"
        );
    }

    #[tokio::test]
    async fn the_counters_still_add_up_when_one_file_yields_two_events() {
        // ⚠️ The regression this guards: `archived` counted envelopes once, so a
        // parsed file scored 2 and `check_accounting` — the thing built to catch
        // a miscount — would fail on precisely the runs that went well.
        let d = dir();
        let src = dir();
        let parsed = src.path().join("brokerage.csv");
        std::fs::write(
            &parsed,
            b"Date,Amount,Balance\n2026-01-05,-20.00,980.00\n2026-01-09,-30.00,950.00\n",
        )
        .unwrap();
        let plain = src.path().join("letter.txt");
        std::fs::write(&plain, b"a letter no parser claims\n").unwrap();

        let report = ingest_paths(d.path(), &[parsed, plain], IngestSource::Bulk, "dev").await;

        report.check_accounting().expect("two files, two documents");
        assert_eq!(report.archived, 2);
        assert_eq!(report.with_parser_fields, 1);
        assert_eq!(report.events.len(), 3, "2 archived + 1 fields");
    }

    #[test]
    fn a_walk_recurses_sorts_and_skips_derived_artifacts() {
        let root = dir();
        std::fs::create_dir_all(root.path().join("cra/2023")).unwrap();
        std::fs::write(root.path().join("cra/2023/notice.pdf"), b"x").unwrap();
        std::fs::write(root.path().join("zz.csv"), b"x").unwrap();
        std::fs::write(root.path().join("aa.csv"), b"x").unwrap();
        // The corpus carries 785 of these beside the documents they came from.
        std::fs::write(root.path().join("main.ledger"), b"x").unwrap();
        std::fs::write(root.path().join("OTHER.LEDGER"), b"x").unwrap();

        let (files, errors) = walk_dir(root.path(), &["ledger"]);

        assert!(errors.is_empty());
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["aa.csv", "notice.pdf", "zz.csv"],
            "recursive, sorted, and neither .ledger case included"
        );
    }

    #[test]
    fn csv_falls_back_to_its_extension_because_it_has_no_magic_number() {
        let p = Path::new("/tmp/export.csv");
        assert_eq!(mime_for(p, b"date,amount\n"), "text/csv");
    }

    #[test]
    fn a_sniffed_type_beats_a_wrong_extension() {
        // Backup folders carry whatever the exporting bank named the file.
        let p = Path::new("/tmp/statement.txt");
        assert_eq!(
            mime_for(p, b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n"),
            "application/pdf"
        );
    }
}
