//! Reading archived documents with a model, on a schedule, after ingest.
//!
//! Why this is a scheduled pass rather than part of ingest, why the candidate
//! query is the work queue, and what it deliberately cannot reach:
//! `docs/src/archive.md`.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::blob;
use crate::db::{Database, DbError, queries};
use crate::document_fields;
use crate::events::{DocumentTextTranscribedPayload, EventWriter, NewEvent, WriteError};
use crate::extraction::document::DocumentReader;
use crate::extraction::transcribe::DocumentTranscriber;
use crate::extraction::{DocumentPart, READABLE_MIMES};

/// Documents one tick may read.
///
/// Small on purpose: the backlog drains across ticks, so a badly chosen model
/// shows itself after a handful of documents rather than after the archive.
pub const DEFAULT_MAX_PER_TICK: usize = 5;

/// Why a candidate produced no event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The blob is not on this host. Expected only once something other than
    /// `POST /documents/archive` can archive a document.
    NoBytes,
    /// Bytes exist but nothing could be put in front of a reader — an email
    /// whose body never parsed, so ingest stored no text to send in its place.
    NoReadableForm,
    /// The reader was asked and could not answer. `derive_fields` logs the
    /// cause; it is retried once its [`RetryHolds`] hold lapses.
    NotRead,
}

/// First hold on a skipped document; each further skip doubles it up to [`HOLD_CAP`].
pub const HOLD_START: Duration = Duration::from_secs(60 * 60);
pub const HOLD_CAP: Duration = Duration::from_secs(24 * 60 * 60);

/// Documents a tick skipped, set aside so one that keeps failing cannot hold the head of
/// the queue. In memory on purpose: a restart retries everything. See `docs/src/archive.md`.
#[derive(Debug, Default)]
pub struct RetryHolds {
    held: HashMap<String, Hold>,
}

#[derive(Debug, Clone, Copy)]
struct Hold {
    skips: u32,
    until: Instant,
}

impl RetryHolds {
    /// Ids still set aside at `now`.
    pub fn held_at(&self, now: Instant) -> Vec<String> {
        self.held
            .iter()
            .filter(|(_, hold)| hold.until > now)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Set every skipped document aside, doubling its hold each time it is skipped again.
    pub fn record(&mut self, skipped: &[SkippedDocument], now: Instant) {
        for doc in skipped {
            let skips = self.held.get(&doc.document_id).map_or(0, |h| h.skips) + 1;
            let hold = HOLD_START
                .saturating_mul(1 << (skips - 1).min(10))
                .min(HOLD_CAP);
            self.held.insert(
                doc.document_id.clone(),
                Hold {
                    skips,
                    until: now + hold,
                },
            );
        }
    }
}

/// Email MIMEs, which are candidates even though no reader accepts them raw.
///
/// An `.eml` is mostly MIME scaffolding and base64 payloads. What gets
/// catalogued is the text `archive::derive_text` already lifted at ingest.
const EMAIL_MIMES: [&str; 2] = ["message/rfc822", "message/global"];

/// Every MIME the pass may select, which is wider than what a reader accepts.
fn candidate_mimes() -> Vec<&'static str> {
    READABLE_MIMES.iter().copied().chain(EMAIL_MIMES).collect()
}

/// One candidate that produced no event, kept with enough identity to act on.
#[derive(Debug, Clone)]
pub struct SkippedDocument {
    pub document_id: String,
    pub reason: SkipReason,
}

/// What one tick did with every candidate it selected.
///
/// The identity `seen == read + every skip reason` is checked at
/// [`EnrichTally::finish`], on `ImportTally`'s reasoning: a pass that can
/// under-report is one whose quiet ticks are indistinguishable from its broken
/// ones. Each reason is counted rather than one being inferred by subtraction,
/// so a reason added later that nobody sums fails the identity instead of
/// hiding inside a sibling.
#[derive(Debug, Default)]
pub struct EnrichTally {
    seen: usize,
    read: usize,
    skipped: Vec<SkippedDocument>,
}

impl EnrichTally {
    pub fn new(seen: usize) -> Self {
        Self {
            seen,
            read: 0,
            skipped: Vec::new(),
        }
    }

    pub fn read(&mut self) {
        self.read += 1;
    }

    pub fn skipped(&mut self, document_id: impl Into<String>, reason: SkipReason) {
        self.skipped.push(SkippedDocument {
            document_id: document_id.into(),
            reason,
        });
    }

    /// Check the identity and produce the summary.
    pub fn finish(self) -> Result<EnrichSummary, EnrichError> {
        let count = |r: SkipReason| self.skipped.iter().filter(|s| s.reason == r).count();
        let no_bytes = count(SkipReason::NoBytes);
        let no_readable_form = count(SkipReason::NoReadableForm);
        let not_read = count(SkipReason::NotRead);
        let accounted = self.read + no_bytes + no_readable_form + not_read;
        if accounted != self.seen {
            return Err(EnrichError::Unaccounted {
                seen: self.seen,
                accounted,
            });
        }
        Ok(EnrichSummary {
            seen: self.seen,
            read: self.read,
            no_bytes,
            no_readable_form,
            not_read,
            unreadable_mime: 0,
            held: 0,
            skipped: self.skipped,
        })
    }
}

/// What one tick did.
#[derive(Debug, Default)]
pub struct EnrichSummary {
    /// Candidates selected this tick, already capped and MIME-filtered.
    pub seen: usize,
    pub read: usize,
    pub no_bytes: usize,
    pub no_readable_form: usize,
    pub not_read: usize,
    /// Uncatalogued documents the pass can never select, by MIME.
    ///
    /// Carried so they cannot be mistaken for documents that do not exist. It
    /// is a standing count of the whole archive, not of this tick.
    pub unreadable_mime: usize,
    /// Candidates set aside by [`RetryHolds`] when this tick selected.
    pub held: usize,
    pub skipped: Vec<SkippedDocument>,
}

impl EnrichSummary {
    /// The first few skips, for a log line that names files rather than a count.
    pub fn sample_skipped(&self, n: usize) -> Vec<&SkippedDocument> {
        self.skipped.iter().take(n).collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EnrichError {
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("{0}")]
    Write(#[from] WriteError),
    #[error("event error: {0}")]
    Event(#[from] serde_json::Error),
    #[error("tick selected {seen} documents but accounted for {accounted}")]
    Unaccounted { seen: usize, accounted: usize },
}

/// Read up to `max` uncatalogued documents and append what the reader found.
///
/// Takes the reader rather than building one so a caller with no reader
/// configured never reaches here and no tick is spent proving it.
pub async fn enrich_fields_once(
    db: &Database,
    writer: &EventWriter,
    blob_dir: &Path,
    reader: &dyn DocumentReader,
    max: usize,
    holds: &mut RetryHolds,
) -> Result<EnrichSummary, EnrichError> {
    let mimes = candidate_mimes();
    let held = holds.held_at(Instant::now());
    let candidates = queries::documents_awaiting_fields(db, &mimes, max as u32, &held).await?;
    let mut tally = EnrichTally::new(candidates.len());

    for row in candidates {
        let raw_mime = row
            .mime_type
            .as_deref()
            .unwrap_or("application/octet-stream");
        let Some(bytes) = read_blob(blob_dir, row.sha256.as_deref()).await else {
            tally.skipped(&row.document_id, SkipReason::NoBytes);
            continue;
        };

        // An email goes to the reader as the text ingest lifted out of it, never
        // as its raw source: an `.eml` is mostly MIME scaffolding and base64,
        // and cataloguing those bytes describes the envelope, not the message.
        let (bytes, mime) = if EMAIL_MIMES.contains(&raw_mime) {
            match queries::document_text(db, &row.document_id).await? {
                Some(text) if !text.trim().is_empty() => (text.into_bytes(), "text/plain"),
                _ => {
                    tally.skipped(&row.document_id, SkipReason::NoReadableForm);
                    continue;
                }
            }
        } else {
            (bytes, raw_mime)
        };

        // Through `derive_fields` rather than `read_document` directly, so the
        // parser still gets first refusal. A parser improved since this file was
        // ingested now claims it here, for free.
        match document_fields::derive_fields(&row.document_id, &bytes, mime, Some(reader)).await {
            Some(fields) => {
                writer
                    .append_new(NewEvent::document_fields_extracted(
                        writer.device_id(),
                        &fields,
                    )?)
                    .await?;
                tally.read();
            }
            None => tally.skipped(&row.document_id, SkipReason::NotRead),
        }
    }

    let mut summary = tally.finish()?;
    holds.record(&summary.skipped, Instant::now());
    summary.held = held.len();
    summary.unreadable_mime = queries::documents_unreadable_count(db, &mimes).await?;
    Ok(summary)
}

/// MIMEs a transcriber can be asked about.
///
/// Narrower than `READABLE_MIMES`: text and HTML documents already carry their
/// words, so `text_source` is never `none` for one and asking would spend a call
/// to be told what the file already said.
const TRANSCRIBABLE_MIMES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "application/pdf"];

/// Transcribe up to `max` documents nothing could read text off at ingest.
///
/// ⚠️ An empty transcription IS appended, and that is deliberate. It records
/// that a named model looked on a given date and found no text, which is a real
/// answer for a photograph of a blank page. It also lifts `text_source` off
/// `none`, so the document stops being a candidate — without that, every blank
/// page in the archive would be re-read on every tick forever and starve the cap.
/// A better model later is simply another event, per `TextSource::rank`'s `>=`.
pub async fn enrich_text_once(
    db: &Database,
    writer: &EventWriter,
    blob_dir: &Path,
    transcriber: &dyn DocumentTranscriber,
    max: usize,
    holds: &mut RetryHolds,
) -> Result<EnrichSummary, EnrichError> {
    let held = holds.held_at(Instant::now());
    let candidates =
        queries::documents_awaiting_text(db, &TRANSCRIBABLE_MIMES, max as u32, &held).await?;
    let mut tally = EnrichTally::new(candidates.len());

    for row in candidates {
        let Some(bytes) = read_blob(blob_dir, row.sha256.as_deref()).await else {
            tally.skipped(&row.document_id, SkipReason::NoBytes);
            continue;
        };
        let Some(mime) = row.mime_type.as_deref() else {
            tally.skipped(&row.document_id, SkipReason::NoReadableForm);
            continue;
        };

        match transcriber
            .transcribe(&[DocumentPart::new(&bytes, mime)])
            .await
        {
            Ok(text) => {
                let payload = DocumentTextTranscribedPayload {
                    document_id: row.document_id.clone(),
                    text,
                    model: transcriber.name().to_string(),
                    transcribed_at: Utc::now().to_rfc3339(),
                };
                writer
                    .append_new(NewEvent::document_text_transcribed(
                        writer.device_id(),
                        &payload,
                    )?)
                    .await?;
                tally.read();
            }
            Err(e) => {
                // Skip, never propagate — `derive_fields`' rule, for its reason.
                tracing::warn!(
                    document_id = %row.document_id,
                    transcriber = transcriber.name(),
                    error = %e,
                    "a document could not be transcribed"
                );
                tally.skipped(&row.document_id, SkipReason::NotRead);
            }
        }
    }

    let mut summary = tally.finish()?;
    holds.record(&summary.skipped, Instant::now());
    summary.held = held.len();
    summary.unreadable_mime = queries::documents_unreadable_count(db, &TRANSCRIBABLE_MIMES).await?;
    Ok(summary)
}

/// A document's bytes, or `None` when this host does not hold them.
///
/// A missing blob is an ordinary outcome rather than an error: it is what a
/// document archived on another host looks like from here.
async fn read_blob(blob_dir: &Path, sha256: Option<&str>) -> Option<Vec<u8>> {
    let path = blob::path_for(blob_dir, sha256?).ok()?;
    tokio::fs::read(path).await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::DocumentPart;
    use crate::extraction::ExtractionError;
    use crate::extraction::document::DocumentSummary;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StubReader {
        calls: AtomicUsize,
        fail: bool,
    }

    impl StubReader {
        fn new(fail: bool) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail,
            }
        }
    }

    #[async_trait]
    impl DocumentReader for StubReader {
        fn name(&self) -> &str {
            "stub"
        }

        async fn read_document(
            &self,
            _parts: &[DocumentPart<'_>],
        ) -> Result<DocumentSummary, ExtractionError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(ExtractionError::Upstream("stub refused".into()));
            }
            Ok(DocumentSummary {
                kind: "letter".into(),
                title: "A letter".into(),
                document_date: None,
                fields: vec![],
                model: "stub@1".into(),
            })
        }
    }

    #[test]
    fn the_identity_holds_when_every_candidate_is_classified() {
        let mut tally = EnrichTally::new(3);
        tally.read();
        tally.skipped("a", SkipReason::NoBytes);
        tally.skipped("b", SkipReason::NotRead);
        let summary = tally.finish().expect("all three accounted for");
        assert_eq!(
            (summary.read, summary.no_bytes, summary.not_read),
            (1, 1, 1)
        );
    }

    #[test]
    fn a_candidate_that_lands_in_no_bucket_fails_the_tick() {
        // The whole reason this type exists rather than a counter: a document
        // dropped by a code path that says nothing must not read as a quiet tick.
        let mut tally = EnrichTally::new(2);
        tally.read();
        assert!(matches!(
            tally.finish(),
            Err(EnrichError::Unaccounted {
                seen: 2,
                accounted: 1
            })
        ));
    }

    #[tokio::test]
    async fn a_document_whose_bytes_are_absent_is_a_skip_not_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_blob(dir.path(), Some(&"a".repeat(64))).await.is_none());
        assert!(read_blob(dir.path(), None).await.is_none());
    }

    // --- the pass end to end, against a real store and projection ---

    use crate::config::ALL_FEATURES;
    use crate::events::{DocumentsProjection, EventStore, ProjectionRunner, SurrealEventStore};
    use std::sync::Arc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("enrich.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        // Through the runner rather than `DocumentsProjection.init_schema` alone,
        // which is what this used to do: that leaves `projection_versions`
        // undefined, so every `apply_events` here failed at its bookmark write.
        // The failure was invisible until the writes started being checked.
        ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)])
            .init_all()
            .await
            .unwrap();
        db
    }

    /// A reader that records what it was actually handed.
    ///
    /// The email path's whole claim is about which bytes reach the model, and a
    /// stub that only returns a summary cannot check that.
    #[derive(Default)]
    struct SpyReader {
        seen: std::sync::Mutex<Vec<(String, String)>>,
    }

    impl SpyReader {
        fn last(&self) -> (String, String) {
            self.seen.lock().unwrap().last().cloned().expect("one call")
        }
    }

    #[async_trait]
    impl DocumentReader for SpyReader {
        fn name(&self) -> &str {
            "spy"
        }

        async fn read_document(
            &self,
            parts: &[DocumentPart<'_>],
        ) -> Result<DocumentSummary, ExtractionError> {
            let part = &parts[0];
            self.seen.lock().unwrap().push((
                String::from_utf8_lossy(part.bytes).into_owned(),
                part.mime.to_string(),
            ));
            Ok(DocumentSummary {
                kind: "letter".into(),
                title: "A letter".into(),
                document_date: None,
                fields: vec![],
                model: "spy@1".into(),
            })
        }
    }

    /// Archive one document with no fields, its bytes on disk, as ingest leaves it.
    async fn seed(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        blob_dir: &Path,
        bytes: &[u8],
        mime: &str,
    ) -> String {
        seed_with_text(
            store,
            runner,
            blob_dir,
            bytes,
            mime,
            "a letter, not a statement",
        )
        .await
    }

    /// As [`seed`], but naming the text ingest lifted out of the file.
    async fn seed_with_text(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        blob_dir: &Path,
        bytes: &[u8],
        mime: &str,
        text: &str,
    ) -> String {
        let sha256 = blob::store(blob_dir, bytes).await.unwrap();
        let document_id = ulid::Ulid::new().to_string();
        let e = store
            .append(NewEvent {
                id: None,
                event_type: "document_archived".into(),
                aggregate_id: document_id.clone(),
                timestamp: chrono::Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "document_id": document_id,
                    "sha256": sha256,
                    "filename": "a-letter.txt",
                    "mime_type": mime,
                    "size": bytes.len() as u64,
                    "archived_at": "2026-09-15T10:00:00Z",
                    "source": "upload",
                    "text": text,
                    "text_source": "extracted",
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();
        document_id
    }

    async fn kind_of(db: &Database, id: &str) -> Option<String> {
        queries::get_document(db, id)
            .await
            .unwrap()
            .and_then(|r| r.kind)
    }

    #[tokio::test]
    async fn the_pass_catalogues_a_document_and_then_stops_selecting_it() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        let id = seed(
            &store,
            &runner,
            blob_dir.path(),
            b"a letter, not a statement",
            "text/plain",
        )
        .await;

        let reader = StubReader::new(false);
        let first = enrich_fields_once(
            &db,
            &writer,
            blob_dir.path(),
            &reader,
            DEFAULT_MAX_PER_TICK,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();
        assert_eq!((first.seen, first.read), (1, 1));
        assert_eq!(kind_of(&db, &id).await.as_deref(), Some("letter"));

        // The candidate query is the queue, so the fold that lands the answer is
        // also what removes the work. Nothing durable tracks it.
        let second = enrich_fields_once(
            &db,
            &writer,
            blob_dir.path(),
            &reader,
            DEFAULT_MAX_PER_TICK,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            second.seen, 0,
            "an enriched document is no longer a candidate"
        );
        assert_eq!(reader.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_refusing_reader_spends_one_call_and_leaves_the_document_for_later() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        let id = seed(&store, &runner, blob_dir.path(), b"a letter", "text/plain").await;

        let reader = StubReader::new(true);
        let out = enrich_fields_once(
            &db,
            &writer,
            blob_dir.path(),
            &reader,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();

        assert_eq!((out.seen, out.read, out.not_read), (1, 0, 1));
        assert!(kind_of(&db, &id).await.is_none(), "no event was appended");
        assert_eq!(out.sample_skipped(1)[0].reason, SkipReason::NotRead);
    }

    fn skip(id: &str) -> Vec<SkippedDocument> {
        vec![SkippedDocument {
            document_id: id.into(),
            reason: SkipReason::NotRead,
        }]
    }

    #[test]
    fn a_hold_doubles_with_each_skip_and_stops_at_the_cap() {
        let now = Instant::now();
        let second = Duration::from_secs(1);
        let mut holds = RetryHolds::default();

        holds.record(&skip("a"), now);
        assert_eq!(holds.held_at(now), vec!["a".to_string()]);
        assert!(
            holds.held_at(now + HOLD_START).is_empty(),
            "first hold lapses"
        );

        holds.record(&skip("a"), now);
        assert!(
            !holds.held_at(now + HOLD_START).is_empty(),
            "second hold is longer"
        );
        assert!(holds.held_at(now + HOLD_START * 2).is_empty());

        for _ in 0..20 {
            holds.record(&skip("a"), now);
        }
        assert!(!holds.held_at(now + HOLD_CAP - second).is_empty());
        assert!(
            holds.held_at(now + HOLD_CAP).is_empty(),
            "never beyond the cap"
        );
    }

    #[tokio::test]
    async fn a_document_that_keeps_failing_does_not_block_the_one_behind_it() {
        // Found on real data: with one read per tick, a note the reader timed out on was
        // selected first on every tick, and nothing behind it was read until a retry succeeded.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        let mut holds = RetryHolds::default();

        let stuck = seed(
            &store,
            &runner,
            blob_dir.path(),
            b"a hard note",
            "text/plain",
        )
        .await;
        let failing = StubReader::new(true);
        let first = enrich_fields_once(&db, &writer, blob_dir.path(), &failing, 1, &mut holds)
            .await
            .unwrap();
        assert_eq!((first.seen, first.not_read), (1, 1));

        let second = enrich_fields_once(&db, &writer, blob_dir.path(), &failing, 1, &mut holds)
            .await
            .unwrap();
        assert_eq!(
            (second.seen, second.held),
            (0, 1),
            "the failure is set aside"
        );
        assert_eq!(
            failing.calls.load(Ordering::SeqCst),
            1,
            "and not paid for again"
        );

        let behind = seed(&store, &runner, blob_dir.path(), b"a letter", "text/plain").await;
        let reader = StubReader::new(false);
        let third = enrich_fields_once(&db, &writer, blob_dir.path(), &reader, 1, &mut holds)
            .await
            .unwrap();
        assert_eq!((third.seen, third.read), (1, 1));
        assert_eq!(kind_of(&db, &behind).await.as_deref(), Some("letter"));
        assert!(kind_of(&db, &stuck).await.is_none());
    }

    #[tokio::test]
    async fn an_email_is_catalogued_from_its_text_not_its_raw_source() {
        // The oracle role C2's bench leans on. If production could not catalogue
        // an email, benching C2 on email headers would measure a path nobody runs.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        let raw = b"From: billing@example.com\r\nSubject: Invoice\r\n\r\nbase64 scaffolding";
        let id = seed_with_text(
            &store,
            &runner,
            blob_dir.path(),
            raw,
            "message/rfc822",
            "From: billing@example.com\nSubject: Invoice\n\nYour invoice is attached.",
        )
        .await;

        let reader = SpyReader::default();
        let out = enrich_fields_once(
            &db,
            &writer,
            blob_dir.path(),
            &reader,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();

        assert_eq!((out.seen, out.read), (1, 1), "an email is a candidate");
        assert_eq!(kind_of(&db, &id).await.as_deref(), Some("letter"));
        let (sent, mime) = reader.last();
        assert_eq!(mime, "text/plain", "sent as text, not as message/rfc822");
        assert!(
            sent.contains("Your invoice is attached."),
            "the reader saw the lifted body, got: {sent}"
        );
        assert!(
            !sent.contains("base64 scaffolding"),
            "the raw source must not be what gets catalogued"
        );
    }

    #[tokio::test]
    async fn an_email_ingest_could_not_parse_is_named_not_swallowed() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        seed_with_text(
            &store,
            &runner,
            blob_dir.path(),
            b"\x00\x01 not an email",
            "message/rfc822",
            "",
        )
        .await;

        let reader = SpyReader::default();
        let out = enrich_fields_once(
            &db,
            &writer,
            blob_dir.path(),
            &reader,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();

        assert_eq!((out.seen, out.no_readable_form), (1, 1));
        assert_eq!(out.read, 0);
        assert_eq!(
            out.sample_skipped(1)[0].reason,
            SkipReason::NoReadableForm,
            "its own reason, so it is not read as a model failure"
        );
    }

    /// A transcriber returning whatever it is told to.
    struct StubTranscriber(String);

    #[async_trait]
    impl DocumentTranscriber for StubTranscriber {
        fn name(&self) -> &str {
            "stub-transcriber@1"
        }

        async fn transcribe(&self, _parts: &[DocumentPart<'_>]) -> Result<String, ExtractionError> {
            Ok(self.0.clone())
        }
    }

    async fn text_source_of(db: &Database, id: &str) -> Option<String> {
        queries::get_document(db, id)
            .await
            .unwrap()
            .and_then(|r| r.text_source)
    }

    /// Seed a scan: bytes on disk, no text, as ingest leaves an image.
    async fn seed_scan(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        blob_dir: &Path,
    ) -> String {
        let sha256 = blob::store(blob_dir, b"\xff\xd8\xff pretend jpeg")
            .await
            .unwrap();
        let document_id = ulid::Ulid::new().to_string();
        let e = store
            .append(NewEvent {
                id: None,
                event_type: "document_archived".into(),
                aggregate_id: document_id.clone(),
                timestamp: chrono::Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "document_id": document_id,
                    "sha256": sha256,
                    "filename": "scan.jpg",
                    "mime_type": "image/jpeg",
                    "size": 14u64,
                    "archived_at": "2026-09-15T10:00:00Z",
                    "source": "scan",
                    "text_source": "none",
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();
        document_id
    }

    #[tokio::test]
    async fn a_scan_gains_text_and_stops_being_a_candidate() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        let id = seed_scan(&store, &runner, blob_dir.path()).await;

        let t = StubTranscriber("INVOICE\ntotal 12.30".into());
        let first = enrich_text_once(
            &db,
            &writer,
            blob_dir.path(),
            &t,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();
        assert_eq!((first.seen, first.read), (1, 1));
        assert_eq!(
            text_source_of(&db, &id).await.as_deref(),
            Some("transcribed")
        );

        let second = enrich_text_once(
            &db,
            &writer,
            blob_dir.path(),
            &t,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            second.seen, 0,
            "a transcribed scan is no longer a candidate"
        );
    }

    #[tokio::test]
    async fn a_blank_page_is_recorded_rather_than_retried_forever() {
        // ⛔ The starvation case. An empty transcription is a real answer, and
        // appending it is what lifts text_source off `none`. Skipping it instead
        // would re-read every blank page on every tick and starve the cap.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        let id = seed_scan(&store, &runner, blob_dir.path()).await;

        let t = StubTranscriber(String::new());
        let out = enrich_text_once(
            &db,
            &writer,
            blob_dir.path(),
            &t,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();

        assert_eq!(
            (out.seen, out.read),
            (1, 1),
            "an empty answer still counts as read"
        );
        assert_eq!(
            text_source_of(&db, &id).await.as_deref(),
            Some("transcribed")
        );
        let again = enrich_text_once(
            &db,
            &writer,
            blob_dir.path(),
            &t,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();
        assert_eq!(again.seen, 0);
    }

    #[tokio::test]
    async fn a_document_that_already_has_text_is_never_transcribed() {
        // text_source is `extracted` for these, and re-reading one would spend a
        // call to be told what the file already stated verbatim.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        seed(&store, &runner, blob_dir.path(), b"a letter", "text/plain").await;

        let t = StubTranscriber("should never be asked".into());
        let out = enrich_text_once(
            &db,
            &writer,
            blob_dir.path(),
            &t,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();
        assert_eq!(out.seen, 0);
    }

    #[tokio::test]
    async fn a_mime_no_reader_accepts_never_becomes_a_candidate() {
        // Without this the cap would be spent on the same unreadable documents
        // every tick, and nothing behind them would ever drain.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        );
        let blob_dir = tempfile::tempdir().unwrap();
        seed(
            &store,
            &runner,
            blob_dir.path(),
            b"PK\x03\x04zip",
            "application/zip",
        )
        .await;

        let reader = StubReader::new(false);
        let out = enrich_fields_once(
            &db,
            &writer,
            blob_dir.path(),
            &reader,
            5,
            &mut RetryHolds::default(),
        )
        .await
        .unwrap();

        assert_eq!(out.seen, 0);
        assert_eq!(reader.calls.load(Ordering::SeqCst), 0, "nothing was sent");
        assert_eq!(
            out.unreadable_mime, 1,
            "counted rather than silently excluded"
        );
    }
}
