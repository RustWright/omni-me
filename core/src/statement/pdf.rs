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
    #[error("the pdf is encrypted and none of the {tried} configured password(s) opened it")]
    Encrypted { tried: usize },
}

/// What poppler prints when the password is wrong.
///
/// ⚠️ A string match, and it has to be: measured against poppler 24.02, a wrong
/// password, an empty password and a damaged file **all exit 1**. The man page
/// documents no code for encryption — only 0, 1, 2, 3 and 99 — so stderr is the
/// only place the two are distinguishable, and the callers need to be: "none of
/// your passwords opened this" and "this file is damaged" are different jobs.
/// ⛔ If a poppler release rewords this, [`is_wrong_password`] silently stops
/// recognising it, and an encrypted document is then reported as a damaged one
/// rather than mis-opened. That is why the fallback keeps poppler's own words.
const WRONG_PASSWORD_STDERR: &str = "Incorrect password";

/// Whether a failure was the password rather than the file.
fn is_wrong_password(stderr: &str) -> bool {
    stderr.contains(WRONG_PASSWORD_STDERR)
}

/// The same question for `pdftoppm`, which prints the same line.
///
/// Exported so the rasterizer shares this match rather than keeping a second copy:
/// two copies is how one path keeps recognising an encrypted file after a poppler
/// release rewords the message and the other stops.
pub fn stderr_is_wrong_password(stderr: &str) -> bool {
    is_wrong_password(stderr)
}

/// Text, and which password opened the file.
pub struct Opened {
    pub text: String,
    /// Index into the candidate list, or `None` when no password was needed.
    pub opened_by: Option<usize>,
}

/// ⛔ Never derive `Debug` here — `text` is a whole statement, and a log line is
/// the easiest place for a person's records to escape. Length only.
impl std::fmt::Debug for Opened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opened")
            .field("text_bytes", &self.text.len())
            .field("opened_by", &self.opened_by)
            .finish()
    }
}

/// Extract text, trying each candidate password until one opens the file.
///
/// ⚠️ The empty password is always tried first, and that is what keeps this cheap:
/// an unencrypted file opens on it, and poppler *succeeds* on an unencrypted file
/// given a wrong password anyway — so the 80% of a corpus that is not encrypted
/// still costs exactly one `pdftotext` run however many passwords are configured.
///
/// Why every password rather than the right one: ingest has no document-to-issuer
/// attribution to key on. `docs/src/archive.md` § Encrypted documents.
pub async fn extract_layout_text_with_any(
    pdf_bytes: &[u8],
    passwords: &[&str],
) -> Result<Opened, PdfTextError> {
    match extract_layout_text(pdf_bytes, "").await {
        Ok(text) => {
            return Ok(Opened {
                text,
                opened_by: None,
            });
        }
        // A damaged or oversized file is not going to open under a password, and
        // trying every one would multiply the same failure.
        Err(e) if !matches!(&e, PdfTextError::Failed { stderr, .. } if is_wrong_password(stderr)) =>
        {
            return Err(e);
        }
        Err(_) => {}
    }

    for (index, password) in passwords.iter().enumerate() {
        match extract_layout_text(pdf_bytes, password).await {
            Ok(text) => {
                return Ok(Opened {
                    text,
                    opened_by: Some(index),
                });
            }
            Err(PdfTextError::Failed { stderr, .. }) if is_wrong_password(&stderr) => continue,
            // Anything else is about the file, not the password.
            Err(e) => return Err(e),
        }
    }
    Err(PdfTextError::Encrypted {
        tried: passwords.len(),
    })
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

/// Wall-clock bound on one `pdfinfo` or `pdftocairo` run. Re-rendering every
/// page costs more than reading the text off them.
const DECRYPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, thiserror::Error)]
pub enum DecryptError {
    #[error("the pdf is encrypted and none of the {tried} configured password(s) opened it")]
    Encrypted { tried: usize },
    #[error("{0}")]
    Failed(String),
}

/// An unencrypted copy of an encrypted PDF, so a device that holds no password
/// can still show it. `None` when the file opens without one.
///
/// `pdftocairo -pdf` re-renders rather than strips the encryption (poppler has
/// no lossless decrypt; `qpdf` would add a dependency), and keeps the text
/// layer. The password goes on argv, as in [`extract_layout_text_from_path`].
pub async fn decrypted_copy_with_any(
    pdf_bytes: &[u8],
    passwords: &[&str],
) -> Result<Option<Vec<u8>>, DecryptError> {
    if pdf_bytes.len() > MAX_PDF_BYTES {
        return Err(DecryptError::Failed(format!(
            "pdf is {} bytes, over the {MAX_PDF_BYTES}-byte limit",
            pdf_bytes.len()
        )));
    }
    let dir = tempfile::tempdir().map_err(|e| DecryptError::Failed(format!("temp dir: {e}")))?;
    let input = dir.path().join("in.pdf");
    let output = dir.path().join("out.pdf");
    tokio::fs::write(&input, pdf_bytes)
        .await
        .map_err(|e| DecryptError::Failed(format!("write temp: {e}")))?;

    // `pdfinfo` is the cheap check: an unencrypted file is never re-rendered.
    match run_poppler("pdfinfo", &["-upw", "", path_arg(&input)?]).await {
        Ok(()) => return Ok(None),
        Err(stderr) if is_wrong_password(&stderr) => {}
        Err(stderr) => return Err(DecryptError::Failed(stderr)),
    }
    for password in passwords {
        let args = [
            "-pdf",
            "-upw",
            password,
            path_arg(&input)?,
            path_arg(&output)?,
        ];
        match run_poppler("pdftocairo", &args).await {
            Ok(()) => {
                let bytes = tokio::fs::read(&output)
                    .await
                    .map_err(|e| DecryptError::Failed(format!("read decrypted copy: {e}")))?;
                return Ok(Some(bytes));
            }
            Err(stderr) if is_wrong_password(&stderr) => continue,
            Err(stderr) => return Err(DecryptError::Failed(stderr)),
        }
    }
    Err(DecryptError::Encrypted {
        tried: passwords.len(),
    })
}

fn path_arg(path: &Path) -> Result<&str, DecryptError> {
    path.to_str()
        .ok_or_else(|| DecryptError::Failed("temp path is not utf-8".into()))
}

/// Run a poppler tool to completion; `Err` carries its stderr, which is where
/// the wrong-password case is told apart from a damaged file.
async fn run_poppler(tool: &str, args: &[&str]) -> Result<(), String> {
    let child = Command::new(tool)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("{tool} could not be run ({e}) — is poppler-utils installed?"))?;
    match tokio::time::timeout(DECRYPT_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(out)) if out.status.success() => Ok(()),
        Ok(Ok(out)) => Err(format!(
            "{tool} exited with status {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )),
        Ok(Err(e)) => Err(format!("{tool}: {e}")),
        Err(_) => Err(format!(
            "{tool} timed out after {}s",
            DECRYPT_TIMEOUT.as_secs()
        )),
    }
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

    /// A real encrypted PDF, because the whole design rests on telling poppler's
    /// wrong-password failure from its damaged-file one, and a mock would assert
    /// what we believe it prints. `core/tests/fixtures/encrypted/README.md`.
    const ENCRYPTED: &[u8] =
        include_bytes!("../../tests/fixtures/encrypted/encrypted-statement.pdf");

    #[tokio::test]
    async fn the_right_password_among_several_opens_the_file() {
        let opened = extract_layout_text_with_any(ENCRYPTED, &["wrong", "letmein", "also-wrong"])
            .await
            .expect("one candidate matches");
        assert!(opened.text.contains("1284.00"), "{}", opened.text);
        // The index, so a caller can log the secret's NAME and never its value.
        assert_eq!(opened.opened_by, Some(1));
    }

    #[tokio::test]
    async fn a_decrypted_copy_opens_with_no_password() {
        let copy = decrypted_copy_with_any(ENCRYPTED, &["wrong", "letmein"])
            .await
            .unwrap()
            .expect("the file is encrypted, so a copy is made");
        let text = extract_layout_text(&copy, "").await.unwrap();
        assert!(text.contains("1284.00"), "{text}");
        // An unencrypted file is never re-rendered.
        assert!(decrypted_copy_with_any(&copy, &[]).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn no_decrypted_copy_without_the_password() {
        let err = decrypted_copy_with_any(ENCRYPTED, &["wrong"])
            .await
            .unwrap_err();
        assert!(matches!(err, DecryptError::Encrypted { tried: 1 }), "{err}");
    }

    #[tokio::test]
    async fn no_matching_password_says_encrypted_rather_than_damaged() {
        let err = extract_layout_text_with_any(ENCRYPTED, &["wrong", "still-wrong"])
            .await
            .unwrap_err();
        match err {
            PdfTextError::Encrypted { tried } => assert_eq!(tried, 2),
            other => panic!("expected Encrypted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_empty_candidate_list_still_reports_encryption() {
        // The default install: no passwords configured. The document is still
        // encrypted, and saying so is what stops 140 files reading as empty-but-fine.
        let err = extract_layout_text_with_any(ENCRYPTED, &[])
            .await
            .unwrap_err();
        assert!(matches!(err, PdfTextError::Encrypted { tried: 0 }), "{err}");
    }

    #[tokio::test]
    async fn a_damaged_file_is_not_retried_against_every_password() {
        // Poppler exits 1 for both cases, so without the stderr check this would
        // spawn one process per configured password to fail the same way each time.
        let err = extract_layout_text_with_any(b"%PDF-1.4\ngarbage", &["a", "b", "c"])
            .await
            .unwrap_err();
        match err {
            PdfTextError::Failed { stderr, .. } => {
                assert!(!is_wrong_password(&stderr), "damaged, not locked: {stderr}");
            }
            PdfTextError::Spawn(_) => {}
            other => panic!("expected a file-level failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unencrypted_file_opens_without_consulting_any_password() {
        let plain = include_bytes!("../../tests/fixtures/pdf-routing/generated-receipt.pdf");
        let opened = extract_layout_text_with_any(plain, &["never-needed"])
            .await
            .expect("an unencrypted pdf opens on the empty password");
        assert_eq!(
            opened.opened_by, None,
            "`None` is what keeps the common case at one pdftotext run"
        );
    }
}
