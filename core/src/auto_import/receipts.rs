//! Generic receipt-email handler.
//!
//! Claims every message no handler ahead of it took, pulls the body text (and
//! concatenates any text-extractable PDF attachments via pdftotext), and
//! forwards the combined text to a `DocumentExtractor` with
//! `ExtractionHint::EmailBody`.
//!
//! There is no sender filter, deliberately. Measured on 54 real messages, a
//! hand-maintained sender list scored 82% recall to the classifier's 100%, and
//! filtering nothing costs about ten cents a month: `docs/src/auto-import.md`.
//!
//! It is therefore a catch-all and belongs last in the dispatch list
//! (`imap::poll_once` routes to the first handler that claims a message).
//!
//! Emits one `AutoImportBatchProposed` per message that carries a charge, built
//! by `to_proposed_event` at the tail of `handle`. The dedup key is
//! `<handler-name>-uid-<message-uid>`, so re-polling a mailbox cannot
//! re-propose mail already seen. A charge-bearing message proposes even when it
//! yielded no drafts; an empty `Vec` means the message carried no charge.
//!
//! A charge-bearing message carrying the vendor's own order number keys on the
//! order instead, so the several mails about one purchase become one review
//! item. Why the reference and not a header: `docs/src/auto-import.md`.
//!
//! Drafts land in the `pending` review inbox and are never auto-committed. See
//! the HARD CONSTRAINT note in `handle` before changing that: the review step
//! is the sole control between a crafted email and a fabricated ledger entry.

use async_trait::async_trait;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

use crate::auto_import_scheduler::ImportError;
use crate::events::{GROUP_MEMBER_KEY, NewEvent, ORDER_GROUP_KEY};
use crate::extraction::{
    DEFAULT_CONFIDENCE_THRESHOLD, DocumentExtractor, DocumentKind, DocumentPart, ExtractionHint,
    RECONCILE_ATTEMPTS, extract_reconciled, receipt_extraction_to_drafts,
};

use super::imap::{ImapHandler, ImapMessage};
use super::mime::{MimeAttachment, parse_eml};
use super::to_proposed_event;

/// Bounds on a vendor reference worth grouping on. The floor rejects the `.`
/// and `-` the model returns when a message has no reference; the ceiling
/// rejects a subject line pasted into the field.
const MIN_ORDER_REF_LEN: usize = 4;
const MAX_ORDER_REF_LEN: usize = 64;

/// A vendor reference reduced to a group key, or `None` if it cannot carry one.
///
/// Junk here is the expensive direction: two distinct purchases sharing a bad
/// key merge into one review item and a transaction goes missing, where no key
/// at all only costs a dismissal.
fn group_key(order_ref: Option<&str>) -> Option<String> {
    let raw = order_ref?.trim();
    if raw.len() < MIN_ORDER_REF_LEN || raw.len() > MAX_ORDER_REF_LEN {
        return None;
    }
    // An order number has no spaces in it; a subject line does.
    if raw.chars().any(char::is_whitespace) {
        return None;
    }
    let key: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    // Punctuation is dropped before this, so `#118-762-884` and `118762884`
    // group together. Prose with no digit in it is not a reference.
    if key.len() < MIN_ORDER_REF_LEN || !key.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(key)
}

pub struct ReceiptHandler {
    name: String,
    device_id: String,
    extractor: Arc<dyn DocumentExtractor>,
}

impl ReceiptHandler {
    pub fn new(
        name: impl Into<String>,
        device_id: impl Into<String>,
        extractor: Arc<dyn DocumentExtractor>,
    ) -> Self {
        Self {
            name: name.into(),
            device_id: device_id.into(),
            extractor,
        }
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

/// Whether an attachment is worth handing to pdftotext.
///
/// Deliberately permissive, matching `archive::is_pdf` and the "no MIME gate"
/// note on `document_fields::parser_fields`: senders mislabel PDFs often enough
/// that trusting the declared type loses real documents. A rental invoice
/// arrived as `application/octet-stream` with a `.pdf` name, and its 487 KB of
/// line items — the only place the amount appeared — was never opened.
///
/// The extension arm adds no exposure: `content_type` is attacker-controlled
/// too, so a crafted file could always reach poppler. What bounds that is
/// [`MAX_PDF_BYTES`] and [`PDFTOTEXT_TIMEOUT`], not this test.
fn looks_like_pdf(att: &MimeAttachment) -> bool {
    let declared = att.content_type.to_ascii_lowercase();
    if declared.starts_with("application/pdf") || declared.starts_with("application/x-pdf") {
        return true;
    }
    if infer::get(&att.bytes).is_some_and(|k| k.mime_type() == "application/pdf") {
        return true;
    }
    att.filename.to_ascii_lowercase().ends_with(".pdf")
}

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

    /// Claims everything. Whether a message is a purchase is decided by
    /// `DocumentKind` in `handle`, which measured better than a sender list on
    /// both recall and precision.
    fn accepts(&self, _message: &ImapMessage) -> bool {
        true
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
            if looks_like_pdf(att) {
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
        // Nothing cross-checked email-sourced drafts before this: `verify` ran only on
        // the manual upload route, so the unattended path had the weaker guarantee.
        //
        // Through `extract_reconciled` rather than `extract` + `verify` because this is
        // the unattended path: nobody is watching to re-run a mail that came back
        // unpriced, and one measured document returned no line items on half its runs
        // against a total it stated correctly every time.
        let (result, report) = extract_reconciled(
            self.extractor.as_ref(),
            &[DocumentPart::new(combined_text.as_bytes(), "text/plain")],
            ExtractionHint::EmailBody,
            DEFAULT_CONFIDENCE_THRESHOLD,
            RECONCILE_ATTEMPTS,
        )
        .await
        .map_err(|e| ImportError::Upstream(format!("receipt extract: {e}")))?;

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

        // A vendor sends many messages per order and only some of them record a
        // charge. Only a label that positively says "no money" drops a message
        // outright — see `DocumentKind::rules_out_a_charge`.
        if let Some(kind) = result.kind()
            && kind.rules_out_a_charge()
        {
            tracing::info!(
                handler = self.name(),
                uid = message.uid,
                subject = %parsed.subject,
                kind = ?kind,
                "receipt: not a charge, proposing nothing"
            );
            return Ok(vec![]);
        }
        // Backstop for mail whose kind is unknown — no label at all, or a word
        // this build does not recognise. With nothing saying this is a charge
        // and no money in it, there is nothing to review.
        //
        // Deliberately does NOT apply when the model called it a charge. A
        // receipt whose amount failed to extract looks identical to a survey
        // here, and dropping it would lose a real purchase silently, where
        // letting it through costs one dismissal in a queue that exists anyway.
        let kind_is_unknown = result
            .kind()
            .is_none_or(|kind| matches!(kind, DocumentKind::Other));
        if kind_is_unknown && !result.has_nonzero_amount() {
            tracing::info!(
                handler = self.name(),
                uid = message.uid,
                subject = %parsed.subject,
                kind = ?result.kind(),
                "receipt: no non-zero amount, proposing nothing"
            );
            return Ok(vec![]);
        }

        let source_prefix = format!("{}-uid-{}", self.name, message.uid);
        let drafts = receipt_extraction_to_drafts(&result, &source_prefix);
        // A message the model called a charge still reaches review when it
        // produced no drafts at all. Silence here is indistinguishable from "no
        // purchase happened", and the batch still carries the source email and
        // the warnings, which is what a person needs to price it by hand.
        let charge_bearing = result.kind().is_some_and(|kind| kind.records_a_charge());
        if drafts.is_empty() && !charge_bearing {
            return Ok(vec![]);
        }
        // Only a charge-bearing kind may group. `order_ref` on other mail is
        // junk the model filled in from a subject line, and grouping two
        // purchases on a shared junk key loses one of them silently.
        let order_group = result
            .kind()
            .filter(|kind| kind.records_a_charge())
            .and_then(|_| group_key(result.order_ref.as_deref()));
        let dedup_key = match &order_group {
            Some(key) => format!("{}-order-{key}", self.name),
            None => format!("{}-uid-{}", self.name, message.uid),
        };
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
            // Without this an empty `warnings` reads as "the arithmetic was
            // checked", which on a single-amount email is false.
            "total_check": report.total_check,
            "document_kind": result.document_kind,
            "order_ref": result.order_ref,
            // What the batch grouped on, and this message's identity within it.
            // The projection needs the member to tell a re-fetched message from
            // a new one about the same order.
            ORDER_GROUP_KEY: order_group,
            GROUP_MEMBER_KEY: format!("uid-{}", message.uid),
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

    fn attachment(filename: &str, content_type: &str, bytes: &[u8]) -> MimeAttachment {
        MimeAttachment {
            filename: filename.into(),
            content_type: content_type.into(),
            bytes: bytes.to_vec(),
            is_inline: false,
        }
    }

    /// The minimum poppler will look at — `infer` keys on this signature.
    const PDF_MAGIC: &[u8] = b"%PDF-1.7\n";

    #[test]
    fn a_pdf_declared_as_a_pdf_is_read() {
        assert!(looks_like_pdf(&attachment(
            "invoice.pdf",
            "application/pdf",
            PDF_MAGIC
        )));
        assert!(looks_like_pdf(&attachment(
            "invoice.pdf",
            "APPLICATION/PDF; name=invoice.pdf",
            PDF_MAGIC
        )));
        assert!(looks_like_pdf(&attachment(
            "invoice.pdf",
            "application/x-pdf",
            PDF_MAGIC
        )));
    }

    /// The case that cost a real rental invoice: the sender declared
    /// `application/octet-stream`, so the old content-type test skipped an
    /// attachment carrying the only copy of the amount.
    #[test]
    fn a_pdf_misdeclared_as_octet_stream_is_still_read() {
        assert!(looks_like_pdf(&attachment(
            "invoice.pdf",
            "application/octet-stream",
            PDF_MAGIC
        )));
    }

    /// Either signal alone is enough — the bytes when the name is unhelpful,
    /// the name when the bytes are not recognisable.
    #[test]
    fn either_the_bytes_or_the_name_is_enough() {
        assert!(looks_like_pdf(&attachment(
            "attachment",
            "application/octet-stream",
            PDF_MAGIC
        )));
        assert!(looks_like_pdf(&attachment(
            "statement.PDF",
            "application/octet-stream",
            b"not a recognisable header"
        )));
    }

    /// Everything else still stays away from poppler: the branding images a
    /// real statement email carries are the common case here.
    #[test]
    fn a_non_pdf_attachment_is_left_alone() {
        assert!(!looks_like_pdf(&attachment(
            "logo.png",
            "image/png",
            b"\x89PNG\r\n\x1a\n"
        )));
        assert!(!looks_like_pdf(&attachment(
            "rows.csv",
            "text/csv",
            b"date,amount\n"
        )));
        assert!(!looks_like_pdf(&attachment(
            "notes",
            "application/octet-stream",
            b"plain bytes"
        )));
    }

    /// The gate is gone: every message is claimed, and `DocumentKind` inside
    /// `handle` decides whether it was a purchase. Measured on 54 real emails,
    /// a sender list scored 82% recall against the classifier's 100%.
    #[test]
    fn every_message_is_claimed() {
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new("receipts", "device-test", extractor);
        for from in [
            "donotreply@audible.ca",
            "random@example.com",
            "noreply@a-vendor-nobody-listed.example",
            "",
        ] {
            assert!(
                handler.accepts(&imap_msg_from(from, Vec::new())),
                "{from:?} must reach the classifier"
            );
        }
    }

    /// A forwarded receipt needs no special case now. The 16 KB quoted-header
    /// scan it used to need flaked on a re-forward, claiming a message on one
    /// forward and dropping the identical bytes on the next.
    #[test]
    fn a_forwarded_receipt_is_just_mail() {
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new("receipts", "device-test", extractor);
        let body = b"From: Name Me <me@gmail.com>\r\n\r\n\
                     ---------- Forwarded message ---------\r\n\
                     From: A Vendor <noreply@vendor.example>\r\n\
                     Subject: your order\r\n\r\nOrder total $42.18\r\n";
        assert!(handler.accepts(&imap_msg_from("me@gmail.com", body.to_vec())));
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
        let handler = ReceiptHandler::new("audible", "device-test", extractor);
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
        let handler = ReceiptHandler::new("oxio", "device-test", extractor);
        let msg = imap_msg_from("billing@oxio.com", body);
        let events = handler
            .handle(&msg)
            .await
            .expect("oxio handler should succeed");
        assert!(events.is_empty());
    }

    /// Returns a canned extraction, so the handler's own gate can be exercised
    /// without a model in the loop.
    struct StubExtractor(crate::extraction::ExtractionResult);

    #[async_trait]
    impl crate::extraction::DocumentExtractor for StubExtractor {
        fn name(&self) -> &str {
            "stub"
        }
        fn supports(&self, _mime: &str) -> bool {
            true
        }
        async fn extract(
            &self,
            _parts: &[crate::extraction::DocumentPart<'_>],
            _hint: crate::extraction::ExtractionHint,
        ) -> Result<crate::extraction::ExtractionResult, crate::extraction::ExtractionError>
        {
            Ok(self.0.clone())
        }
    }

    fn stub_result(kind: Option<&str>, amount: &str) -> crate::extraction::ExtractionResult {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        crate::extraction::ExtractionResult {
            date: None,
            date_as_printed: None,
            description: Some("Northwind".into()),
            postings: vec![crate::extraction::ExtractedPosting {
                account_hint: Some("Expenses:Groceries".into()),
                commodity: "CAD".into(),
                amount: Decimal::from_str(amount).unwrap(),
                line_label: None,
            }],
            total: None,
            confidence: 0.9,
            dropped_postings: 0,
            model: "stub".into(),
            total_discarded: false,
            document_kind: kind.map(String::from),
            order_ref: Some("ORD-1".into()),
            raw_response: serde_json::Value::Null,
        }
    }

    fn plain_eml() -> Vec<u8> {
        b"From: shop@northwind.example\r\nSubject: about your order\r\nDate: Sat, 16 May 2026 12:00:00 +0000\r\nContent-Type: text/plain\r\n\r\nSomething about your order.\r\n".to_vec()
    }

    async fn events_for(kind: Option<&str>, amount: &str) -> usize {
        let extractor = Arc::new(StubExtractor(stub_result(kind, amount)));
        let handler = ReceiptHandler::new("shop", "device-test", extractor);
        let msg = imap_msg_from("shop@northwind.example", plain_eml());
        handler.handle(&msg).await.expect("handler ok").len()
    }

    /// One Walmart order produced six batches because every message from a
    /// listed sender became a transaction, a satisfaction survey included.
    #[tokio::test]
    async fn a_feedback_request_proposes_nothing() {
        assert_eq!(events_for(Some("feedback_request"), "0.00").await, 0);
        // Even when the model attaches a real-looking amount to it.
        assert_eq!(events_for(Some("feedback_request"), "105.43").await, 0);
    }

    #[tokio::test]
    async fn marketing_proposes_nothing() {
        assert_eq!(events_for(Some("marketing"), "0.00").await, 0);
    }

    /// The backstop is for UNLABELLED mail only.
    #[tokio::test]
    async fn an_unlabelled_zero_amount_message_proposes_nothing() {
        assert_eq!(events_for(None, "0.00").await, 0);
    }

    /// ⚠️ The asymmetry that decides this: a receipt whose amount failed to
    /// extract is indistinguishable from a survey at this point, so suppressing
    /// it would lose a real purchase with nothing on screen to say so. One junk
    /// draft in a review queue is the cheaper mistake.
    #[tokio::test]
    async fn a_charge_the_model_could_not_price_still_reaches_review() {
        assert_eq!(events_for(Some("receipt"), "0.00").await, 1);
    }

    /// ⚠️ Built from the artifact, not the stub. A real Instacart order
    /// confirmation (2026-09-24) extracted **zero postings**, not a zero
    /// *amount* — and the zero-amount fixture above made that look covered
    /// while the message was silently dropped.
    #[tokio::test]
    async fn a_charge_with_no_postings_at_all_still_reaches_review() {
        let mut result = stub_result(Some("order_confirmation"), "0.00");
        result.postings.clear();
        let extractor = Arc::new(StubExtractor(result));
        let handler = ReceiptHandler::new("shop", "device-test", extractor);
        let msg = imap_msg_from("shop@northwind.example", plain_eml());
        let events = handler.handle(&msg).await.expect("handler ok");
        assert_eq!(
            events.len(),
            1,
            "an unpriced confirmation must be reviewable"
        );
        assert_eq!(
            events[0].payload["draft_postings"].as_array().map(Vec::len),
            Some(0),
            "it carries no drafts — the source email and warnings are the point"
        );
    }

    /// The other half: with no charge-bearing label, no drafts still means no
    /// batch. Otherwise every unparseable marketing mail becomes a review item.
    #[tokio::test]
    async fn an_unlabelled_message_with_no_postings_proposes_nothing() {
        let mut result = stub_result(None, "105.43");
        result.postings.clear();
        let extractor = Arc::new(StubExtractor(result));
        let handler = ReceiptHandler::new("shop", "device-test", extractor);
        let msg = imap_msg_from("shop@northwind.example", plain_eml());
        assert!(handler.handle(&msg).await.expect("handler ok").is_empty());
    }

    #[tokio::test]
    async fn a_real_charge_still_proposes_a_batch() {
        assert_eq!(events_for(Some("receipt"), "105.43").await, 1);
        assert_eq!(events_for(Some("shipping_notice"), "99.93").await, 1);
    }

    /// Fail open. A model that omits the field is not evidence the message is
    /// uninteresting, and dropping a real purchase silently is the worse bug.
    #[tokio::test]
    async fn an_unlabelled_message_is_still_proposed() {
        assert_eq!(events_for(None, "105.43").await, 1);
    }

    // ----- What a batch is keyed on -----

    /// The proposal a message produced, or `None` if it produced nothing.
    async fn proposal_for(
        kind: Option<&str>,
        order_ref: Option<&str>,
        uid: u32,
    ) -> Option<NewEvent> {
        let mut result = stub_result(kind, "105.43");
        result.order_ref = order_ref.map(String::from);
        let extractor = Arc::new(StubExtractor(result));
        let handler = ReceiptHandler::new("shop", "device-test", extractor);
        let mut msg = imap_msg_from("shop@northwind.example", plain_eml());
        msg.uid = uid;
        handler
            .handle(&msg)
            .await
            .expect("handler ok")
            .into_iter()
            .next()
    }

    async fn dedup_key_for(kind: Option<&str>, order_ref: Option<&str>) -> String {
        let event = proposal_for(kind, order_ref, 7).await.expect("a proposal");
        event.payload["dedup_key"]
            .as_str()
            .expect("dedup_key is a string")
            .to_string()
    }

    #[tokio::test]
    async fn a_charge_with_an_order_number_keys_on_the_order() {
        assert_eq!(
            dedup_key_for(Some("receipt"), Some("118762884")).await,
            "shop-order-118762884"
        );
    }

    /// Punctuation and case are dropped, so the same reference printed two ways
    /// in two mails still groups them.
    #[tokio::test]
    async fn an_order_number_groups_however_the_vendor_punctuates_it() {
        let a = dedup_key_for(Some("receipt"), Some("#118-762-884")).await;
        let b = dedup_key_for(Some("order_confirmation"), Some("118762884")).await;
        assert_eq!(a, b);
    }

    /// Two distinct purchases merging on a junk key loses one of them with
    /// nothing on screen to say so, which is why each of these falls back to the
    /// message.
    #[tokio::test]
    async fn junk_in_the_order_field_groups_nothing() {
        for junk in [
            ".",
            "-",
            "ord",
            "Your order was delivered",
            "no digits here",
            "thankyouforshopping",
        ] {
            assert_eq!(
                dedup_key_for(Some("receipt"), Some(junk)).await,
                "shop-uid-7",
                "{junk:?} must not become a group key"
            );
        }
        assert_eq!(dedup_key_for(Some("receipt"), None).await, "shop-uid-7");
    }

    /// `order_ref` on mail that carries no charge is whatever the model found in
    /// the subject line, so the kind gate comes first.
    #[tokio::test]
    async fn an_unlabelled_message_never_groups() {
        assert_eq!(
            dedup_key_for(None, Some("118762884")).await,
            "shop-uid-7",
            "fail-open mail proposes, but it does not group"
        );
    }

    /// The projection needs the member to tell a re-fetched message from a new
    /// one about the same order; without it grouping cannot be safe.
    #[tokio::test]
    async fn every_proposal_names_the_message_it_came_from() {
        let event = proposal_for(Some("receipt"), Some("118762884"), 4242)
            .await
            .expect("a proposal");
        let meta = &event.payload["source_metadata"];
        assert_eq!(meta[GROUP_MEMBER_KEY].as_str(), Some("uid-4242"));
        assert_eq!(meta[ORDER_GROUP_KEY].as_str(), Some("118762884"));
    }

    /// An unrecognised label maps to `Other`, which means unknown, so the money
    /// decides. A real rental invoice was lost here: the model answered a
    /// reasonable word it was never told to use and the answer was thrown away.
    #[tokio::test]
    async fn an_unrecognised_label_with_money_still_reaches_review() {
        assert_eq!(events_for(Some("price_drop_alert"), "105.43").await, 1);
    }

    /// The other half of it. An unrecognised label with no money in it is the
    /// same as no label with no money: nothing to review.
    #[tokio::test]
    async fn an_unrecognised_label_with_no_money_proposes_nothing() {
        assert_eq!(events_for(Some("price_drop_alert"), "0.00").await, 0);
    }

    /// The asymmetry this fixes: a recognised non-charge is still a decision,
    /// and it still drops the message whatever amount rides along with it.
    #[tokio::test]
    async fn a_recognised_non_charge_still_drops_with_money_attached() {
        assert_eq!(events_for(Some("marketing"), "105.43").await, 0);
    }

    #[tokio::test]
    async fn handles_message_with_no_text_returns_parse_error() {
        // An empty-body / no-text message should surface as a Parse error,
        // not be silently accepted (we'd waste an LLM call on nothing).
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ReceiptHandler::new("any", "device-test", extractor);
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
