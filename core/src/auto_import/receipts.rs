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
//! Event-emission deferred (same shape as `ScNgnHandler` — wires through
//! Gemini, returns empty Vec for now to avoid duplicate-event ghosts during
//! the bring-up period).

use async_trait::async_trait;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

use crate::auto_import_scheduler::ImportError;
use crate::events::NewEvent;
use crate::extraction::{
    receipt_extraction_to_events, DocumentExtractor, ExtractionHint,
};

use super::imap::{ImapHandler, ImapMessage};
use super::mime::parse_eml;

pub struct ReceiptHandler {
    name: String,
    /// Lowercased patterns matched against the message's `from` header
    /// (substring match — `"@audible.ca"`, `"oxio.com"`, etc.).
    sender_patterns: Vec<String>,
    /// Excluded patterns — handlers earlier in the dispatch chain may
    /// claim these (e.g. ScNgnHandler claims `@sc.com`); this list lets
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

/// Pdftotext over bytes, no encryption — used to pull text out of plain
/// (non-password-protected) PDF attachments. Returns empty string when
/// pdftotext can't extract (typically image-only PDFs); the caller decides
/// whether to fall back to image-mode extraction.
async fn pdftotext_bytes(pdf_bytes: &[u8]) -> Result<String, ImportError> {
    use std::io::Write;
    let mut temp = tempfile::NamedTempFile::new()
        .map_err(|e| ImportError::Io(format!("temp file: {e}")))?;
    temp.write_all(pdf_bytes)
        .map_err(|e| ImportError::Io(format!("write temp: {e}")))?;
    temp.flush()
        .map_err(|e| ImportError::Io(format!("flush temp: {e}")))?;

    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg(temp.path())
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ImportError::Io(format!("pdftotext spawn: {e}")))?;

    if !output.status.success() {
        // Non-fatal — image-only PDFs return error; let caller decide.
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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
        self.sender_patterns
            .iter()
            .any(|p| from_lower.contains(p))
    }

    async fn handle(&self, message: &ImapMessage) -> Result<Vec<NewEvent>, ImportError> {
        let parsed = parse_eml(&message.body)
            .map_err(|e| ImportError::Parse(format!("receipt mime: {e}")))?;

        // Start with the text body; append text from any non-encrypted PDF
        // attachments. Image-only PDFs (pdftotext returns empty) contribute
        // nothing here — they'd need image-mode extraction (Phase 4+).
        let mut combined_text = parsed.body_text.clone();
        for att in &parsed.attachments {
            if att.content_type.to_ascii_lowercase().starts_with("application/pdf") {
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
            "receipt: emitting events"
        );

        let source_prefix = format!("{}-uid-{}", self.name, message.uid);
        let events = receipt_extraction_to_events(&result, &source_prefix, &self.device_id);
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn fixture_eml(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".reference/imap poller")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|e| panic!("read fixture {name}: {e}"))
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
        // A catch-all `.com` handler that excludes `@sc.com` (since SC has
        // a dedicated handler upstream).
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new("catchall", vec![".com".into()], "device-test", extractor)
            .with_excluded(vec!["@sc.com".into()]);
        assert!(handler.accepts(&imap_msg_from("any@anywhere.com", Vec::new())));
        assert!(!handler.accepts(&imap_msg_from("notifications@sc.com", Vec::new())));
    }

    #[tokio::test]
    async fn handles_audible_inline_body_eml() {
        let body = fixture_eml("Thanks, your order is complete_audible.eml");
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler =
            ReceiptHandler::new("audible", vec!["@audible.ca".into()], "device-test", extractor);
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
        let body = fixture_eml("📫 oxio invoice available..eml");
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new("oxio", vec!["oxio".into()], "device-test", extractor);
        let msg = imap_msg_from("billing@oxio.com", body);
        let events = handler.handle(&msg).await.expect("oxio handler should succeed");
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
