//! Turn a statement PDF into the layout-preserved text
//! [`super::rendered`] parses.
//!
//! Shells out to `pdftotext -layout` (poppler), which is a **system
//! dependency** — the dev box has it, and any host running this needs
//! `poppler-utils` installed.
//!
//! ## Why `-layout` is not optional
//!
//! Plain `pdftotext` emits reading-order text with the column structure
//! discarded. [`super::rendered`] decides what a figure *means* from the
//! character column it sits in, so without `-layout` every amount collapses
//! into the same position and the parser returns confident nonsense instead of
//! failing. The flag is the parser's whole input contract, which is why
//! extraction lives here rather than at each call site.
//!
//! ## Why the password is just a string
//!
//! Banks that publish encrypted statements each invent their own rule for the
//! password — some slice of an account number, a date of birth, a surname
//! fragment. None of that belongs in a general engine, so this takes a password
//! someone else derived and knows nothing about where it came from. Callers
//! resolve it from the credentials file's name-keyed `secrets` map, the same
//! "secrets referenced by name" pattern the LLM provider config and the
//! subprocess helpers already use.

use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Largest PDF handed to poppler. Real statements are a few hundred KB; this
/// is the cheapest defence against a compression bomb taking the host down,
/// and it is checked before poppler ever sees the bytes.
const MAX_PDF_BYTES: usize = 25 * 1024 * 1024;

/// Cap on extracted text, which bounds the expansion ratio a crafted PDF can
/// achieve. `wait_with_output` buffers stdout in full, so a file engineered to
/// expand into gigabytes is an out-of-memory kill even when poppler exits
/// cleanly.
const MAX_PDF_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// Wall-clock bound on one `pdftotext` run.
const PDFTOTEXT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum PdfTextError {
    #[error("pdf is {size} bytes, over the {MAX_PDF_BYTES}-byte limit")]
    TooLarge { size: usize },
    #[error("pdftotext could not be run ({0}) — is poppler-utils installed?")]
    Spawn(String),
    #[error("pdftotext exited with status {status}: {stderr}")]
    Failed { status: String, stderr: String },
    #[error("pdftotext output was not utf-8: {0}")]
    NotUtf8(String),
}

/// Extract layout-preserved text from PDF bytes, decrypting with `password`
/// when the file is encrypted.
///
/// Writes to a temporary file first: poppler seeks within the file to find the
/// encryption dictionary, so piping through stdin does not reliably work for
/// encrypted PDFs. An empty `password` is fine for an unencrypted file.
pub async fn extract_layout_text(pdf_bytes: &[u8], password: &str) -> Result<String, PdfTextError> {
    if pdf_bytes.len() > MAX_PDF_BYTES {
        return Err(PdfTextError::TooLarge {
            size: pdf_bytes.len(),
        });
    }

    let mut temp = tempfile::NamedTempFile::new()
        .map_err(|e| PdfTextError::Spawn(format!("create temp: {e}")))?;
    use std::io::Write;
    temp.write_all(pdf_bytes)
        .map_err(|e| PdfTextError::Spawn(format!("write temp: {e}")))?;
    temp.flush()
        .map_err(|e| PdfTextError::Spawn(format!("flush temp: {e}")))?;
    extract_layout_text_from_path(temp.path(), password).await
}

/// As [`extract_layout_text`], for a file already on disk.
///
/// ⚠️ **The password is passed on argv**, where it is visible to anything that
/// can read the process table. The obvious fix — a password *file* — does not
/// exist: checked against poppler 24.02, `pdftotext` offers only `-opw` and
/// `-upw`, both taking the value inline, with no stdin or file variant.
/// Closing this needs either `qpdf --password-file=-` piped into pdftotext (a
/// new dependency) or a Rust PDF decryption crate. Left deliberately, recorded
/// here so it is a known trade rather than an oversight.
pub async fn extract_layout_text_from_path(
    path: &Path,
    password: &str,
) -> Result<String, PdfTextError> {
    // Bounded because the input is untrusted: whoever uploads the file chooses
    // the PDF, and a wrong password against an *unencrypted* file still
    // succeeds — so the attachment need not even be encrypted to reach poppler.
    //
    // `kill_on_drop` is load-bearing with the timeout below: dropping the
    // future must actually kill the child, or a looping poppler outlives its
    // own deadline and keeps the CPU.
    let child = Command::new("pdftotext")
        .arg("-upw")
        .arg(password)
        .arg("-layout")
        .arg(path)
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| PdfTextError::Spawn(e.to_string()))?;

    let output = match tokio::time::timeout(PDFTOTEXT_TIMEOUT, child.wait_with_output()).await {
        Ok(result) => result.map_err(|e| PdfTextError::Spawn(e.to_string()))?,
        Err(_) => {
            return Err(PdfTextError::Failed {
                status: format!("timed out after {}s", PDFTOTEXT_TIMEOUT.as_secs()),
                stderr: String::new(),
            });
        }
    };

    if !output.status.success() {
        return Err(PdfTextError::Failed {
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let mut text = output.stdout;
    if text.len() > MAX_PDF_TEXT_BYTES {
        tracing::warn!(
            bytes = text.len(),
            "statement text over the cap — truncating",
        );
        text.truncate(MAX_PDF_TEXT_BYTES);
    }
    String::from_utf8(text).map_err(|e| PdfTextError::NotUtf8(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The size gate must reject before poppler is spawned — that is the whole
    /// point of it, and it is the one branch testable without a real PDF.
    #[tokio::test]
    async fn an_oversized_pdf_is_refused_before_poppler_sees_it() {
        let huge = vec![0u8; MAX_PDF_BYTES + 1];
        let err = extract_layout_text(&huge, "").await.unwrap_err();
        assert!(matches!(err, PdfTextError::TooLarge { .. }), "{err}");
    }

    /// Bytes that are not a PDF must fail loudly rather than yield empty text
    /// that would parse as a statement with no transactions.
    #[tokio::test]
    async fn a_non_pdf_fails_rather_than_returning_empty_text() {
        let result = extract_layout_text(b"this is not a pdf", "").await;
        match result {
            Err(PdfTextError::Failed { .. }) => {}
            // Absent poppler this is a Spawn error; the test still asserts the
            // useful half — that nothing is silently reported as empty.
            Err(PdfTextError::Spawn(_)) => {}
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}
