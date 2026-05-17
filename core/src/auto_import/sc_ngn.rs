//! Standard Chartered (Nigeria) eStatement decryption + text extraction.
//!
//! SC Nigeria emails monthly account statements as password-protected PDFs.
//! Per the SC notification email (verified 2026-05-16 against a real
//! statement): the password is the **third to eighth digits** of the user's
//! NEW NUBAN account number (Nigerian standardized 10-digit account format).
//!
//! `derive_pdf_password(account)` returns those 6 characters as a string.
//! `decrypt_and_extract(...)` shells out to `pdftotext -upw <pw>` to both
//! decrypt and pull text in one step — `poppler-utils` is already installed
//! on this dev box; production hosts need it as a system dep.
//!
//! Wiring into the IMAP dispatch path (Phase 2.11) lives in a separate
//! `ScNgnHandler` impl — deferred until we add a MIME parser crate to pull
//! the attachment out of the email body.

use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

use crate::auto_import_scheduler::ImportError;
use crate::events::NewEvent;
use crate::extraction::{
    statement_extraction_to_events, DocumentExtractor, ExtractionHint,
};

use super::imap::{ImapHandler, ImapMessage};
use super::mime::parse_eml;

#[derive(Debug, thiserror::Error)]
pub enum ScError {
    #[error("NUBAN account number must be at least 8 digits (got {0})")]
    AccountTooShort(usize),
    #[error("NUBAN account number must be all digits (got non-digit at position {0})")]
    NonDigitAccount(usize),
    #[error("pdftotext spawn failed: {0}")]
    Spawn(String),
    #[error("pdftotext exited with status {status}: {stderr}")]
    DecryptFailed { status: String, stderr: String },
    #[error("pdftotext output was not utf-8: {0}")]
    NotUtf8(String),
}

/// Derive the 6-character PDF password from the NUBAN account number.
/// Returns characters at 0-indexed positions [2..8] (i.e. the 3rd through
/// 8th digit). Validates the input is all-digits and long enough.
pub fn derive_pdf_password(account_number: &str) -> Result<String, ScError> {
    if account_number.len() < 8 {
        return Err(ScError::AccountTooShort(account_number.len()));
    }
    if let Some(idx) = account_number.chars().position(|c| !c.is_ascii_digit()) {
        return Err(ScError::NonDigitAccount(idx));
    }
    Ok(account_number[2..8].to_string())
}

/// Decrypt PDF bytes + extract text. Writes to a temp file under the hood
/// because pdftotext seeks within the file to find the encryption dictionary;
/// piping via stdin doesn't reliably work for encrypted PDFs.
pub async fn decrypt_and_extract_bytes(
    pdf_bytes: &[u8],
    password: &str,
) -> Result<String, ScError> {
    let mut temp = tempfile::NamedTempFile::new()
        .map_err(|e| ScError::Spawn(format!("create temp: {e}")))?;
    use std::io::Write;
    temp.write_all(pdf_bytes)
        .map_err(|e| ScError::Spawn(format!("write temp: {e}")))?;
    temp.flush()
        .map_err(|e| ScError::Spawn(format!("flush temp: {e}")))?;
    decrypt_and_extract(temp.path(), password).await
}

/// Decrypt the PDF at `path` using `password` and extract its text via
/// `pdftotext -upw <pw> -layout - -`. `-layout` preserves table-like
/// columnar structure (useful for statement tables); `-` for output sends
/// to stdout.
pub async fn decrypt_and_extract(path: &Path, password: &str) -> Result<String, ScError> {
    let output = Command::new("pdftotext")
        .arg("-upw")
        .arg(password)
        .arg("-layout")
        .arg(path)
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ScError::Spawn(e.to_string()))?;

    if !output.status.success() {
        return Err(ScError::DecryptFailed {
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    String::from_utf8(output.stdout).map_err(|e| ScError::NotUtf8(e.to_string()))
}

/// ImapHandler for Standard Chartered eStatements. Accepts any mail from
/// `@sc.com` with `statement` in the subject. On match: parses the MIME
/// tree, finds the application/pdf attachment, decrypts via
/// `derive_pdf_password(account)`, and forwards the extracted text to a
/// `DocumentExtractor` with `ExtractionHint::BankStatement`.
///
/// Mapping the resulting `ExtractionResult` → typed events lives one layer
/// up (Phase 1.7 projection consumers + the W4 / Phase 5.5 statement-feed
/// flow). For now the handler logs the extraction and returns no events —
/// safe placeholder that proves the wire works without ghost-emitting
/// duplicate transactions during testing.
pub struct ScNgnHandler {
    name: String,
    /// NUBAN account number used to derive the per-statement PDF password.
    /// Stored as a `String` because SC accounts have leading zeros that
    /// must be preserved literally (integer storage would drop them).
    account_number: String,
    /// hledger account this SC account maps to (e.g. "Assets:StandardChartered:USD").
    /// Used as the bank-side posting account in emitted events.
    hledger_account: String,
    /// Commodity the account holds — fallback if extraction doesn't include it.
    commodity: String,
    device_id: String,
    extractor: Arc<dyn DocumentExtractor>,
}

impl ScNgnHandler {
    pub fn new(
        name: impl Into<String>,
        account_number: String,
        hledger_account: impl Into<String>,
        commodity: impl Into<String>,
        device_id: impl Into<String>,
        extractor: Arc<dyn DocumentExtractor>,
    ) -> Self {
        Self {
            name: name.into(),
            account_number,
            hledger_account: hledger_account.into(),
            commodity: commodity.into(),
            device_id: device_id.into(),
            extractor,
        }
    }
}

#[async_trait]
impl ImapHandler for ScNgnHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn accepts(&self, message: &ImapMessage) -> bool {
        message.from.to_lowercase().contains("@sc.com")
            && message.subject.to_lowercase().contains("statement")
    }

    async fn handle(&self, message: &ImapMessage) -> Result<Vec<NewEvent>, ImportError> {
        let parsed = parse_eml(&message.body)
            .map_err(|e| ImportError::Parse(format!("sc_ngn mime: {e}")))?;
        let pdf = parsed
            .find_attachment("application/pdf")
            .ok_or_else(|| ImportError::Parse("sc_ngn: no PDF attachment found".into()))?;

        let password = derive_pdf_password(&self.account_number)
            .map_err(|e| ImportError::NotConfigured(format!("sc_ngn password: {e}")))?;
        let text = decrypt_and_extract_bytes(&pdf.bytes, &password)
            .await
            .map_err(|e| ImportError::Upstream(format!("sc_ngn decrypt: {e}")))?;

        let result = self
            .extractor
            .extract(text.as_bytes(), "text/plain", ExtractionHint::BankStatement)
            .await
            .map_err(|e| ImportError::Upstream(format!("sc_ngn extract: {e}")))?;

        tracing::info!(
            handler = self.name(),
            confidence = result.confidence,
            postings = result.postings.len(),
            "sc_ngn: emitting events"
        );

        let source_prefix = format!("{}-uid-{}", self.name, message.uid);
        let events = statement_extraction_to_events(
            &result,
            &source_prefix,
            &self.hledger_account,
            &self.commodity,
            &self.device_id,
        );
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_extracts_chars_3_to_8() {
        assert_eq!(derive_pdf_password("0123456789").unwrap(), "234567");
        // 10-digit NUBAN, password is positions 3-8 (1-indexed) = chars
        // [2..8] (0-indexed).
    }

    #[test]
    fn derive_works_on_minimum_length_input() {
        assert_eq!(derive_pdf_password("01234567").unwrap(), "234567");
    }

    #[test]
    fn derive_rejects_too_short_input() {
        let err = derive_pdf_password("1234567").unwrap_err();
        assert!(matches!(err, ScError::AccountTooShort(7)));
    }

    #[test]
    fn derive_rejects_non_digit_chars() {
        let err = derive_pdf_password("01234567A").unwrap_err();
        assert!(matches!(err, ScError::NonDigitAccount(8)));
        let err2 = derive_pdf_password("ab234567").unwrap_err();
        assert!(matches!(err2, ScError::NonDigitAccount(0)));
    }

    #[test]
    fn derive_handles_longer_account_numbers_too() {
        // If SC ever issues 11+ digit accounts, the formula still grabs
        // the same range — won't randomly break.
        assert_eq!(derive_pdf_password("0123456789012345").unwrap(), "234567");
    }

    use chrono::TimeZone;

    fn make_imap_message(from: &str, subject: &str, body: Vec<u8>) -> ImapMessage {
        ImapMessage {
            uid: 1,
            from: from.into(),
            subject: subject.into(),
            date: chrono::Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
            body,
        }
    }

    #[test]
    fn handler_accepts_sc_estatement_mail() {
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ScNgnHandler::new(
            "sc_ngn",
            "0123456789".into(),
            "Assets:StandardChartered:USD",
            "USD",
            "device-test",
            extractor,
        );
        let msg = make_imap_message(
            "notifications@sc.com",
            "Your Estatement on 30042026 now available",
            Vec::new(),
        );
        assert!(handler.accepts(&msg));
    }

    #[test]
    fn handler_rejects_non_sc_mail() {
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ScNgnHandler::new(
            "sc_ngn",
            "0123456789".into(),
            "Assets:StandardChartered:USD",
            "USD",
            "device-test",
            extractor,
        );
        let other = make_imap_message("random@example.com", "Statement attached", Vec::new());
        assert!(!handler.accepts(&other));
    }

    #[test]
    fn handler_rejects_sc_non_statement_mail() {
        // Marketing/security/promo emails from @sc.com shouldn't try to
        // decrypt — they don't have PDF attachments to decrypt anyway.
        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ScNgnHandler::new(
            "sc_ngn",
            "0123456789".into(),
            "Assets:StandardChartered:USD",
            "USD",
            "device-test",
            extractor,
        );
        let promo = make_imap_message("offers@sc.com", "New rewards await", Vec::new());
        assert!(!handler.accepts(&promo));
    }

    /// Full handler pass against the real SC eml — parse MIME → find PDF →
    /// decrypt → extract → call NullExtractor → return. Skipped without
    /// the account number, same as the decrypt-only test below.
    #[tokio::test]
    async fn handler_processes_real_sc_eml_end_to_end() {
        let account = match std::env::var("SC_USD_ACCNT_NO") {
            Ok(v) => v,
            Err(_) => {
                eprintln!("SC_USD_ACCNT_NO not set — skipping handler e2e test");
                return;
            }
        };
        let eml_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".reference/imap poller")
            .join("Your Estatement on 30042026 now available.eml");
        let body = std::fs::read(&eml_path).expect("read eml fixture");
        let msg = make_imap_message(
            "notifications@sc.com",
            "Your Estatement on 30042026 now available",
            body,
        );

        let extractor = Arc::new(crate::extraction::null::NullExtractor);
        let handler = ScNgnHandler::new(
            "sc_ngn",
            account,
            "Assets:StandardChartered:USD",
            "USD",
            "device-test",
            extractor,
        );
        let events = handler.handle(&msg).await.expect("e2e should succeed");
        // NullExtractor returns no postings → mapper produces no events.
        // Wiring is complete; events will populate once a real extractor lands.
        assert!(events.is_empty());
    }

    /// Live decryption against the real reference statement (USD account).
    /// Set `SC_USD_ACCNT_NO` in `.env` to run this — otherwise the test is
    /// skipped (not failed) so CI / non-user environments stay green.
    /// Same password formula applies to NGN accounts (`SC_NGN_ACCNT_NO`)
    /// since SC uses one statement system across currencies.
    #[tokio::test]
    async fn decrypt_real_reference_statement_when_account_provided() {
        let account = match std::env::var("SC_USD_ACCNT_NO") {
            Ok(v) => v,
            Err(_) => {
                eprintln!("SC_USD_ACCNT_NO not set — skipping live decrypt test");
                return;
            }
        };
        let password = derive_pdf_password(&account).expect("valid NUBAN");
        let pdf_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".reference/imap poller")
            .join("CurrentSPCAccounts_00028XXXXX_000220380_USD_30042026_1_0000021190.pdf");
        let text = decrypt_and_extract(&pdf_path, &password)
            .await
            .expect("decrypt + extract should succeed with derived password");
        assert!(
            text.contains("Standard Chartered") || text.to_lowercase().contains("statement"),
            "extracted text should mention SC or 'statement'; got first 200 chars: {}",
            &text.chars().take(200).collect::<String>()
        );
        eprintln!("Decrypted SC statement, {} chars of text", text.len());
    }
}
