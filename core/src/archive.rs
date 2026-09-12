//! Document ingest — bytes in, a blob and a `DocumentArchived` event out.
//!
//! Why the archive is two events, how fields fold, and why text travels in the
//! event: `docs/src/archive.md`.
//!
//! ⛔ **Do not wire an extractor in here.** Only deterministic text is produced
//! in this module. A scan carries no text layer, so reading one needs a model —
//! and *which event carries model-transcribed text* is an open design question
//! rather than an oversight: `DocumentArchived` is the only carrier today and it
//! is written once, at ingest. A transcription quietly added here would be
//! unreachable for every document already filed.

use std::path::Path;

use crate::blob;
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
    pub archived: Vec<NewEvent>,
    /// `(path, reason)` per file that could not be archived at all.
    pub failed: Vec<(String, String)>,
    /// Archived, but with no text layer — so findable by name and not by content.
    ///
    /// ⚠️ Not a failure and deliberately not counted as one. It is reported
    /// separately because a batch that archives 700 files and can read none of
    /// them is technically a success and practically a problem.
    pub without_text: usize,
}

impl IngestReport {
    pub fn check_accounting(&self) -> Result<(), String> {
        let accounted = self.archived.len() + self.failed.len();
        if accounted != self.seen {
            return Err(format!(
                "ingest did not account for every file: saw {} but classified {} \
                 ({} archived + {} failed)",
                self.seen,
                accounted,
                self.archived.len(),
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

/// Store one document's bytes and build the event that records it.
///
/// ⛔ Returns the event rather than appending it — see the module note.
pub async fn ingest_one(
    blob_dir: &Path,
    bytes: &[u8],
    filename: &str,
    mime: &str,
    source: IngestSource,
    device_id: &str,
) -> Result<(NewEvent, TextSource), IngestError> {
    let sha256 = blob::store(blob_dir, bytes).await?;
    let (text, text_source) = derive_text(bytes, mime).await;

    let payload = DocumentArchivedPayload {
        // ⚠️ A fresh id per ingest, deliberately not the hash. The same file
        // arriving twice from two sources is two archive entries — see
        // `DocumentArchivedPayload::document_id`.
        document_id: ulid::Ulid::new().to_string(),
        sha256,
        filename: filename.to_string(),
        mime_type: mime.to_string(),
        size: bytes.len() as u64,
        archived_at: chrono::Utc::now().to_rfc3339(),
        source: source.as_str().to_string(),
        text,
        text_source: text_source.as_str().to_string(),
    };

    Ok((
        NewEvent::document_archived(device_id, &payload)?,
        text_source,
    ))
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

        match ingest_one(blob_dir, &bytes, &filename, &mime, source, device_id).await {
            Ok((event, text_source)) => {
                if text_source == TextSource::None {
                    report.without_text += 1;
                }
                report.archived.push(event);
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
        _ => "application/octet-stream".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[tokio::test]
    async fn a_csv_is_archived_with_its_text_readable() {
        let d = dir();
        let csv = b"date,description,amount\n2026-03-01,RENT,-1450.00\n";
        let (event, source) = ingest_one(
            d.path(),
            csv,
            "chequing.csv",
            "text/csv",
            IngestSource::Bulk,
            "dev",
        )
        .await
        .unwrap();

        assert_eq!(source, TextSource::Extracted);
        assert_eq!(event.event_type, "document_archived");
        let payload: DocumentArchivedPayload = serde_json::from_value(event.payload).unwrap();
        assert!(payload.text.unwrap().contains("RENT"));
        assert_eq!(payload.size, csv.len() as u64);
    }

    #[tokio::test]
    async fn a_file_with_no_readable_text_is_still_archived() {
        // The whole reason ingest and extraction are separate events. An image
        // carries no text layer, and refusing it would mean the archive cannot
        // hold a photograph of a document.
        let d = dir();
        let (event, source) = ingest_one(
            d.path(),
            &[0xFF, 0xD8, 0xFF, 0xE0, 0x00],
            "receipt.jpg",
            "image/jpeg",
            IngestSource::Scan,
            "dev",
        )
        .await
        .unwrap();

        assert_eq!(source, TextSource::None);
        let payload: DocumentArchivedPayload = serde_json::from_value(event.payload).unwrap();
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
        let (a, _) = ingest_one(
            d.path(),
            bytes,
            "s.csv",
            "text/csv",
            IngestSource::Email,
            "dev",
        )
        .await
        .unwrap();
        let (b, _) = ingest_one(
            d.path(),
            bytes,
            "s.csv",
            "text/csv",
            IngestSource::Scan,
            "dev",
        )
        .await
        .unwrap();

        let pa: DocumentArchivedPayload = serde_json::from_value(a.payload).unwrap();
        let pb: DocumentArchivedPayload = serde_json::from_value(b.payload).unwrap();
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
        assert_eq!(report.archived.len(), 1);
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

        assert_eq!(report.archived.len(), 2);
        assert_eq!(report.without_text, 1);
        report.check_accounting().unwrap();
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
