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

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

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
