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
use crate::extraction::{
    DEFAULT_CONFIDENCE_THRESHOLD, DocumentExtractor, DocumentPart, ExtractionHint,
    receipt_extraction_to_drafts, verify,
};

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
    /// The user's own mailbox addresses, lowercased. A message from one of
    /// these is claimed only when it wraps a forwarded message whose original
    /// sender matches `sender_patterns` — see `accepts`.
    self_addresses: Vec<String>,
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
            self_addresses: Vec::new(),
            device_id: device_id.into(),
            extractor,
        }
    }

    pub fn with_excluded(mut self, excluded: Vec<String>) -> Self {
        self.excluded_patterns = excluded.into_iter().map(|s| s.to_lowercase()).collect();
        self
    }

    /// Enable forward-to-capture for these addresses — normally the `account`
    /// of every configured mailbox. Without them a forwarded receipt is dropped,
    /// because the forwarding client rewrites `From:` to the forwarder.
    pub fn with_self_addresses(mut self, addresses: Vec<String>) -> Self {
        self.self_addresses = addresses.into_iter().map(|s| s.to_lowercase()).collect();
        self
    }
}

/// How far into a message to look for the forwarded original's `From:`.
///
/// The wrapper's own headers plus the forward preamble sit well inside this;
/// scanning a whole multi-megabyte message to reject it is the cost being
/// avoided, and a `From:` deeper than this is quoted history, not the subject.
const FORWARD_SCAN_BYTES: usize = 16 * 1024;

/// How many lines after an embedded `From:` may carry its sibling headers.
const HEADER_BLOCK_WINDOW: usize = 3;

/// The original sender of a forwarded message, if this looks like one.
///
/// ⚠️ Do not gate this on a "Forwarded message" separator. A real Gmail forward
/// captured 2026-09-23 had none — its body part opened directly on the quoted
/// `From:`/`Date:`/`Subject:`/`To:` block — and it carried no `X-Forwarded-For`
/// or `Resent-From` either. The quoted header block is the only thing actually
/// present across forward styles, so it is what this keys on.
///
/// The block is also what keeps a plain note to self out: prose mentioning a
/// vendor has one `From:` (the wrapper's own) and no second header block.
fn forwarded_original_sender(body: &[u8]) -> Option<String> {
    let head = &body[..body.len().min(FORWARD_SCAN_BYTES)];
    let text = String::from_utf8_lossy(head).to_lowercase();
    let lines: Vec<&str> = text.lines().map(|l| l.trim_start()).collect();

    let mut seen_wrapper_from = false;
    for (i, line) in lines.iter().enumerate() {
        let Some(value) = line.strip_prefix("from:") else {
            continue;
        };
        // The first is the forwarder's own, rewritten by their mail client.
        if !seen_wrapper_from {
            seen_wrapper_from = true;
            continue;
        }
        let has_sibling = lines[i + 1..]
            .iter()
            .take(HEADER_BLOCK_WINDOW)
            .any(|l| l.starts_with("subject:") || l.starts_with("date:"));
        if has_sibling {
            return Some(value.trim().to_string());
        }
    }
    None
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
        if self.sender_patterns.iter().any(|p| from_lower.contains(p)) {
            return true;
        }
        // Forward-to-capture. Gated on the sender being the user's own address
        // AND the message wrapping another one AND that original matching a
        // vendor, so a plain note to self is still ignored. The .edu mailbox
        // reaches omni-me this way, since its SSO blocks IMAP.
        if !self.self_addresses.iter().any(|a| from_lower.contains(a)) {
            return false;
        }
        match forwarded_original_sender(&message.body) {
            Some(original) => {
                !self.excluded_patterns.iter().any(|p| original.contains(p))
                    && self.sender_patterns.iter().any(|p| original.contains(p))
            }
            None => false,
        }
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
                &[DocumentPart::new(combined_text.as_bytes(), "text/plain")],
                ExtractionHint::EmailBody,
            )
            .await
            .map_err(|e| ImportError::Upstream(format!("receipt extract: {e}")))?;

        // Nothing cross-checked email-sourced drafts before this: `verify` ran only on
        // the manual upload route, so the unattended path had the weaker guarantee.
        let report = verify(
            &result,
            ExtractionHint::EmailBody,
            DEFAULT_CONFIDENCE_THRESHOLD,
        );

        tracing::info!(
            handler = self.name(),
            from = %message.from,
            subject = %parsed.subject,
            confidence = result.confidence,
            effective_confidence = report.effective_confidence,
            needs_manual_review = report.needs_manual_review,
            dropped_postings = result.dropped_postings,
            warnings = ?report.warnings,
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
            // Carried so review can tell a salvaged draft from a clean one. Without
            // it the only record of a discarded line item is a log line.
            "effective_confidence": report.effective_confidence,
            "needs_manual_review": report.needs_manual_review,
            "warnings": report.warnings,
            "dropped_postings": result.dropped_postings,
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

    /// Shaped after a real Gmail forward captured 2026-09-23: the top-level
    /// `From:` is the forwarder, the original survives inside the wrapper, and
    /// there is no `X-Forwarded-For` or `Resent-From` to key off instead.
    /// Byte-for-byte the shape of a real Gmail forward captured 2026-09-23:
    /// wrapper headers, a MIME boundary, then the quoted original header block
    /// with **no** "Forwarded message" separator anywhere. `original_from` is
    /// the whole header value, display name included, because the vendor match
    /// runs over the entire line exactly as it does for a direct `from`.
    fn gmail_forward(original_from: &str) -> Vec<u8> {
        format!(
            "Delivered-To: me@gmail.com\r\n\
             References: <abc@walmart.ca>\r\n\
             From: Name Me <me@gmail.com>\r\n\
             Date: Wed, 23 Sep 2026 00:03:15 -0400\r\n\
             Subject: Thank you for shopping with us!\r\n\
             To: other@gmail.com\r\n\
             Content-Type: multipart/alternative; boundary=\"000000000000c52\"\r\n\r\n\
             --000000000000c52\r\n\
             Content-Type: text/plain; charset=\"UTF-8\"\r\n\
             Content-Transfer-Encoding: quoted-printable\r\n\r\n\
             From: {original_from}\r\n\
             Date: Fri, Sep 18, 2026, 10:45=E2=80=AFp.m.\r\n\
             Subject: Thank you for shopping with us!\r\n\
             To: <me@gmail.com>\r\n\r\n\
             Order total $42.18\r\n"
        )
        .into_bytes()
    }

    /// The older desktop-Gmail shape, which DOES carry the separator. Kept so
    /// the separator style cannot regress while the newer one is the fixture.
    fn gmail_forward_with_separator(original_from: &str) -> Vec<u8> {
        format!(
            "From: Name Me <me@gmail.com>\r\n\
             Subject: Fwd: receipt\r\n\r\n\
             ---------- Forwarded message ---------\r\n\
             From: {original_from}\r\n\
             Date: Mon, 22 Sep 2026 19:02:11 -0400\r\n\
             Subject: Thank you for shopping with us!\r\n\r\n\
             Order total $42.18\r\n"
        )
        .into_bytes()
    }

    fn forward_handler() -> ReceiptHandler {
        ReceiptHandler::new(
            "receipts",
            vec!["walmart".into()],
            "device-test",
            Arc::new(crate::extraction::null::NullExtractor),
        )
        .with_self_addresses(vec!["me@gmail.com".into()])
    }

    #[test]
    fn a_self_forward_is_claimed_by_the_original_sender() {
        let h = forward_handler();
        assert!(
            h.accepts(&imap_msg_from(
                "me@gmail.com",
                gmail_forward("Walmart Canada <noreply@walmart.ca>")
            )),
            "the real Gmail forward carries no separator — the quoted header block is the signal"
        );
    }

    #[test]
    fn the_older_separator_style_still_works() {
        let h = forward_handler();
        assert!(h.accepts(&imap_msg_from(
            "me@gmail.com",
            gmail_forward_with_separator("Walmart Canada <noreply@walmart.ca>")
        )));
    }

    /// A bare `From:` with no sibling headers is a mention, not a forward.
    #[test]
    fn a_quoted_address_without_a_header_block_is_not_a_forward() {
        let h = forward_handler();
        let body = b"From: Name Me <me@gmail.com>\r\n\r\n\
                     i got this from: noreply@walmart.ca\r\n\
                     should i keep it?\r\n";
        assert!(!h.accepts(&imap_msg_from("me@gmail.com", body.to_vec())));
    }

    #[test]
    fn a_plain_note_to_self_is_still_ignored() {
        let h = forward_handler();
        let body = b"From: Name Me <me@gmail.com>\r\n\r\nremember to buy milk at walmart\r\n";
        assert!(
            !h.accepts(&imap_msg_from("me@gmail.com", body.to_vec())),
            "a note merely mentioning a vendor must not be claimed"
        );
    }

    #[test]
    fn a_forward_of_unrelated_mail_is_ignored() {
        let h = forward_handler();
        assert!(!h.accepts(&imap_msg_from(
            "me@gmail.com",
            gmail_forward("A Friend <friend@example.com>")
        )));
    }

    #[test]
    fn forwarding_is_off_until_self_addresses_are_configured() {
        let h = ReceiptHandler::new(
            "receipts",
            vec!["walmart".into()],
            "device-test",
            Arc::new(crate::extraction::null::NullExtractor),
        );
        assert!(!h.accepts(&imap_msg_from(
            "me@gmail.com",
            gmail_forward("Walmart Canada <noreply@walmart.ca>")
        )));
    }

    #[test]
    fn an_exclusion_still_wins_inside_a_forward() {
        let h = forward_handler().with_excluded(vec!["@sc.com".into()]);
        let body = gmail_forward("SC Bank <statements@sc.com>");
        assert!(!h.accepts(&imap_msg_from("me@gmail.com", body)));
    }

    #[test]
    fn a_from_beyond_the_scan_window_is_not_read() {
        let h = forward_handler();
        // A header block that WOULD be claimed in range, pushed out of it — so
        // this fails if the window stops being applied, not merely if the block
        // is malformed.
        let mut body = b"From: Name Me <me@gmail.com>\r\n\r\n".to_vec();
        body.extend(std::iter::repeat_n(b'x', FORWARD_SCAN_BYTES));
        body.extend_from_slice(
            b"\r\nFrom: Walmart Canada <noreply@walmart.ca>\r\nSubject: receipt\r\n",
        );
        assert!(!h.accepts(&imap_msg_from("me@gmail.com", body.clone())));

        // Control: the same block inside the window IS claimed.
        let mut near = b"From: Name Me <me@gmail.com>\r\n\r\n".to_vec();
        near.extend_from_slice(
            b"From: Walmart Canada <noreply@walmart.ca>\r\nSubject: receipt\r\n",
        );
        assert!(h.accepts(&imap_msg_from("me@gmail.com", near)));
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
