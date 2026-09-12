//! SurrealDB projection over the document archive.
//!
//! One table, `documents`. A row is a file that entered the archive plus
//! everything anything has since read out of it. Why that is two events and how
//! fields fold: `docs/src/archive.md`.
//!
//! ⚠️ **Rank is compared before arrival order, never after** — see
//! [`DocumentField::rank`] and [`merge_fields`]. Reversing the two would make a
//! re-extraction silently overwrite every value a person corrected by hand, and
//! nothing would report it.

use async_trait::async_trait;
use std::collections::BTreeMap;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::{
    DOCUMENT_DATE_KEY, DOCUMENT_KIND_KEY, DOCUMENT_TITLE_KEY, DocumentArchivedPayload,
    DocumentField, DocumentFieldsExtractedPayload,
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
        1
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
             -- Hoisted from `fields` so the archive list and date filters read
             -- them directly. ⛔ They are still ordinary folded keys — see
             -- `DocumentFieldsExtractedPayload::fields`.
             DEFINE FIELD IF NOT EXISTS kind ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS title ON documents TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS document_date ON documents TYPE option<string>;
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
             DEFINE INDEX IF NOT EXISTS documents_date ON documents FIELDS document_date;",
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
                text = $text,
                text_source = $text_source,
                device_id = $device_id",
        )
        .bind(("id", parsed.document_id))
        .bind(("sha256", parsed.sha256))
        .bind(("filename", parsed.filename))
        .bind(("mime_type", parsed.mime_type))
        .bind(("size", parsed.size))
        .bind(("archived_at", parsed.archived_at))
        .bind(("source", parsed.source))
        .bind(("text", parsed.text))
        .bind(("text_source", parsed.text_source))
        .bind(("device_id", event.device_id.clone()))
        .await?
        .check()?;
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
        let hoisted = |key: &str| {
            merged
                .iter()
                .find(|f| f.key == key)
                .map(|f| f.value.clone())
        };

        let fields = serde_json::to_value(&merged)
            .map_err(|e| EventError::Validation(format!("could not serialize fields: {e}")))?;

        db.query(
            "UPSERT type::record('documents', $id) SET
                document_id = $id,
                fields = $fields,
                kind = $kind,
                title = $title,
                document_date = $document_date",
        )
        .bind(("id", parsed.document_id.clone()))
        .bind(("fields", fields))
        .bind(("kind", hoisted(DOCUMENT_KIND_KEY)))
        .bind(("title", hoisted(DOCUMENT_TITLE_KEY)))
        .bind(("document_date", hoisted(DOCUMENT_DATE_KEY)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::types::{EventType, validate_payload};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("documents.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
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
}
