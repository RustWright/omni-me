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

/// Read whatever text the bytes themselves carry.
///
/// Returns `(text, source)`. Whitespace-only output counts as none: `pdftotext`
/// succeeds on a scanned page and hands back a run of newlines, and storing that
/// as a text layer would make the document look searchable while matching
/// nothing.
pub async fn derive_text(
    bytes: &[u8],
    mime: &str,
    passwords: &crate::credentials::PdfPasswords,
) -> (Option<String>, TextSource) {
    let text = if is_pdf(mime) {
        match pdf::extract_layout_text_with_any(bytes, &passwords.values()).await {
            Ok(opened) => {
                // The secret's NAME, never its value, and only when one was needed
                // — which password opened a document is the first thing to check
                // when one still arrives blank.
                if let Some(name) = opened.opened_by.and_then(|i| passwords.name_of(i)) {
                    tracing::info!(secret = name, "an encrypted pdf opened");
                }
                Some(opened.text)
            }
            Err(e) => {
                // Not an ingest failure. An encrypted or oversized PDF is still a
                // document worth keeping; it simply arrives without text.
                //
                // ⚠️ At `warn` when it is the password, because that one is fixable
                // and used to be invisible: 140 of 765 corpus statements landed here
                // and read as empty-but-fine.
                match &e {
                    pdf::PdfTextError::Encrypted { tried } => tracing::warn!(
                        tried,
                        configured = passwords.len(),
                        "an encrypted pdf stays textless — no configured password opened it"
                    ),
                    _ => tracing::debug!(error = %e, "no text layer could be read from a pdf"),
                }
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
            Ok(parsed) => {
                // The sent date reaches nothing else. `DocumentArchivedPayload`
                // carries `archived_at`, which is when this filed it, and the
                // blob holds the raw header nobody reads — so without this line
                // an archived email has no date any reader or search can use.
                //
                // Date only, not the timestamp: this text is a search index and
                // the anchor a reader copies `document_date` from, and that
                // wants YYYY-MM-DD. An omitted line when the header is absent or
                // unparseable is deliberate — a dateless email must stay
                // dateless rather than gain today's.
                let date = parsed
                    .date
                    .map(|d| format!("Date: {}\n", d.format("%Y-%m-%d")))
                    .unwrap_or_default();
                Some(format!(
                    "From: {}\nSubject: {}\n{date}\n{}",
                    parsed.from, parsed.subject, parsed.body_text
                ))
            }
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

/// Both spellings of an email type. Nothing here reads the extension, so an `.eml`
/// sent as `application/octet-stream` is filed by name only; the bulk upload
/// script names it `message/rfc822`.
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

/// Where a document is being filed, as whom, and with what to open it.
///
/// One struct because the three travel together through every ingest entry point,
/// and `ingest_one` crossed clippy's argument threshold the moment the passwords
/// joined them. ⚠️ It replaces `auto_import::imap::ArchiveTarget`, which was the
/// same triple built for one caller — two names for one idea is how the email path
/// and the upload path end up disagreeing about what ingest needs.
pub struct IngestContext<'a> {
    pub blob_dir: &'a Path,
    pub device_id: &'a str,
    /// Tried in turn on an encrypted PDF. See [`crate::credentials::PdfPasswords`].
    pub passwords: &'a crate::credentials::PdfPasswords,
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
    ctx: &IngestContext<'_>,
    bytes: &[u8],
    filename: &str,
    mime: &str,
    source: IngestSource,
    parent_document_id: Option<&str>,
) -> Result<Ingested, IngestError> {
    let sha256 = blob::store(ctx.blob_dir, bytes).await?;
    let (text, text_source) = derive_text(bytes, mime, ctx.passwords).await;

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

    let mut events = vec![NewEvent::document_archived(ctx.device_id, &payload)?];
    let parsed_fields = match document_fields::parser_fields(&document_id, bytes) {
        Some(fields) => {
            events.push(NewEvent::document_fields_extracted(ctx.device_id, &fields)?);
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
    ctx: &IngestContext<'_>,
    raw: &[u8],
    parsed: &crate::mime::ParsedMessage,
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
        ctx,
        raw,
        &filename,
        "message/rfc822",
        IngestSource::Email,
        None,
    )
    .await?;

    let email_document_id = email.document_id.clone();
    let mut events = email.events;
    if let Some(sent) = parsed.date {
        events.push(NewEvent::document_fields_extracted(
            ctx.device_id,
            &document_fields::email_date(&email_document_id, sent),
        )?);
    }
    let mut attachment_document_ids = Vec::new();

    for att in parsed.real_attachments() {
        let child = ingest_one(
            ctx,
            &att.bytes,
            &att.filename,
            &att.content_type,
            IngestSource::Email,
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

#[cfg(test)]
mod tests {
    /// No passwords configured, which is every test here: the encrypted path has
    /// its own coverage in `statement::pdf` and `extraction::media`, both against a
    /// real encrypted fixture.
    fn no_passwords() -> crate::credentials::PdfPasswords {
        crate::credentials::PdfPasswords::default()
    }

    /// The context these tests ingest into: a temp blob dir, device `dev`, and no
    /// passwords. Borrowed rather than returned by value so the `PdfPasswords` it
    /// points at outlives the call.
    fn ctx<'a>(
        blob_dir: &'a Path,
        passwords: &'a crate::credentials::PdfPasswords,
    ) -> IngestContext<'a> {
        IngestContext {
            blob_dir,
            device_id: "dev",
            passwords,
        }
    }

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
            &ctx(d.path(), &no_passwords()),
            csv,
            "chequing.csv",
            "text/csv",
            IngestSource::Bulk,
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
            &ctx(d.path(), &no_passwords()),
            &[0xFF, 0xD8, 0xFF, 0xE0, 0x00],
            "receipt.jpg",
            "image/jpeg",
            IngestSource::Scan,
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

    /// The gate itself, end to end: 140 of 765 corpus statements are encrypted and
    /// every one of them was archived with no text and no complaint.
    #[tokio::test]
    async fn an_encrypted_pdf_is_archived_with_its_text_once_a_password_is_configured() {
        let encrypted: &[u8] =
            include_bytes!("../tests/fixtures/encrypted/encrypted-statement.pdf");
        let mut secrets = std::collections::HashMap::new();
        secrets.insert("pdf_password_wrong".to_string(), "nope".to_string());
        secrets.insert("pdf_password_right".to_string(), "letmein".to_string());
        let configured = crate::credentials::PdfPasswords::from_secrets(&secrets);

        let d = dir();
        let ingested = ingest_one(
            &ctx(d.path(), &configured),
            encrypted,
            "statement.pdf",
            "application/pdf",
            IngestSource::Bulk,
            None,
        )
        .await
        .unwrap();

        assert_eq!(ingested.text_source, TextSource::Extracted);
        let text = archived_payload(&ingested).text.expect("text was read");
        assert!(text.contains("1284.00"), "{text}");

        // And the other half of the gate: with nothing configured the document is
        // still archived, just textless — never refused, never silently dropped.
        let bare = ingest_one(
            &ctx(d.path(), &no_passwords()),
            encrypted,
            "statement.pdf",
            "application/pdf",
            IngestSource::Bulk,
            None,
        )
        .await
        .unwrap();
        assert_eq!(bare.text_source, TextSource::None);
        assert!(archived_payload(&bare).text.is_none());
    }

    #[tokio::test]
    async fn whitespace_only_text_counts_as_no_text() {
        // `pdftotext` on a scanned page succeeds and returns newlines. Stored as
        // a text layer, that document would look searchable and match nothing.
        let (text, source) = derive_text(b"   \n\n\t  \n", "text/plain", &no_passwords()).await;
        assert!(text.is_none());
        assert_eq!(source, TextSource::None);
    }

    #[tokio::test]
    async fn the_same_bytes_from_two_sources_are_two_documents_one_blob() {
        let d = dir();
        let bytes = b"date,amount\n2026-03-01,-10.00\n";
        let a = ingest_one(
            &ctx(d.path(), &no_passwords()),
            bytes,
            "s.csv",
            "text/csv",
            IngestSource::Email,
            None,
        )
        .await
        .unwrap();
        let b = ingest_one(
            &ctx(d.path(), &no_passwords()),
            bytes,
            "s.csv",
            "text/csv",
            IngestSource::Scan,
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
        let out = ingest_email(&ctx(d.path(), &no_passwords()), &raw, &parsed)
            .await
            .unwrap();

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

    /// Retention counts from this date, so old mail backfilled today is old.
    #[tokio::test]
    async fn an_emails_own_date_is_filed_at_parser_rank() {
        let raw = b"From: shop@example.com\r\n\
                    Date: Tue, 05 Mar 2024 14:00:00 +0000\r\n\
                    Subject: Spring sale\r\n\
                    Content-Type: text/plain\r\n\r\n\
                    Everything 20% off.\r\n";
        let parsed = crate::mime::parse_eml(raw).expect("parses");
        let d = dir();
        let out = ingest_email(&ctx(d.path(), &no_passwords()), raw, &parsed)
            .await
            .unwrap();

        let fields: crate::events::DocumentFieldsExtractedPayload = out
            .events
            .iter()
            .find(|e| e.event_type == "document_fields_extracted")
            .map(|e| serde_json::from_value(e.payload.clone()).unwrap())
            .expect("a dated email files its date");
        assert_eq!(fields.document_id, out.email_document_id);
        assert_eq!(fields.fields[0].key, crate::events::DOCUMENT_DATE_KEY);
        assert_eq!(fields.fields[0].value, "2024-03-05");
        assert_eq!(fields.fields[0].source, "parser:email");
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
        let out = ingest_email(&ctx(d.path(), &no_passwords()), raw, &parsed)
            .await
            .unwrap();

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

    #[tokio::test]
    async fn an_emails_sent_date_reaches_the_text_that_gets_indexed() {
        // It reaches nothing else: `archived_at` is when this filed the message,
        // and the raw header sits in a blob nobody reads. Role C2 is asked for
        // `document_date` off this text, so a missing line means a missing date.
        let raw = b"From: billing@example.com\r\n\
                    Subject: February invoice\r\n\
                    Date: Sat, 14 Feb 2026 09:31:00 +0000\r\n\
                    Content-Type: text/plain\r\n\r\n\
                    Amount due 142.65.\r\n";
        let (text, source) = derive_text(raw, "message/rfc822", &no_passwords()).await;

        let text = text.expect("an email with a body has text");
        assert!(text.contains("Date: 2026-02-14"), "got: {text}");
        assert!(text.contains("Amount due 142.65."));
        assert_eq!(source, TextSource::Extracted);
    }

    #[tokio::test]
    async fn an_email_with_no_date_header_stays_dateless() {
        // ⛔ No fallback to today. A guessed date files the message under the
        // wrong month and nothing downstream can tell it was invented — the same
        // rule `document_prompt` states for `document_date`.
        let raw = b"From: a@b.test\r\nSubject: no date here\r\n\r\nBody.\r\n";
        let (text, _) = derive_text(raw, "message/rfc822", &no_passwords()).await;

        let text = text.expect("still has a body");
        assert!(!text.contains("Date:"), "got: {text}");
        assert!(text.contains("Body."));
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
            &ctx(d.path(), &no_passwords()),
            csv,
            "brokerage.csv",
            "text/csv",
            IngestSource::Bulk,
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
}
