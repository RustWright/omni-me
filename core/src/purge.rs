//! Removing documents on purpose, bytes and all.
//!
//! The one irreversible operation in the archive, and the only place anything in
//! this project deletes a user's data. Everything here is shaped by that: it
//! previews before it acts, it counts what it did in a form that cannot flatter
//! itself, and it refuses to delete bytes anything else still references.
//!
//! ⛔ **This never decides *what* to purge.** A person does, per group, having
//! seen every item in it. The model may flag junk; flagging is a proposal, and
//! deletion is never grantable to autonomy. Why, and the alternatives that were
//! refused: `docs/src/archive.md`.
//!
//! ⚠️ **A purge does not empty the event log.** The archive event stays, text and
//! all — the log is append-only and a rebuild replays it. What a purge reclaims
//! is the *blob*, which is where the bytes actually are, and what it removes from
//! every read surface is the projection row's content. Those are different
//! things and conflating them is how "purged" comes to mean "still searchable".

use std::path::Path;

use crate::blob;
use crate::db::queries::{self, DocumentRow};
use crate::db::{Database, DbError};
use crate::events::{DocumentPurgedPayload, EventError, EventWriter, NewEvent, WriteError};

/// Items listed in one preview before the list is truncated.
///
/// ⚠️ The **count is always exact**; only the list is cut. Pointing a purge at a
/// thousand-message newsletter backlog must not build a thousand-row
/// confirmation nobody reads, and must not understate what is about to go.
/// `preview_obsidian_export` sized its own cap for the same reason.
pub const MAX_PREVIEW_ITEMS: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum PurgeError {
    #[error("purge query: {0}")]
    Db(#[from] DbError),
    #[error("purge write: {0}")]
    Write(#[from] WriteError),
    #[error("purge blob: {0}")]
    Blob(#[from] blob::BlobError),
}

/// One document a purge would remove.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PurgeItem {
    pub document_id: String,
    /// What a person recognises it by — the title if anything read it, else the
    /// filename. ⛔ Never empty: an unlabelled row is one nobody can decide about.
    pub label: String,
    pub archived_at: Option<String>,
    pub ingest_source: Option<String>,
    pub size: Option<i64>,
    /// Whether these bytes stay regardless, because something else references
    /// them.
    ///
    /// ⚠️ Shown per item rather than only in the totals: "purging this reclaims
    /// nothing" is a reason a person might spare it, and they can only act on it
    /// if they can see which one it applies to.
    pub bytes_shared: bool,
}

/// What a purge would do, computed without writing anything.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PurgePreview {
    /// The tag this group was formed under.
    pub group: String,
    /// Up to [`MAX_PREVIEW_ITEMS`] of them.
    pub items: Vec<PurgeItem>,
    /// Every matching document, whether or not it is listed above.
    pub total: usize,
    /// Bytes that would actually come back.
    pub bytes_reclaimable: u64,
    /// Bytes that would not, because another document or a transaction
    /// attachment still references the same blob.
    ///
    /// ⚠️ Reported separately rather than netted off. "Frees 31 MB" and "frees
    /// 0.3 MB of the 31 MB you selected" are different answers, and only the
    /// second is true when a corpus shares bytes.
    pub bytes_shared: u64,
}

/// What a purge actually did.
///
/// ⚠️ `selected == purged + skipped + failed`, asserted by
/// [`Self::check_accounting`] rather than assumed. The shape is
/// `archive::IngestReport`'s and exists for the same reason: a bare count of
/// successes is unfalsifiable, and for an irreversible operation an unnoticed
/// gap between "selected" and "done" is the worst possible thing to discover
/// later.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PurgeReport {
    pub selected: usize,
    pub purged: usize,
    /// Already purged when we got there — an ordinary outcome, not a failure.
    /// Two devices can confirm the same group.
    pub skipped: usize,
    pub failed: usize,
    pub blobs_deleted: usize,
    /// Blobs left alone because something else still references them.
    pub blobs_retained_shared: usize,
    pub bytes_deleted: u64,
}

impl PurgeReport {
    /// ⛔ Call before returning one. A purge that cannot account for every
    /// document it selected has lost track of an irreversible operation.
    pub fn check_accounting(&self) -> Result<(), String> {
        let sum = self.purged + self.skipped + self.failed;
        if sum != self.selected {
            return Err(format!(
                "purge accounting: selected {} but purged {} + skipped {} + failed {} = {sum}",
                self.selected, self.purged, self.skipped, self.failed
            ));
        }
        Ok(())
    }
}

/// What a purge of everything tagged `tag` would remove.
///
/// ⛔ Writes nothing and deletes nothing. The same selection the apply half uses,
/// so the preview cannot drift from what actually happens — the rule
/// `preview_obsidian_export` states outright.
///
/// `archived_before` narrows the group to documents older than an RFC3339 instant,
/// which is how a **retention** group is previewed: its members are "tagged X and
/// past X's rule", not the whole tag. ⛔ It narrows the one selection rather than
/// adding a second query, because the preview and the confirm have to be looking at
/// the same set.
pub async fn preview(
    db: &Database,
    tag: &str,
    archived_before: Option<&str>,
) -> Result<PurgePreview, PurgeError> {
    let rows = queries::documents_tagged(db, tag, archived_before).await?;
    let total = rows.len();

    let mut bytes_reclaimable = 0u64;
    let mut bytes_shared = 0u64;
    let mut items = Vec::with_capacity(rows.len().min(MAX_PREVIEW_ITEMS));
    // Bytes are a property of the blob, not of the row. Two selected documents
    // sharing one blob free it once, and [`apply`] dedupes for the same reason —
    // counting per row promised twice what a confirm could deliver, and the
    // report then contradicted the preview the person decided on.
    let mut counted: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // ⚠️ Counted over the whole set, not just the listed page. A total derived
    // from the truncated list would shrink as the selection grew, which is
    // exactly backwards.
    for row in &rows {
        let shared = is_shared(db, row, &rows).await?;
        // A row with no blob has no bytes to reclaim, whatever `size` says it was
        // at ingest — there is nothing to unlink.
        let first_sighting = row.sha256.as_deref().is_some_and(|sha| counted.insert(sha));
        if first_sighting {
            let size = row.size.unwrap_or(0).max(0) as u64;
            if shared {
                bytes_shared += size;
            } else {
                bytes_reclaimable += size;
            }
        }
        if items.len() < MAX_PREVIEW_ITEMS {
            items.push(PurgeItem {
                document_id: row.document_id.clone(),
                label: label_for(row),
                archived_at: row.archived_at.clone(),
                ingest_source: row.ingest_source.clone(),
                size: row.size,
                bytes_shared: shared,
            });
        }
    }

    Ok(PurgePreview {
        group: tag.to_string(),
        items,
        total,
        bytes_reclaimable,
        bytes_shared,
    })
}

/// Whether a row's bytes would survive this purge regardless.
///
/// ⚠️ References *inside the selection* do not count as sharing — they are about
/// to go too. Counting them would report every duplicate in a batch as shared and
/// make a purge look like it reclaims nothing.
async fn is_shared(
    db: &Database,
    row: &DocumentRow,
    selection: &[DocumentRow],
) -> Result<bool, PurgeError> {
    let Some(sha) = row.sha256.as_deref() else {
        // No bytes recorded, so nothing to reclaim and nothing shared.
        return Ok(false);
    };
    let outside = queries::blob_reference_count(db, sha).await?;
    let inside = selection
        .iter()
        .filter(|r| r.sha256.as_deref() == Some(sha))
        .count();
    Ok(outside > inside)
}

/// Title if anything has read the document, else its filename.
///
/// ⛔ Never the bare id. A row a person cannot identify is one they cannot
/// decide about, and this list is the whole of their evidence.
fn label_for(row: &DocumentRow) -> String {
    match (&row.title, &row.filename) {
        (Some(t), _) if !t.trim().is_empty() => t.clone(),
        (_, Some(f)) if !f.trim().is_empty() => f.clone(),
        _ => format!("Untitled document ({})", row.document_id),
    }
}

/// Purge exactly these documents, and reclaim any bytes nothing else needs.
///
/// ⛔ **The caller has already shown this exact list to the user.** Nothing here
/// re-derives a selection; an id that reaches this function is one a person
/// confirmed. That is what keeps deletion out of autonomy's reach.
///
/// The order is load-bearing. Every tombstone is written **first**, then blobs
/// are considered — because [`queries::blob_reference_count`] excludes purged
/// documents, so a blob becomes reclaimable only once every document naming it
/// has been tombstoned. Deleting as we went would refuse to free anything a batch
/// internally shares.
pub async fn apply(
    db: &Database,
    writer: &EventWriter,
    blob_dir: &Path,
    document_ids: &[String],
    reason: Option<&str>,
) -> Result<PurgeReport, PurgeError> {
    let mut report = PurgeReport {
        selected: document_ids.len(),
        ..Default::default()
    };

    let purged_at = chrono::Utc::now().to_rfc3339();
    let mut hashes: Vec<String> = Vec::new();

    for id in document_ids {
        let Some(row) = queries::get_document(db, id).await? else {
            // Nothing to purge and nothing to report as done — the id named a
            // document this host has never seen.
            report.failed += 1;
            tracing::warn!(document_id = %id, "purge: no such document");
            continue;
        };
        if row.purged.unwrap_or(false) {
            report.skipped += 1;
            continue;
        }

        let payload = DocumentPurgedPayload {
            document_id: id.clone(),
            purged_at: purged_at.clone(),
            reason: reason.map(str::to_string),
        };
        let event = NewEvent::document_purged(writer.device_id(), &payload)
            .map_err(|e| WriteError::Event(EventError::Validation(e.to_string())))?;
        match writer.append_new(event).await {
            Ok(_) => {
                report.purged += 1;
                if let Some(sha) = row.sha256 {
                    hashes.push(sha);
                }
            }
            Err(e) => {
                report.failed += 1;
                tracing::warn!(document_id = %id, error = %e, "purge: event rejected");
            }
        }
    }

    // Deduplicated: two documents sharing bytes must not be counted as two
    // reclaimed blobs, and must not attempt the same unlink twice.
    hashes.sort();
    hashes.dedup();

    for sha in hashes {
        let remaining = queries::blob_reference_count(db, &sha).await?;
        if remaining > 0 {
            report.blobs_retained_shared += 1;
            continue;
        }
        // Size before the unlink, since afterwards there is nothing to stat.
        let size = blob_size(blob_dir, &sha).await;
        if blob::delete(blob_dir, &sha).await? {
            report.blobs_deleted += 1;
            report.bytes_deleted += size;
        }
    }

    if let Err(e) = report.check_accounting() {
        tracing::error!(error = %e, "purge accounting failed");
    }
    tracing::info!(
        selected = report.selected,
        purged = report.purged,
        skipped = report.skipped,
        failed = report.failed,
        blobs_deleted = report.blobs_deleted,
        blobs_retained_shared = report.blobs_retained_shared,
        bytes_deleted = report.bytes_deleted,
        "purge complete"
    );
    Ok(report)
}

/// A blob's size on disk, or 0 when it is not here.
///
/// ⚠️ Measured rather than taken from `documents.size`: the row records the size
/// of the *document* as ingest saw it, and what a purge actually frees is the
/// file. On a host that never held the bytes the honest answer is zero.
async fn blob_size(dir: &Path, sha: &str) -> u64 {
    let Ok(path) = blob::path_for(dir, sha) else {
        return 0;
    };
    tokio::fs::metadata(path)
        .await
        .map(|m| m.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ALL_FEATURES;
    use crate::events::{
        DocumentsProjection, EventStore, NewEvent, ProjectionRunner, SurrealEventStore,
    };
    use std::sync::Arc;

    struct Harness {
        db: Database,
        writer: EventWriter,
        store: SurrealEventStore,
        runner: ProjectionRunner,
        blob_dir: tempfile::TempDir,
    }

    async fn harness() -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("purge.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(DocumentsProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "test-device",
        );
        Harness {
            db,
            writer,
            store,
            runner,
            blob_dir: tempfile::tempdir().unwrap(),
        }
    }

    /// Archive a document with real bytes and a tag, through the real projection.
    async fn seed(h: &Harness, bytes: &[u8], tag: &str) -> String {
        let sha256 = blob::store(h.blob_dir.path(), bytes).await.unwrap();
        let id = ulid::Ulid::new().to_string();
        for (event_type, payload) in [
            (
                "document_archived",
                serde_json::json!({
                    "document_id": id, "sha256": sha256, "filename": format!("{id}.pdf"),
                    "mime_type": "application/pdf", "size": bytes.len() as u64,
                    "archived_at": "2026-09-01T00:00:00Z", "source": "email",
                    "text": "spam body", "text_source": "extracted",
                }),
            ),
            (
                "document_fields_extracted",
                serde_json::json!({
                    "document_id": id, "extracted_at": "2026-09-01T00:00:00Z",
                    "fields": [{ "key": "tags", "value": tag,
                                 "source": "human", "verified": true }],
                }),
            ),
        ] {
            let e = h
                .store
                .append(NewEvent {
                    id: None,
                    event_type: event_type.into(),
                    aggregate_id: id.clone(),
                    timestamp: chrono::Utc::now(),
                    device_id: "test-device".into(),
                    payload,
                })
                .await
                .unwrap();
            h.runner.apply_events(&[e]).await.unwrap();
        }
        id
    }

    #[tokio::test]
    async fn a_preview_writes_nothing_and_deletes_nothing() {
        let h = harness().await;
        let id = seed(&h, b"one newsletter", "newsletter").await;

        let p = preview(&h.db, "newsletter", None).await.unwrap();
        assert_eq!(p.total, 1);
        assert_eq!(p.items[0].document_id, id);
        assert!(!p.items[0].bytes_shared);

        // ⛔ Still there, still unpurged, bytes intact.
        let row = queries::get_document(&h.db, &id).await.unwrap().unwrap();
        assert_ne!(row.purged, Some(true));
        let sha = row.sha256.unwrap();
        assert!(blob::path_for(h.blob_dir.path(), &sha).unwrap().exists());
    }

    /// The ordinary case, end to end: tombstone written, bytes gone, accounting
    /// adds up.
    #[tokio::test]
    async fn a_purge_tombstones_the_row_and_reclaims_the_bytes() {
        let h = harness().await;
        let id = seed(&h, b"one newsletter", "newsletter").await;
        let sha = queries::get_document(&h.db, &id)
            .await
            .unwrap()
            .unwrap()
            .sha256
            .unwrap();

        let report = apply(
            &h.db,
            &h.writer,
            h.blob_dir.path(),
            std::slice::from_ref(&id),
            Some("newsletter"),
        )
        .await
        .unwrap();

        report
            .check_accounting()
            .expect("must account for every id");
        assert_eq!((report.selected, report.purged), (1, 1));
        assert_eq!(report.blobs_deleted, 1);
        assert_eq!(report.bytes_deleted, b"one newsletter".len() as u64);
        assert!(!blob::path_for(h.blob_dir.path(), &sha).unwrap().exists());

        let row = queries::get_document(&h.db, &id).await.unwrap().unwrap();
        assert_eq!(row.purged, Some(true));
        // And it has left every read surface.
        assert!(
            queries::documents_tagged(&h.db, "newsletter", None)
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// 🔴 Identical bytes are one blob and two documents. Purging one must not
    /// take the other's file — the archive would keep a row pointing at nothing,
    /// with no record that anything had been removed.
    #[tokio::test]
    async fn purging_one_of_two_documents_sharing_bytes_keeps_the_file() {
        let h = harness().await;
        let emailed = seed(&h, b"identical bytes", "newsletter").await;
        let scanned = seed(&h, b"identical bytes", "keep").await;
        let sha = queries::get_document(&h.db, &emailed)
            .await
            .unwrap()
            .unwrap()
            .sha256
            .unwrap();

        let report = apply(&h.db, &h.writer, h.blob_dir.path(), &[emailed], None)
            .await
            .unwrap();

        assert_eq!(report.purged, 1);
        assert_eq!(
            report.blobs_deleted, 0,
            "the other filing still needs these"
        );
        assert_eq!(report.blobs_retained_shared, 1);
        assert_eq!(report.bytes_deleted, 0);
        assert!(
            blob::path_for(h.blob_dir.path(), &sha).unwrap().exists(),
            "⛔ the surviving document's bytes were deleted"
        );
        assert_ne!(
            queries::get_document(&h.db, &scanned)
                .await
                .unwrap()
                .unwrap()
                .purged,
            Some(true)
        );
    }

    /// ⚠️ Bytes shared only *within* the batch are reclaimable — both referrers
    /// are going. Treating them as shared would make a purge of duplicates report
    /// that it freed nothing.
    #[tokio::test]
    async fn bytes_shared_only_inside_the_batch_are_still_reclaimed() {
        let h = harness().await;
        let a = seed(&h, b"identical bytes", "newsletter").await;
        let b = seed(&h, b"identical bytes", "newsletter").await;

        let p = preview(&h.db, "newsletter", None).await.unwrap();
        assert_eq!(p.total, 2);
        assert_eq!(p.bytes_shared, 0, "both referrers are in the selection");

        let report = apply(&h.db, &h.writer, h.blob_dir.path(), &[a, b], None)
            .await
            .unwrap();
        assert_eq!(report.purged, 2);
        assert_eq!(report.blobs_deleted, 1, "one blob, deleted once");
    }

    /// Confirming the same group twice is an ordinary outcome — two devices can
    /// do it — and must not read as a failure.
    #[tokio::test]
    async fn purging_an_already_purged_document_is_skipped_not_failed() {
        let h = harness().await;
        let id = seed(&h, b"one newsletter", "newsletter").await;
        apply(
            &h.db,
            &h.writer,
            h.blob_dir.path(),
            std::slice::from_ref(&id),
            None,
        )
        .await
        .unwrap();

        let again = apply(&h.db, &h.writer, h.blob_dir.path(), &[id], None)
            .await
            .unwrap();
        again.check_accounting().unwrap();
        assert_eq!((again.purged, again.skipped, again.failed), (0, 1, 0));
    }

    /// An id naming nothing is a failure, and it must still balance.
    #[tokio::test]
    async fn an_unknown_id_fails_without_breaking_the_accounting() {
        let h = harness().await;
        let report = apply(
            &h.db,
            &h.writer,
            h.blob_dir.path(),
            &["01JKNOSUCHDOCUMENT00000000".to_string()],
            None,
        )
        .await
        .unwrap();
        report.check_accounting().expect("must still balance");
        assert_eq!((report.selected, report.failed), (1, 1));
    }

    /// ⛔ The accounting check has to be able to fail, or it is decoration.
    #[test]
    fn accounting_rejects_a_report_that_lost_a_document() {
        let bad = PurgeReport {
            selected: 3,
            purged: 1,
            ..Default::default()
        };
        assert!(bad.check_accounting().is_err());
    }
}
