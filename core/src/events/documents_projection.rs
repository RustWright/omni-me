//! SurrealDB projection over the document archive.
//!
//! One table, `documents`. A row is a file that entered the archive plus
//! everything anything has since read out of it. Why that is three events and
//! how fields fold: `docs/src/archive.md`.
//!
//! ⚠️ **Rank is compared before arrival order, never after** — see
//! [`DocumentField::rank`] and [`merge_fields`] for fields, [`TextSource::rank`]
//! and [`write_text_if_it_outranks`] for text. Reversing the two would make a
//! re-extraction silently overwrite every value a person corrected by hand, and
//! would let an archive event's absent text erase a transcription that reached
//! this device first. Neither would report anything.

use async_trait::async_trait;
use std::collections::BTreeMap;

use crate::archive::TextSource;
use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::{
    DOCUMENT_DATE_KEY, DOCUMENT_KIND_KEY, DOCUMENT_TAGS_KEY, DOCUMENT_TITLE_KEY,
    DocumentArchivedPayload, DocumentField, DocumentFieldsExtractedPayload, DocumentPurgedPayload,
    DocumentTextTranscribedPayload, Tag, decode_tag_set,
};

pub struct DocumentsProjection;

impl DocumentsProjection {
    pub const NAME: &'static str = "documents";
}

#[async_trait]
impl Projection for DocumentsProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn version(&self) -> u32 {
        // 2: `tags`, and the purge tombstone columns. One bump for both because a
        // bump costs minutes of blank UI on the phone while the rebuild runs.
        2
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS documents SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS document_id ON documents TYPE string;
             -- ⚠️ Every column an event writes is `option<>`, for the reason
             -- `beliefs` documents at length: the pull filter runs on the
             -- author's clock, so fields extracted on one device can be folded
             -- before the archive event that created the row. Required columns
             -- would fail that inbound event, and a document whose fields cannot
             -- land is one that never becomes findable.
             DEFINE FIELD IF NOT EXISTS sha256 ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS filename ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS mime_type ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS size ON documents TYPE option<number>;
             DEFINE FIELD IF NOT EXISTS archived_at ON documents TYPE option<datetime>;
             DEFINE FIELD IF NOT EXISTS ingest_source ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS text ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS text_source ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS device_id ON documents TYPE option<string>;
             -- The document this one arrived inside (an email, for its
             -- attachments). ⛔ A link, never ownership — see
             -- `DocumentArchivedPayload::parent_document_id`.
             DEFINE FIELD IF NOT EXISTS parent_document_id ON documents TYPE option<string>;
             -- Hoisted from `fields` so the archive list and date filters read
             -- them directly. ⛔ They are still ordinary folded keys — see
             -- `DocumentFieldsExtractedPayload::fields`.
             DEFINE FIELD IF NOT EXISTS kind ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS title ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS document_date ON documents TYPE option<string>;
             -- Hoisted from the `tags` key, which stores the whole set joined by
             -- `TAG_SEPARATOR`. An array here rather than that string so the
             -- filter is `CONTAINS` against a value, not a substring match that
             -- would make `receipt` find `receipts-2026`.
             DEFINE FIELD IF NOT EXISTS tags ON documents TYPE option<array>;
             DEFINE FIELD IF NOT EXISTS tags.* ON documents TYPE string;
             -- A document removed on purpose. ⛔ The row stays and reads as
             -- purged; the `document_archived` event is still in the log and a
             -- rebuild replays it, so without this the row would come back
             -- looking ordinary while its bytes were gone.
             DEFINE FIELD IF NOT EXISTS purged ON documents TYPE option<bool>;
             DEFINE FIELD IF NOT EXISTS purged_at ON documents TYPE option<datetime>;
             DEFINE FIELD IF NOT EXISTS purge_reason ON documents TYPE option<string>;
             -- A SCHEMAFULL table validates objects inside an array key by key,
             -- so every `DocumentField` column is spelled out. Same closed-struct
             -- argument as `beliefs.evidence`, and the same trap if FLEXIBLE is
             -- reached for instead.
             DEFINE FIELD IF NOT EXISTS fields ON documents TYPE option<array>;
             DEFINE FIELD IF NOT EXISTS fields.* ON documents TYPE object;
             DEFINE FIELD IF NOT EXISTS fields.*.key ON documents TYPE string;
             DEFINE FIELD IF NOT EXISTS fields.*.value ON documents TYPE string;
             DEFINE FIELD IF NOT EXISTS fields.*.source ON documents TYPE string;
             DEFINE FIELD IF NOT EXISTS fields.*.verified ON documents TYPE bool;
             DEFINE INDEX IF NOT EXISTS documents_sha256 ON documents FIELDS sha256;
             DEFINE INDEX IF NOT EXISTS documents_kind ON documents FIELDS kind;
             DEFINE INDEX IF NOT EXISTS documents_date ON documents FIELDS document_date;
             DEFINE INDEX IF NOT EXISTS documents_parent ON documents FIELDS parent_document_id;
             DEFINE INDEX IF NOT EXISTS documents_tags ON documents FIELDS tags;
             -- One index per field, as the catalog's `text_fields` requires.
             -- ⚠️ `filename` is indexed and the other two may be absent: a scan
             -- carries `text_source: none` and never gains a title, so its name
             -- is the only thing search can match. Drop it and that document is
             -- reachable only by knowing its id.
             DEFINE INDEX IF NOT EXISTS documents_filename_fts ON documents
                 FIELDS filename FULLTEXT ANALYZER omni_text BM25 HIGHLIGHTS;
             DEFINE INDEX IF NOT EXISTS documents_title_fts ON documents
                 FIELDS title FULLTEXT ANALYZER omni_text BM25 HIGHLIGHTS;
             DEFINE INDEX IF NOT EXISTS documents_text_fts ON documents
                 FIELDS text FULLTEXT ANALYZER omni_text BM25 HIGHLIGHTS;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query("DELETE FROM documents").await?.check()?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "document_archived" => self.on_archived(event, db).await,
            "document_fields_extracted" => self.on_fields(event, db).await,
            "document_text_transcribed" => self.on_transcribed(event, db).await,
            "document_purged" => self.on_purged(event, db).await,
            _ => Ok(()),
        }
    }
}

impl DocumentsProjection {
    async fn on_archived(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // Skip, never error — `ConfigProjection::on_set`'s rule. A document this
        // build cannot read was written by a different one, and failing the batch
        // would take the user's unrelated edits down with it.
        let parsed: DocumentArchivedPayload = match serde_json::from_value(event.payload.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    error = %e,
                    "skipping a document this build cannot read"
                );
                return Ok(());
            }
        };

        // Writes only its own columns, so fields that landed first are not undone
        // by the archive event arriving after them.
        db.query(
            "UPSERT type::record('documents', $id) SET
                document_id = $id,
                sha256 = $sha256,
                filename = $filename,
                mime_type = $mime_type,
                size = $size,
                archived_at = type::datetime($archived_at),
                ingest_source = $source,
                device_id = $device_id,
                parent_document_id = $parent_document_id",
        )
        .bind(("id", parsed.document_id.clone()))
        .bind(("sha256", parsed.sha256))
        .bind(("filename", parsed.filename))
        .bind(("mime_type", parsed.mime_type))
        .bind(("size", parsed.size))
        .bind(("archived_at", parsed.archived_at))
        .bind(("source", parsed.source))
        .bind(("device_id", event.device_id.clone()))
        .bind(("parent_document_id", parsed.parent_document_id))
        .await?
        .check()?;

        // ⚠️ Text is written through the same guard as a transcription, and it is
        // this handler — not the new one — that needed it. The pull filter runs
        // on the author's clock (see `init_schema`), so a transcription authored
        // on another device can fold *before* the archive event that created the
        // row. An unconditional `text = $text` here would then overwrite a real
        // reading with the `None` that made transcription necessary in the first
        // place, and nothing would report it.
        write_text_if_it_outranks(db, &parsed.document_id, parsed.text, &parsed.text_source).await
    }

    async fn on_transcribed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // Skip, never error — `on_archived`'s rule, for its reason.
        let parsed: DocumentTextTranscribedPayload =
            match serde_json::from_value(event.payload.clone()) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        event_id = %event.id,
                        error = %e,
                        "skipping a transcription this build cannot read"
                    );
                    return Ok(());
                }
            };

        // ⛔ `document_id` only. A transcription never creates the columns that
        // identify a file — no `sha256`, no `filename`. If it folds before the
        // archive event, the row it leaves behind is a text-only stub that
        // `on_archived` completes on arrival, which is exactly what the
        // `option<>` columns exist for.
        let source = match parsed.text_source.as_deref() {
            Some("extracted") => TextSource::Extracted,
            _ => TextSource::Transcribed,
        };
        write_text_if_it_outranks(db, &parsed.document_id, Some(parsed.text), source.as_str()).await
    }

    /// Tombstone a document: the row stays, marked purged, and its text goes.
    ///
    /// ⛔ **`UPSERT`, not `UPDATE`** — the lesson `routines_projection` paid for.
    /// A bare `UPDATE` silently matches nothing when the purge folds before the
    /// archive event it purges, and nothing ever retries a no-op'd mutation; the
    /// archive event would then arrive and materialize a fully visible row whose
    /// bytes had already been deleted. That is the one outcome this whole design
    /// exists to prevent.
    ///
    /// ⚠️ **`text` is cleared here, and that is the only place purging reclaims
    /// anything from the log.** Blob bytes live on disk and are deleted by the
    /// caller; a document's text lives in the *event payload*, which is
    /// append-only and stays. Nulling the column is what takes a purged spam
    /// email out of search and out of the embedding sweep — the log still holds
    /// the words, but nothing reads them back.
    async fn on_purged(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let parsed: DocumentPurgedPayload = match serde_json::from_value(event.payload.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    error = %e,
                    "skipping a purge this build cannot read"
                );
                return Ok(());
            }
        };

        db.query(
            "UPSERT type::record('documents', $id) SET
                document_id = $id,
                purged = true,
                purged_at = type::datetime($purged_at),
                purge_reason = $reason,
                text = NONE,
                text_source = $none",
        )
        .bind(("id", parsed.document_id.clone()))
        .bind(("purged_at", parsed.purged_at.clone()))
        .bind(("reason", parsed.reason.clone()))
        .bind(("none", TextSource::None.as_str().to_string()))
        .await?
        .check()?;

        tracing::info!(
            document_id = %parsed.document_id,
            reason = parsed.reason.as_deref().unwrap_or("none given"),
            "document purged"
        );
        Ok(())
    }

    async fn on_fields(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let parsed: DocumentFieldsExtractedPayload =
            match serde_json::from_value(event.payload.clone()) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        event_id = %event.id,
                        error = %e,
                        "skipping document fields this build cannot read"
                    );
                    return Ok(());
                }
            };

        // Read-modify-write rather than a single statement, because the merge is
        // a rank comparison per key and SurrealQL cannot express it. Safe without
        // a transaction: `ProjectionRunner` applies one event at a time and each
        // device owns its own database, so there is no second writer to race.
        // Read back as `serde_json::Value` and decode with serde, matching
        // `verbs::count_rows`. The driver's own typed `take` needs `SurrealValue`
        // on the struct, and `DocumentField` travels in an event payload — it is
        // serde's shape, and giving it a second serialization contract is how the
        // two drift.
        let existing: Vec<DocumentField> = db
            .query("SELECT VALUE fields FROM type::record('documents', $id)")
            .bind(("id", parsed.document_id.clone()))
            .await?
            .check()?
            .take::<Vec<serde_json::Value>>(0)
            .ok()
            .and_then(|rows| rows.into_iter().next())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let merged = merge_fields(existing, parsed.fields);
        // An empty value is a person removing the field: it outranks any re-read,
        // so it stays in `fields`, but the column reads as never set. Tags are
        // the exception, where empty means "every tag taken off".
        let hoisted = |key: &str| {
            merged
                .iter()
                .find(|f| f.key == key)
                .map(|f| f.value.clone())
                .filter(|v| key == DOCUMENT_TAGS_KEY || !v.is_empty())
        };

        let fields = serde_json::to_value(&merged)
            .map_err(|e| EventError::Validation(format!("could not serialize fields: {e}")))?;

        // ⚠️ `None`, not `Some(vec![])`, when the key is absent. An empty array
        // would make an untagged document indistinguishable from one whose tags
        // were cleared, and `docs/src/archive.md` records why an absent column
        // has to stay absent: `string::concat` renders it as the literal `NONE`
        // and the embedding sweep would index that word as the document's text.
        let tags: Option<Vec<String>> = hoisted(DOCUMENT_TAGS_KEY)
            .map(|joined| decode_tag_set(&joined).iter().map(Tag::to_string).collect());

        db.query(
            "UPSERT type::record('documents', $id) SET
                document_id = $id,
                fields = $fields,
                kind = $kind,
                title = $title,
                document_date = $document_date,
                tags = $tags",
        )
        .bind(("id", parsed.document_id.clone()))
        .bind(("fields", fields))
        .bind(("kind", hoisted(DOCUMENT_KIND_KEY)))
        .bind(("title", hoisted(DOCUMENT_TITLE_KEY)))
        .bind(("document_date", hoisted(DOCUMENT_DATE_KEY)))
        .bind(("tags", tags))
        .await?
        .check()?;
        Ok(())
    }
}

/// Fold `incoming` over `existing`, one key at a time, by origin rank.
///
/// ⚠️ The comparison is `>=`, not `>`. Equal ranks must let the newer value
/// through, or a second human correction of the same field would be discarded
/// and the UI would report a save that did nothing — the silent-success class
/// `ActionParam::verbatim` guards against on the note path.
fn merge_fields(existing: Vec<DocumentField>, incoming: Vec<DocumentField>) -> Vec<DocumentField> {
    let mut by_key: BTreeMap<String, DocumentField> =
        existing.into_iter().map(|f| (f.key.clone(), f)).collect();

    for field in incoming {
        let wins = by_key.get(&field.key).is_none_or(|held| {
            DocumentField::rank(&field.source) >= DocumentField::rank(&held.source)
        });
        if wins {
            by_key.insert(field.key.clone(), field);
        }
    }

    by_key.into_values().collect()
}

/// Write a document's text only when its provenance is at least as good as what
/// the row already holds.
///
/// ⚠️ `>=`, matching [`merge_fields`] and for the same reason: equal ranks let
/// the newer value through, so re-running transcription with a better model
/// lands rather than being silently discarded. What the ordering itself is —
/// and therefore what can shadow what — is [`TextSource::rank`].
///
/// Read-modify-write rather than one statement, because the comparison is a rank
/// lookup SurrealQL cannot express. Safe without a transaction for
/// [`DocumentsProjection::on_fields`]'s reason: `ProjectionRunner` applies one
/// event at a time and each device owns its own database.
async fn write_text_if_it_outranks(
    db: &Database,
    document_id: &str,
    text: Option<String>,
    text_source: &str,
) -> Result<(), EventError> {
    // ⛔ A purge outranks every text source there is, and this is the one place
    // that has to know it. Both writers of text come through here, and either
    // would otherwise restore a purged document's words: the archive event
    // carries `extracted`, a transcription carries `transcribed`, and both beat
    // the `none` a purge leaves behind. The pull filter runs on the authoring
    // device's clock, so either can arrive *after* the purge and undo it — on a
    // device that never held the bytes and has no way to notice.
    let purged: bool = db
        .query("SELECT VALUE purged FROM type::record('documents', $id)")
        .bind(("id", document_id.to_string()))
        .await?
        .check()?
        .take::<Vec<Option<bool>>>(0)
        .ok()
        .and_then(|rows| rows.into_iter().next().flatten())
        .unwrap_or(false);

    if purged {
        tracing::debug!(
            document_id,
            incoming = text_source,
            "not restoring text to a purged document"
        );
        return Ok(());
    }

    let held: String = db
        .query("SELECT VALUE text_source FROM type::record('documents', $id)")
        .bind(("id", document_id.to_string()))
        .await?
        .check()?
        .take::<Vec<Option<String>>>(0)
        .ok()
        .and_then(|rows| rows.into_iter().next().flatten())
        .unwrap_or_else(|| TextSource::None.as_str().to_string());

    if TextSource::rank(text_source) < TextSource::rank(&held) {
        tracing::debug!(
            document_id,
            incoming = text_source,
            held = %held,
            "keeping the better-sourced text already on the row"
        );
        return Ok(());
    }

    db.query(
        "UPSERT type::record('documents', $id) SET
            document_id = $id,
            text = $text,
            text_source = $text_source",
    )
    .bind(("id", document_id.to_string()))
    .bind(("text", text))
    .bind(("text_source", text_source.to_string()))
    .await?
    .check()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::types::{EventType, validate_payload};

    async fn test_db() -> Database {
        let db = crate::db::test_db().await;
        DocumentsProjection.init_schema(&db).await.unwrap();
        db
    }

    fn event(event_type: EventType, aggregate_id: &str, payload: serde_json::Value) -> Event {
        // Through the real validator, so a payload the write path would reject
        // cannot pass a projection test.
        validate_payload(&event_type, &payload).expect("payload must satisfy validate_payload");
        Event {
            id: ulid::Ulid::new().to_string(),
            event_type: event_type.to_string(),
            aggregate_id: aggregate_id.to_string(),
            timestamp: chrono::Utc::now(),
            device_id: "test-device".to_string(),
            payload,
            received_at: None,
        }
    }

    fn archived(document_id: &str) -> Event {
        event(
            EventType::DocumentArchived,
            document_id,
            serde_json::json!({
                "document_id": document_id,
                "sha256": "a".repeat(64),
                "filename": "notice-2023.pdf",
                "mime_type": "application/pdf",
                "size": 82_140u64,
                "archived_at": "2026-09-12T10:00:00Z",
                "source": "bulk",
                "text": "Notice of assessment for the 2023 tax year.",
                "text_source": "extracted",
            }),
        )
    }

    fn extracted(document_id: &str, fields: &[DocumentField]) -> Event {
        event(
            EventType::DocumentFieldsExtracted,
            document_id,
            serde_json::json!({
                "document_id": document_id,
                "extracted_at": "2026-09-12T10:05:00Z",
                "fields": fields,
            }),
        )
    }

    async fn column(db: &Database, id: &str, col: &str) -> Option<String> {
        let sql = format!("SELECT VALUE {col} FROM type::record('documents', $id)");
        db.query(&sql)
            .bind(("id", id.to_string()))
            .await
            .unwrap()
            .take::<Vec<serde_json::Value>>(0)
            .ok()
            .and_then(|rows| rows.into_iter().next())
            .and_then(|v| v.as_str().map(str::to_string))
    }

    #[tokio::test]
    async fn a_document_lands_with_its_text() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("doc-1"), &db)
            .await
            .unwrap();

        assert_eq!(
            column(&db, "doc-1", "filename").await.as_deref(),
            Some("notice-2023.pdf")
        );
        assert!(
            column(&db, "doc-1", "text")
                .await
                .unwrap()
                .contains("2023 tax year"),
            "the text layer must reach the row — it is the only copy a device \
             without the blob will ever have"
        );
    }

    #[tokio::test]
    async fn fields_arriving_before_the_archive_event_still_land() {
        // ⚠️ The case every `option<>` column exists for. The pull filter runs on
        // the author's clock, so an extraction done on one device can be folded
        // ahead of the archive event that created the row. Required columns would
        // reject it, and the document would never become findable.
        let db = test_db().await;

        DocumentsProjection
            .apply(
                &extracted(
                    "doc-2",
                    &[field("kind", "notice_of_assessment", "parser:cra")],
                ),
                &db,
            )
            .await
            .unwrap();
        DocumentsProjection
            .apply(&archived("doc-2"), &db)
            .await
            .unwrap();

        assert_eq!(
            column(&db, "doc-2", "kind").await.as_deref(),
            Some("notice_of_assessment"),
            "the archive event must not clobber fields that landed first"
        );
        assert_eq!(
            column(&db, "doc-2", "filename").await.as_deref(),
            Some("notice-2023.pdf")
        );
    }

    #[tokio::test]
    async fn a_human_correction_survives_a_later_model_pass_through_the_database() {
        // `merge_fields` is unit-tested; this proves the rule survives the round
        // trip through SurrealQL, where the existing fields are read back as JSON.
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("doc-3"), &db)
            .await
            .unwrap();

        for e in [
            extracted("doc-3", &[field("title", "Untitled scan", "model:qwen@2")]),
            extracted(
                "doc-3",
                &[field("title", "2023 Notice of Assessment", "human")],
            ),
            extracted("doc-3", &[field("title", "Untitled scan", "model:qwen@3")]),
        ] {
            DocumentsProjection.apply(&e, &db).await.unwrap();
        }

        assert_eq!(
            column(&db, "doc-3", "title").await.as_deref(),
            Some("2023 Notice of Assessment"),
            "a re-run must never undo what the user fixed by hand"
        );
    }

    #[tokio::test]
    async fn a_removed_field_reads_as_unset_and_a_rerun_cannot_restore_it() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("doc-r"), &db)
            .await
            .unwrap();
        for e in [
            extracted("doc-r", &[field("title", "Payslip", "model:qwen@2")]),
            extracted("doc-r", &[field("title", "", "human")]),
            extracted("doc-r", &[field("title", "Payslip", "model:qwen@3")]),
        ] {
            DocumentsProjection.apply(&e, &db).await.unwrap();
        }
        assert_eq!(column(&db, "doc-r", "title").await, None);
    }

    /// The `tags` column, read as elements rather than as a string.
    async fn tags_column(db: &Database, id: &str) -> Option<Vec<String>> {
        db.query("SELECT VALUE tags FROM type::record('documents', $id)")
            .bind(("id", id.to_string()))
            .await
            .unwrap()
            .take::<Vec<serde_json::Value>>(0)
            .ok()
            .and_then(|rows| rows.into_iter().next())
            .and_then(|v| serde_json::from_value(v).ok())
    }

    #[tokio::test]
    async fn the_tag_field_is_hoisted_into_elements() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("t1"), &db)
            .await
            .unwrap();
        DocumentsProjection
            .apply(
                &extracted(
                    "t1",
                    &[field("tags", "taxes,institution:rbc", "model:qwen@2")],
                ),
                &db,
            )
            .await
            .unwrap();

        assert_eq!(
            tags_column(&db, "t1").await,
            Some(vec!["taxes".to_string(), "institution:rbc".to_string()]),
            "the joined value must land as elements the filter can match whole"
        );
    }

    /// Tags fold by rank like any other key, which is the whole reason they are a
    /// field rather than a column of their own.
    #[tokio::test]
    async fn a_persons_tags_survive_a_later_model_pass() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("t2"), &db)
            .await
            .unwrap();

        for e in [
            extracted("t2", &[field("tags", "receipt", "model:qwen@2")]),
            extracted("t2", &[field("tags", "taxes,receipt", "human")]),
            extracted("t2", &[field("tags", "receipt", "model:qwen@3")]),
        ] {
            DocumentsProjection.apply(&e, &db).await.unwrap();
        }

        assert_eq!(
            tags_column(&db, "t2").await,
            Some(vec!["taxes".to_string(), "receipt".to_string()]),
            "a later model pass must not undo the user's own labels"
        );
    }

    /// ⚠️ Cleared is not the same state as never tagged, and the column has to
    /// keep them apart: an empty array means a person took the tags off, absent
    /// means nothing has ever tagged this document. Collapsing the two would make
    /// "I removed that tag" and "no tag was ever suggested" look identical.
    #[tokio::test]
    async fn clearing_tags_is_distinguishable_from_never_having_any() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("t3"), &db)
            .await
            .unwrap();
        assert_eq!(tags_column(&db, "t3").await, None, "never tagged");

        DocumentsProjection
            .apply(&extracted("t3", &[field("tags", "receipt", "human")]), &db)
            .await
            .unwrap();
        assert_eq!(tags_column(&db, "t3").await, Some(vec!["receipt".into()]));

        // The whole set, emptied — which is what removing the last chip sends.
        DocumentsProjection
            .apply(&extracted("t3", &[field("tags", "", "human")]), &db)
            .await
            .unwrap();
        assert_eq!(
            tags_column(&db, "t3").await,
            Some(vec![]),
            "cleared must be an empty array, never absent"
        );
    }

    /// The `>=` rule reaches tags too: a second edit by the same person lands
    /// rather than reporting a save that changed nothing.
    #[tokio::test]
    async fn a_second_human_tag_edit_lands() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("t4"), &db)
            .await
            .unwrap();

        for e in [
            extracted("t4", &[field("tags", "receipt", "human")]),
            extracted("t4", &[field("tags", "receipt,groceries", "human")]),
        ] {
            DocumentsProjection.apply(&e, &db).await.unwrap();
        }

        assert_eq!(
            tags_column(&db, "t4").await,
            Some(vec!["receipt".to_string(), "groceries".to_string()])
        );
    }

    fn purged(document_id: &str, reason: Option<&str>) -> Event {
        event(
            EventType::DocumentPurged,
            document_id,
            serde_json::json!({
                "document_id": document_id,
                "purged_at": "2026-09-27T10:00:00Z",
                "reason": reason,
            }),
        )
    }

    async fn flag(db: &Database, id: &str, col: &str) -> Option<bool> {
        let sql = format!("SELECT VALUE {col} FROM type::record('documents', $id)");
        db.query(&sql)
            .bind(("id", id.to_string()))
            .await
            .unwrap()
            .take::<Vec<Option<bool>>>(0)
            .ok()
            .and_then(|rows| rows.into_iter().next().flatten())
    }

    /// A purge tombstones the row and takes its text out of every read surface.
    ///
    /// ⚠️ The text lives in the *event payload* as well, and that stays — the log
    /// is append-only. Nulling the column is what removes a purged spam email from
    /// search and from the embedding sweep; the words survive where nothing reads
    /// them back.
    #[tokio::test]
    async fn a_purge_tombstones_the_row_and_clears_its_text() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("p1"), &db)
            .await
            .unwrap();
        assert!(
            column(&db, "p1", "text").await.is_some(),
            "text to begin with"
        );

        DocumentsProjection
            .apply(&purged("p1", Some("newsletter")), &db)
            .await
            .unwrap();

        assert_eq!(flag(&db, "p1", "purged").await, Some(true));
        assert_eq!(column(&db, "p1", "text").await, None, "text must be gone");
        assert_eq!(
            column(&db, "p1", "purge_reason").await.as_deref(),
            Some("newsletter"),
            "a purge is irreversible, so why it happened has to survive"
        );
        assert_eq!(
            column(&db, "p1", "text_source").await.as_deref(),
            Some("none")
        );
    }

    /// ⛔ The case the `UPSERT` exists for, and the one a bare `UPDATE` loses.
    ///
    /// The pull filter runs on the authoring device's clock, so a purge can reach
    /// a device before the archive event it purges. `routines_projection` paid for
    /// this lesson: a bare `UPDATE` matches nothing, nothing retries a no-op'd
    /// mutation, and the archive event then materializes a fully visible row whose
    /// bytes have already been deleted.
    #[tokio::test]
    async fn a_purge_that_arrives_before_its_archive_event_still_holds() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&purged("p2", None), &db)
            .await
            .unwrap();
        DocumentsProjection
            .apply(&archived("p2"), &db)
            .await
            .unwrap();

        assert_eq!(
            flag(&db, "p2", "purged").await,
            Some(true),
            "the archive event must not resurrect a purged document"
        );
        assert_eq!(
            column(&db, "p2", "text").await,
            None,
            "⛔ and it must not restore the text either — on a device with no \
             bytes, nothing would ever notice"
        );
    }

    /// The same guard from the other direction: a transcription is the second
    /// writer of text, and it arrives later by design.
    #[tokio::test]
    async fn a_transcription_cannot_give_text_back_to_a_purged_document() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("p3"), &db)
            .await
            .unwrap();
        DocumentsProjection
            .apply(&purged("p3", None), &db)
            .await
            .unwrap();

        let t = event(
            EventType::DocumentTextTranscribed,
            "p3",
            serde_json::json!({
                "document_id": "p3",
                "text": "a model read the scan",
                "model": "qwen@3",
                "transcribed_at": "2026-09-27T11:00:00Z",
            }),
        );
        DocumentsProjection.apply(&t, &db).await.unwrap();

        assert_eq!(column(&db, "p3", "text").await, None);
    }

    /// ⚠️ Fields are NOT blocked, deliberately. A purged row keeps its title and
    /// kind so the archive can say *which* document went; only the content goes.
    #[tokio::test]
    async fn a_purged_document_keeps_the_labels_that_say_what_it_was() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived("p4"), &db)
            .await
            .unwrap();
        DocumentsProjection
            .apply(
                &extracted("p4", &[field("title", "Weekly digest", "human")]),
                &db,
            )
            .await
            .unwrap();
        DocumentsProjection
            .apply(&purged("p4", None), &db)
            .await
            .unwrap();

        assert_eq!(
            column(&db, "p4", "title").await.as_deref(),
            Some("Weekly digest"),
            "a tombstone nobody can identify is not an account of anything"
        );
    }

    #[tokio::test]
    async fn an_unreadable_payload_is_skipped_rather_than_failing_the_batch() {
        // `ConfigProjection::on_set`'s rule: an event this build cannot read was
        // written by a different one, and failing here would take the user's
        // unrelated edits down with it.
        let db = test_db().await;
        let mut bad = archived("doc-4");
        bad.payload = serde_json::json!({ "document_id": "doc-4" });

        DocumentsProjection.apply(&bad, &db).await.unwrap();
        assert_eq!(column(&db, "doc-4", "filename").await, None);
    }

    fn field(key: &str, value: &str, source: &str) -> DocumentField {
        DocumentField {
            key: key.to_string(),
            value: value.to_string(),
            source: source.to_string(),
            verified: source.starts_with("parser:"),
        }
    }

    fn value_of(fields: &[DocumentField], key: &str) -> String {
        fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.value.clone())
            .unwrap_or_default()
    }

    #[test]
    fn a_model_rerun_never_overwrites_a_human_correction() {
        let held = vec![field("kind", "notice_of_assessment", "human")];
        let incoming = vec![field("kind", "bank_statement", "model:qwen@2")];

        let merged = merge_fields(held, incoming);

        assert_eq!(
            value_of(&merged, "kind"),
            "notice_of_assessment",
            "a later model pass must not undo what the user fixed by hand"
        );
    }

    #[test]
    fn a_parser_outranks_a_model_but_not_a_person() {
        let merged = merge_fields(
            vec![field("closing_balance", "100.00", "model:qwen@2")],
            vec![field("closing_balance", "142.37", "parser:rendered")],
        );
        assert_eq!(value_of(&merged, "closing_balance"), "142.37");

        let merged = merge_fields(
            vec![field("closing_balance", "142.37", "human")],
            vec![field("closing_balance", "100.00", "parser:rendered")],
        );
        assert_eq!(value_of(&merged, "closing_balance"), "142.37");
    }

    #[test]
    fn a_second_correction_of_the_same_field_replaces_the_first() {
        // The `>=` in `merge_fields`. With `>` this save would report success and
        // change nothing.
        let merged = merge_fields(
            vec![field("title", "first guess", "human")],
            vec![field("title", "what it actually says", "human")],
        );
        assert_eq!(value_of(&merged, "title"), "what it actually says");
    }

    #[test]
    fn keys_nobody_has_touched_survive_a_partial_emission() {
        // A correction sends one field. Everything else must stay put — otherwise
        // fixing a title would silently drop every value a parser had supplied.
        let merged = merge_fields(
            vec![
                field("kind", "bank_statement", "parser:rendered"),
                field("closing_balance", "142.37", "parser:rendered"),
            ],
            vec![field("title", "March statement", "human")],
        );

        assert_eq!(merged.len(), 3);
        assert_eq!(value_of(&merged, "closing_balance"), "142.37");
        assert_eq!(value_of(&merged, "kind"), "bank_statement");
    }

    #[test]
    fn an_unverified_parser_field_is_expressible() {
        // The chequing export carries no balance column, so its parse is clean
        // and unchecked. `verified` cannot be derived from `source` for that
        // reason — see `statement::Verifiability::NotVerifiable`.
        let merged = merge_fields(
            Vec::new(),
            vec![DocumentField {
                key: "closing_balance".into(),
                value: "142.37".into(),
                source: "parser:chequing_csv".into(),
                verified: false,
            }],
        );
        assert!(!merged[0].verified);
        assert_eq!(DocumentField::rank(&merged[0].source), 1);
    }

    /// A scan: archived successfully, but nothing deterministic could read it.
    fn archived_without_text(document_id: &str) -> Event {
        event(
            EventType::DocumentArchived,
            document_id,
            serde_json::json!({
                "document_id": document_id,
                "sha256": "b".repeat(64),
                "filename": "scan-0042.pdf",
                "mime_type": "application/pdf",
                "size": 1_204_880u64,
                "archived_at": "2026-09-12T10:00:00Z",
                "source": "scan",
                "text_source": "none",
            }),
        )
    }

    fn transcribed(document_id: &str, text: &str, model: &str) -> Event {
        event(
            EventType::DocumentTextTranscribed,
            document_id,
            serde_json::json!({
                "document_id": document_id,
                "text": text,
                "model": model,
                "transcribed_at": "2026-09-12T11:00:00Z",
            }),
        )
    }

    #[tokio::test]
    async fn a_transcription_gives_text_to_a_scan_that_had_none() {
        let db = test_db().await;

        DocumentsProjection
            .apply(&archived_without_text("scan-1"), &db)
            .await
            .unwrap();
        assert_eq!(
            column(&db, "scan-1", "text_source").await.as_deref(),
            Some("none"),
            "a scan with no text layer is archived text-less — an ordinary outcome"
        );

        DocumentsProjection
            .apply(
                &transcribed("scan-1", "NOTICE OF ASSESSMENT 2023", "vision@1"),
                &db,
            )
            .await
            .unwrap();

        assert_eq!(
            column(&db, "scan-1", "text").await.as_deref(),
            Some("NOTICE OF ASSESSMENT 2023")
        );
        assert_eq!(
            column(&db, "scan-1", "text_source").await.as_deref(),
            Some("transcribed"),
            "⛔ the provenance must change with the text — a reader who cannot \
             tell a model's reading from a text layer will trust both equally"
        );
        assert_eq!(
            column(&db, "scan-1", "filename").await.as_deref(),
            Some("scan-0042.pdf"),
            "a transcription writes text only; it must not disturb identity"
        );
    }

    #[tokio::test]
    async fn a_transcription_folded_before_its_archive_event_survives_it() {
        // ⚠️ The case the rank guard exists for, and it is `on_archived` that
        // needed it. The pull filter runs on the author's clock, so a phone's
        // transcription can fold ahead of the laptop's archive event. That event
        // carries `None` — the absence that made transcription necessary — and
        // writing it unconditionally would wipe the reading with nothing to
        // report it and no way back: blobs do not sync, so only the capturing
        // device could ever re-read the file.
        let db = test_db().await;

        DocumentsProjection
            .apply(
                &transcribed("scan-2", "NOTICE OF ASSESSMENT 2023", "vision@1"),
                &db,
            )
            .await
            .unwrap();
        DocumentsProjection
            .apply(&archived_without_text("scan-2"), &db)
            .await
            .unwrap();

        assert_eq!(
            column(&db, "scan-2", "text").await.as_deref(),
            Some("NOTICE OF ASSESSMENT 2023"),
            "the archive event's `None` must not outrank a transcription"
        );
        assert_eq!(
            column(&db, "scan-2", "text_source").await.as_deref(),
            Some("transcribed")
        );
        assert_eq!(
            column(&db, "scan-2", "filename").await.as_deref(),
            Some("scan-0042.pdf"),
            "the stub row a transcription leaves must still be completed by the \
             archive event when it lands"
        );
    }

    #[tokio::test]
    async fn a_transcription_never_shadows_a_real_text_layer() {
        let db = test_db().await;

        DocumentsProjection
            .apply(&archived("doc-9"), &db)
            .await
            .unwrap();
        DocumentsProjection
            .apply(
                &transcribed("doc-9", "a model's guess at the page", "vision@1"),
                &db,
            )
            .await
            .unwrap();

        assert!(
            column(&db, "doc-9", "text")
                .await
                .unwrap()
                .contains("2023 tax year"),
            "extracted text outranks transcribed — what the file states beats a \
             reading of it"
        );
        assert_eq!(
            column(&db, "doc-9", "text_source").await.as_deref(),
            Some("extracted")
        );
    }

    #[tokio::test]
    async fn a_better_model_replaces_an_earlier_transcription() {
        // Equal ranks let the newer value through, matching `merge_fields`. A
        // re-run that silently did nothing would be the worse failure.
        let db = test_db().await;

        DocumentsProjection
            .apply(&archived_without_text("scan-3"), &db)
            .await
            .unwrap();
        DocumentsProjection
            .apply(
                &transcribed("scan-3", "N0TICE 0F ASSESSMEN7", "vision@1"),
                &db,
            )
            .await
            .unwrap();
        DocumentsProjection
            .apply(
                &transcribed("scan-3", "NOTICE OF ASSESSMENT 2023", "vision@2"),
                &db,
            )
            .await
            .unwrap();

        assert_eq!(
            column(&db, "scan-3", "text").await.as_deref(),
            Some("NOTICE OF ASSESSMENT 2023")
        );
    }

    #[test]
    fn an_unknown_text_source_outranks_none() {
        // ⚠️ Asymmetric on purpose. A source a later build introduced must not be
        // erasable by an archive event carrying `None`, because that loss is
        // unrecoverable on any device that does not hold the blob.
        assert!(TextSource::rank("ocr-v2") > TextSource::rank(TextSource::None.as_str()));
        assert!(
            TextSource::rank(TextSource::Extracted.as_str()) > TextSource::rank("ocr-v2"),
            "a real text layer still wins"
        );
    }

    #[tokio::test]
    async fn an_unreadable_transcription_is_skipped_rather_than_failing_the_batch() {
        let db = test_db().await;
        DocumentsProjection
            .apply(&archived_without_text("scan-4"), &db)
            .await
            .unwrap();

        let mut bad = transcribed("scan-4", "unused", "vision@1");
        bad.payload = serde_json::json!({ "document_id": "scan-4" });

        DocumentsProjection.apply(&bad, &db).await.unwrap();
        assert_eq!(
            column(&db, "scan-4", "text_source").await.as_deref(),
            Some("none"),
            "a payload this build cannot read leaves the row untouched"
        );
    }
}
