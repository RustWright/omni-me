//! Generic receipt-email handler.
//!
//! Accepts mail from a user-configured list of sender domains/patterns,
//! pulls the body text (and concatenates any text-extractable PDF
//! attachments via pdftotext), and forwards the combined text to a
//! `DocumentExtractor` with `ExtractionHint::EmailBody`.
//!
//! One handler instance per logical category; users can configure several
//! (e.g. one for subscriptions, one for utilities) or just one catch-all
//! that lists every sender. The dispatch loop (`imap::poll_once`) routes
//! each message to the first handler that claims it.
//!
//! Emits one `AutoImportBatchProposed` per message that yields drafts, built by
//! `to_proposed_event` at the tail of `handle`. The dedup key is
//! `<handler-name>-uid-<message-uid>`, so re-polling a mailbox cannot
//! re-propose mail already seen. An empty `Vec` means only that extraction
//! produced no drafts — this path is fully wired, not a stub.
//!
//! Drafts land in the `pending` review inbox and are never auto-committed. See
//! the HARD CONSTRAINT note in `handle` before changing that: the review step
//! is the sole control between a crafted email and a fabricated ledger entry.

use async_trait::async_trait;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

use crate::auto_import_scheduler::ImportError;
use crate::events::NewEvent;
use crate::extraction::{DocumentExtractor, ExtractionHint, receipt_extraction_to_drafts};

use super::imap::{ImapHandler, ImapMessage};
use super::mime::parse_eml;
use super::to_proposed_event;

pub struct ReceiptHandler {
    name: String,
    /// Lowercased patterns matched against the message's `from` header
    /// (substring match — `"@audible.ca"`, `"oxio.com"`, etc.).
    sender_patterns: Vec<String>,
    /// Excluded patterns — handlers earlier in the dispatch chain may
    /// claim these (e.g. a bank statement handler claims its own sender); this list lets
    /// a downstream "catch-all" receipt handler skip them defensively.
    excluded_patterns: Vec<String>,
    device_id: String,
    extractor: Arc<dyn DocumentExtractor>,
}

impl ReceiptHandler {
    pub fn new(
        name: impl Into<String>,
        sender_patterns: Vec<String>,
        device_id: impl Into<String>,
        extractor: Arc<dyn DocumentExtractor>,
    ) -> Self {
        Self {
            name: name.into(),
            sender_patterns: sender_patterns
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect(),
            excluded_patterns: Vec::new(),
            device_id: device_id.into(),
            extractor,
        }
    }

    pub fn with_excluded(mut self, excluded: Vec<String>) -> Self {
        self.excluded_patterns = excluded.into_iter().map(|s| s.to_lowercase()).collect();
        self
    }
}

/// Largest PDF attachment handed to poppler. Receipts and statements are well
/// under a megabyte; anything at this size is not a receipt.
const MAX_PDF_BYTES: usize = 25 * 1024 * 1024;

/// Cap on extracted text. Bounds the decompression ratio a crafted PDF can
/// achieve even when poppler itself exits cleanly.
const MAX_PDF_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// Wall-clock bound on one `pdftotext` run.
const PDFTOTEXT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Pdftotext over bytes, no encryption — used to pull text out of plain
/// (non-password-protected) PDF attachments. Returns empty string when
/// pdftotext can't extract (typically image-only PDFs); the caller decides
/// whether to fall back to image-mode extraction.
async fn pdftotext_bytes(pdf_bytes: &[u8]) -> Result<String, ImportError> {
    use std::io::Write;

    // Refuse oversized attachments before poppler ever sees them.
    //
    // This is the least-trusted input in the system: an arbitrary PDF, from an
    // unauthenticated email, reaching a large C++ parser as the server user
    // with no sandbox — and `accepts()` is spoofable, since `from` is taken
    // from the raw RFC822 header with no SPF or DKIM check. A size cap does not
    // make poppler safe, but it removes the cheapest attack (a compression bomb
    // that OOM-kills the box) and costs nothing: real receipts are tiny.
    if pdf_bytes.len() > MAX_PDF_BYTES {
        return Err(ImportError::Parse(format!(
            "pdf attachment is {} bytes, over the {MAX_PDF_BYTES}-byte limit",
            pdf_bytes.len(),
        )));
    }

    let mut temp =
        tempfile::NamedTempFile::new().map_err(|e| ImportError::Io(format!("temp file: {e}")))?;
    temp.write_all(pdf_bytes)
        .map_err(|e| ImportError::Io(format!("write temp: {e}")))?;
    temp.flush()
        .map_err(|e| ImportError::Io(format!("flush temp: {e}")))?;

    // `kill_on_drop` so the timeout below actually terminates poppler: a
    // malformed PDF that sends it into a loop would otherwise wedge this
    // source's scheduler task for the life of the process.
    let child = Command::new("pdftotext")
        .arg("-layout")
        .arg(temp.path())
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ImportError::Io(format!("pdftotext spawn: {e}")))?;

    let output = match tokio::time::timeout(PDFTOTEXT_TIMEOUT, child.wait_with_output()).await {
        Ok(result) => result.map_err(|e| ImportError::Io(format!("pdftotext wait: {e}")))?,
        Err(_) => {
            tracing::warn!(
                timeout_secs = PDFTOTEXT_TIMEOUT.as_secs(),
                "pdftotext exceeded its timeout and was killed",
            );
            // Non-fatal, same as an unextractable PDF — and now that a handler
            // error no longer wedges the mailbox, either outcome is survivable.
            return Ok(String::new());
        }
    };

    if !output.status.success() {
        // Non-fatal — image-only PDFs return error; let caller decide.
        return Ok(String::new());
    }

    // `.output()`/`wait_with_output` buffer stdout in full, so a PDF crafted to
    // expand into gigabytes of text is an OOM even when poppler itself behaves.
    let mut text = output.stdout;
    if text.len() > MAX_PDF_TEXT_BYTES {
        tracing::warn!(
            bytes = text.len(),
            "pdftotext output over the cap — truncating",
        );
        text.truncate(MAX_PDF_TEXT_BYTES);
    }
    Ok(String::from_utf8_lossy(&text).into_owned())
}

#[async_trait]
impl ImapHandler for ReceiptHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn accepts(&self, message: &ImapMessage) -> bool {
        let from_lower = message.from.to_lowercase();
        if self
            .excluded_patterns
            .iter()
            .any(|p| from_lower.contains(p))
        {
            return false;
        }
        self.sender_patterns.iter().any(|p| from_lower.contains(p))
    }

    async fn handle(&self, message: &ImapMessage) -> Result<Vec<NewEvent>, ImportError> {
        let parsed = parse_eml(&message.body)
            .map_err(|e| ImportError::Parse(format!("receipt mime: {e}")))?;

        // Start with the text body; append text from any non-encrypted PDF
        // attachments. Image-only PDFs (pdftotext returns empty) contribute
        // nothing here — recovering those needs image-mode extraction, which
        // this text-only path deliberately does not attempt.
        let mut combined_text = parsed.body_text.clone();
        for att in &parsed.attachments {
            if att
                .content_type
                .to_ascii_lowercase()
                .starts_with("application/pdf")
            {
                match pdftotext_bytes(&att.bytes).await {
                    Ok(t) if !t.is_empty() => {
                        combined_text.push_str("\n\n--- PDF: ");
                        combined_text.push_str(&att.filename);
                        combined_text.push_str(" ---\n\n");
                        combined_text.push_str(&t);
                    }
                    _ => {
                        tracing::debug!(
                            handler = self.name(),
                            attachment = %att.filename,
                            "skipping unextractable PDF attachment"
                        );
                    }
                }
            }
        }

        if combined_text.trim().is_empty() {
            return Err(ImportError::Parse(format!(
                "receipt: message {} from {} produced no extractable text",
                message.uid, message.from
            )));
        }

        // HARD CONSTRAINT — read before wiring any auto-commit path.
        //
        // `combined_text` is attacker-controlled twice over: the email body and
        // the text lifted out of its PDF attachments, concatenated and sent to
        // the extractor verbatim. Prompt injection can therefore make the LLM
        // return amounts and an `account_hint` of the sender's choosing, which
        // become drafts.
        //
        // That is acceptable ONLY because every draft lands in the `pending`
        // review inbox and requires an explicit user commit. The review step is
        // not a UX nicety here — it is the sole control standing between a
        // crafted email and a fabricated transaction in the ledger. The planned
        // LLM-primary interface will be tempted to auto-commit high-confidence
        // drafts; doing so on this path, without sender authentication (there
        // is no SPF/DKIM check — see `accepts`), hands write access to anyone
        // who knows the watched address.
        let result = self
            .extractor
            .extract(
                combined_text.as_bytes(),
                "text/plain",
                ExtractionHint::EmailBody,
            )
            .await
            .map_err(|e| ImportError::Upstream(format!("receipt extract: {e}")))?;

        tracing::info!(
            handler = self.name(),
            from = %message.from,
            subject = %parsed.subject,
            confidence = result.confidence,
            postings = result.postings.len(),
            "receipt: producing proposed batch"
        );

        let source_prefix = format!("{}-uid-{}", self.name, message.uid);
        let drafts = receipt_extraction_to_drafts(&result, &source_prefix);
        if drafts.is_empty() {
            return Ok(vec![]);
        }
        let dedup_key = format!("{}-uid-{}", self.name, message.uid);
        let source_metadata = serde_json::json!({
            "from": message.from,
            "subject": parsed.subject,
            "uid": message.uid,
        });
        let event = to_proposed_event(
            self.name(),
            dedup_key,
            drafts,
            Some(source_metadata),
            self.device_id.clone(),
        );
        Ok(vec![event])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

    /// `.reference/` is gitignored — skip rather than panic if fixtures aren't
    /// present (fresh-clone / CI safety).
    fn fixture_eml(name: &str) -> Option<Vec<u8>> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".reference/imap poller")
            .join(name);
        std::fs::read(&path).ok()
    }

    fn imap_msg_from(from: &str, body: Vec<u8>) -> ImapMessage {
        ImapMessage {
            uid: 1,
            from: from.into(),
            subject: "Test".into(),
            date: chrono::Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
            body,
        }
    }

    #[test]
    fn accepts_matching_sender() {
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new(
            "subs",
            vec!["@audible.ca".into(), "@oxio.com".into()],
            "device-test",
            extractor,
        );
        assert!(handler.accepts(&imap_msg_from("donotreply@audible.ca", Vec::new())));
        assert!(handler.accepts(&imap_msg_from("hello@oxio.com", Vec::new())));
        assert!(!handler.accepts(&imap_msg_from("random@example.com", Vec::new())));
    }

    #[test]
    fn rejects_excluded_senders_even_when_pattern_matches() {
        // A catch-all `.com` handler that excludes `@meridian.example` (since SC has
        // a dedicated handler upstream).
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler =
            ReceiptHandler::new("catchall", vec![".com".into()], "device-test", extractor)
                .with_excluded(vec!["@meridian.example".into()]);
        assert!(handler.accepts(&imap_msg_from("any@anywhere.com", Vec::new())));
        assert!(!handler.accepts(&imap_msg_from("notifications@meridian.example", Vec::new())));
    }

    #[tokio::test]
    async fn handles_audible_inline_body_eml() {
        let body = match fixture_eml("Thanks, your order is complete_audible.eml") {
            Some(b) => b,
            None => {
                eprintln!("fixture missing — skipping");
                return;
            }
        };
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new(
            "audible",
            vec!["@audible.ca".into()],
            "device-test",
            extractor,
        );
        let msg = imap_msg_from("donotreply@audible.ca", body);
        let events = handler
            .handle(&msg)
            .await
            .expect("audible handler should succeed");
        // NullExtractor → empty events; the point is the pipeline doesn't error.
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn handles_oxio_inline_body_eml() {
        let body = match fixture_eml("📫 oxio invoice available..eml") {
            Some(b) => b,
            None => {
                eprintln!("fixture missing — skipping");
                return;
            }
        };
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new("oxio", vec!["oxio".into()], "device-test", extractor);
        let msg = imap_msg_from("billing@oxio.com", body);
        let events = handler
            .handle(&msg)
            .await
            .expect("oxio handler should succeed");
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn handles_message_with_no_text_returns_parse_error() {
        // An empty-body / no-text message should surface as a Parse error,
        // not be silently accepted (we'd waste an LLM call on nothing).
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new("any", vec![".com".into()], "device-test", extractor);
        // Minimal MIME message with only headers — no real body.
        let body = b"From: x@example.com\r\nSubject: empty\r\nDate: Sat, 16 May 2026 12:00:00 +0000\r\n\r\n".to_vec();
        let msg = imap_msg_from("x@example.com", body);
        let err = handler.handle(&msg).await.unwrap_err();
        match err {
            ImportError::Parse(m) => assert!(m.contains("no extractable text")),
            other => panic!("expected Parse error, got {other:?}"),
        }
    }
}
