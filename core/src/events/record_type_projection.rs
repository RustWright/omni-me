//! Materializes `RecordTypeDeclared` into the `record_types` table.
//!
//! Modelled on `config_projection` down to the ordering guard, and for the same
//! reason: a declaration nobody revisits never self-corrects the way a note does.
//! Unlike config, it is read at **point of use** rather than once at startup, so a
//! shape changed on another device applies as soon as it syncs.

use async_trait::async_trait;

use crate::db::Database;
use crate::record_type::RecordType;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::RecordTypeDeclaredPayload;

pub struct RecordTypeProjection;

impl RecordTypeProjection {
    pub const NAME: &'static str = "record_types";
}

#[async_trait]
impl Projection for RecordTypeProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        // Called twice on a normal boot, like `app_config` — once from the startup
        // read that needs a table to select from, once from `init_all`.
        db.query(
            "DEFINE TABLE IF NOT EXISTS record_types SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS name ON record_types TYPE string;
             DEFINE FIELD IF NOT EXISTS declaration ON record_types TYPE object FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS updated_at ON record_types TYPE datetime;",
        )
        .await?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query("DELETE FROM record_types").await?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "record_type_declared" => self.on_declared(event, db).await,
            _ => Ok(()),
        }
    }
}

impl RecordTypeProjection {
    async fn on_declared(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // Skip, never error — `config_projection::on_set`'s reasoning applies
        // unchanged. A declaration this build cannot read came from a different
        // build, and failing the batch would take the user's unrelated edits down
        // with it to protect a shape.
        let parsed: RecordTypeDeclaredPayload = match serde_json::from_value(event.payload.clone())
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    aggregate_id = %event.aggregate_id,
                    error = %e,
                    "skipping a record type declaration this build cannot read"
                );
                return Ok(());
            }
        };
        if let Err(e) = parsed.record_type.validate() {
            tracing::warn!(name = %parsed.record_type.name, error = %e, "skipping an invalid record type declaration");
            return Ok(());
        }

        let name = parsed.record_type.name.clone();

        // Authoring time, not arrival time — see `config_projection::on_set`.
        let mut existing = db
            .query("SELECT <string> updated_at AS updated_at FROM type::record('record_types', $name) LIMIT 1")
            .bind(("name", name.clone()))
            .await?;
        let stored: Option<String> = existing.take("updated_at").unwrap_or(None);
        if let Some(stored) = stored
            && let Ok(stored_ts) = chrono::DateTime::parse_from_rfc3339(&stored)
            && stored_ts.with_timezone(&chrono::Utc) >= event.timestamp
        {
            return Ok(());
        }

        let declaration = serde_json::to_value(&parsed.record_type)
            .map_err(|e| EventError::Validation(format!("could not serialize record type: {e}")))?;

        db.query(
            "UPSERT type::record('record_types', $name) SET
                name = $name,
                declaration = $declaration,
                updated_at = type::datetime($ts)",
        )
        .bind(("name", name))
        .bind(("declaration", declaration))
        .bind(("ts", event.timestamp.to_rfc3339()))
        .await?;

        Ok(())
    }
}

/// Read one record type's current declaration, or `None` if it has never been
/// declared.
///
/// Callers that need a shape no matter what should use [`journal_record_type`],
/// which carries the migration fallback.
pub async fn load_record_type(db: &Database, name: &str) -> Result<Option<RecordType>, EventError> {
    RecordTypeProjection.init_schema(db).await?;

    let mut resp = db
        .query("SELECT declaration FROM type::record('record_types', $name) LIMIT 1")
        .bind(("name", name.to_string()))
        .await?;
    let rows: Vec<Option<serde_json::Value>> = resp.take("declaration").unwrap_or_default();
    let Some(Some(value)) = rows.into_iter().next() else {
        return Ok(None);
    };

    match serde_json::from_value::<RecordType>(value) {
        Ok(decl) => Ok(Some(decl)),
        Err(e) => {
            tracing::warn!(name, error = %e, "ignoring a record type row this build cannot read");
            Ok(None)
        }
    }
}

/// The journal's record type, with the fallback an undeclared log needs.
///
/// ⚠️ **The fallback is the reflective preset, and that is load-bearing rather than
/// a default worth tidying.** Every install that predates record types has a log
/// full of journal entries and no declaration in it, and the first thing that
/// happens on upgrade is a replay. Falling back to the minimal preset would find
/// nothing required, mark the entire back catalogue incomplete, and quietly undo a
/// year of auto-closes. Falling back to the shape those entries were written under
/// keeps `complete` identical, which is exactly what the migration test asserts.
/// The seed written straight after startup then makes the declaration explicit.
pub async fn journal_record_type(db: &Database) -> RecordType {
    match load_record_type(db, crate::record_type::JOURNAL).await {
        Ok(Some(decl)) => decl,
        Ok(None) => RecordType::journal_reflective(),
        Err(e) => {
            // A working app on the pre-record-type shape beats refusing to project.
            tracing::warn!(error = %e, "could not read the journal record type; using the pre-record-type shape");
            RecordType::journal_reflective()
        }
    }
}

/// Declare the journal's shape once, on the first launch that finds none declared.
///
/// Which preset depends on what the log already holds, and the distinction is the
/// whole migration:
/// - **An install that predates record types** has journal entries written under the
///   three reflection prompts. It gets the reflective preset, so the declaration
///   matches what its entries already are and nothing is reinterpreted.
/// - **A fresh install** has no entries and no expectations, so it gets the minimal
///   preset: a date, a body, and no prompts. The reflective preset stays available
///   as a worked example rather than being imposed as the app's opinion.
///
/// Idempotent by the "already declared" check, so it is safe on every launch. Two
/// devices upgrading at once both emit; they emit the same shape, and the
/// projection's authoring-timestamp guard settles which row survives.
///
/// Returns what it declared, or `None` when a declaration already existed.
pub async fn seed_journal_record_type<S: super::store::EventStore + ?Sized>(
    db: &Database,
    store: &S,
    projections: &super::projection::ProjectionRunner,
    device_id: &str,
) -> Result<Option<RecordType>, EventError> {
    if load_record_type(db, crate::record_type::JOURNAL)
        .await?
        .is_some()
    {
        return Ok(None);
    }

    let mut resp = db
        .query("SELECT id FROM events WHERE event_type = 'journal_entry_created' LIMIT 1")
        .await?;
    let existing: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
    let preset = if existing.is_empty() {
        RecordType::journal_minimal()
    } else {
        RecordType::journal_reflective()
    };

    tracing::info!(
        preset = %preset.name,
        properties = preset.properties.len(),
        upgrade = !existing.is_empty(),
        "declaring the journal record type for the first time"
    );

    let event = store
        .append(
            super::store::NewEvent::record_type_declared(device_id, preset.clone()).map_err(
                |e| EventError::Validation(format!("could not build the declaration event: {e}")),
            )?,
        )
        .await?;
    projections.apply_events(&[event]).await?;

    Ok(Some(preset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::notes_projection::NotesProjection;
    use crate::events::projection::ProjectionRunner;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use crate::record_type::{PropertyDecl, RecordType};
    use chrono::Utc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    /// Every journal frontmatter shape the pre-record-type scanner was tested
    /// against, one per date. Deliberately the awkward ones: this corpus is what
    /// gives the migration assertion below its teeth.
    fn journal_corpus() -> Vec<(&'static str, &'static str)> {
        vec![
            (
                "2026-04-01",
                "---\ndate: 2026-04-01\ntags: [daily_note]\nhomework_for_life: a\ngrateful_for: b\nlearnt_today: c\n---\n\nbody",
            ),
            // Fence-less, the mobile-entry shape.
            (
                "2026-04-02",
                "homework_for_life: a\ngrateful_for: b\nlearnt_today: c\n\nbody",
            ),
            // Block-list tags — the shape that once terminated the scan early.
            (
                "2026-04-03",
                "---\ntags:\n  - daily_note\n  - reflections\nhomework_for_life: a\ngrateful_for: b\nlearnt_today: c\n---\nbody",
            ),
            // Reordered keys.
            (
                "2026-04-04",
                "---\nlearnt_today: c\ngrateful_for: b\ndate: 2026-04-04\nhomework_for_life: a\n---\nbody",
            ),
            // Blank lines inside the fence.
            (
                "2026-04-05",
                "---\nhomework_for_life: a\n\ngrateful_for: b\n\nlearnt_today: c\n---\nbody",
            ),
            // Incomplete: one property empty.
            (
                "2026-04-06",
                "---\nhomework_for_life: a\ngrateful_for:\nlearnt_today: c\n---\nbody",
            ),
            // Incomplete: one property absent entirely.
            (
                "2026-04-07",
                "---\nhomework_for_life: a\ngrateful_for: b\n---\nbody",
            ),
            // Body prose containing a colon — must not read as frontmatter.
            (
                "2026-04-08",
                "Meeting notes: discussed the roadmap\nMore body.",
            ),
            // Legacy Obsidian frontmatter alongside the reflections.
            (
                "2026-04-09",
                "---\naliases: [Apr 9]\nmood: 7\nhomework_for_life: a\ngrateful_for: b\nlearnt_today: c\n---\nbody",
            ),
        ]
    }

    fn journal_event(date: &str, raw_text: &str) -> NewEvent {
        NewEvent {
            id: None,
            event_type: "journal_entry_created".into(),
            aggregate_id: date.into(),
            timestamp: Utc::now(),
            device_id: "d1".into(),
            payload: serde_json::json!({
                "journal_id": date,
                "date": date,
                "raw_text": raw_text,
            }),
        }
    }

    /// Replay the corpus, optionally preceded by a declaration, and report
    /// `complete` per date in corpus order.
    async fn replay_completeness(declare: Option<RecordType>) -> Vec<(String, bool)> {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(RecordTypeProjection), Box::new(NotesProjection)],
        );
        runner.init_all().await.unwrap();

        let mut events = Vec::new();
        if let Some(decl) = declare {
            events.push(
                store
                    .append(NewEvent::record_type_declared("d1", decl).unwrap())
                    .await
                    .unwrap(),
            );
        }
        for (date, raw) in journal_corpus() {
            events.push(store.append(journal_event(date, raw)).await.unwrap());
        }
        runner.apply_events(&events).await.unwrap();

        let mut out = Vec::new();
        for (date, _) in journal_corpus() {
            let mut resp = db
                .query("SELECT complete FROM type::record('journal_entries', $date)")
                .bind(("date", date.to_string()))
                .await
                .unwrap();
            let complete: Option<bool> = resp.take("complete").unwrap();
            out.push((date.to_string(), complete.unwrap()));
        }
        out
    }

    /// ⚠️ **The migration gate.** The one test that had to exist before any of the
    /// record-type work merged.
    ///
    /// Replays a corpus of real journal frontmatter shapes twice: once with no
    /// declaration in the log at all (an install that predates record types, which
    /// falls through to the reflective shape) and once with the reflective preset
    /// declared ahead of the entries. `complete` must be identical for every entry.
    /// A year of existing entries keep the answers they already had, or auto-close
    /// history silently changes underneath the user.
    #[tokio::test]
    async fn declaring_the_reflective_preset_changes_no_existing_entry() {
        let before = replay_completeness(None).await;
        let after = replay_completeness(Some(RecordType::journal_reflective())).await;

        assert_eq!(
            before, after,
            "declaring the shipped default must not reinterpret a single existing entry"
        );
        // Guard the guard: a corpus where nothing is complete would pass vacuously.
        assert!(
            before.iter().any(|(_, c)| *c) && before.iter().any(|(_, c)| !*c),
            "the corpus must contain both complete and incomplete entries"
        );
    }

    #[tokio::test]
    async fn the_minimal_preset_completes_nothing() {
        // End-to-end cover for the vacuous-truth trap: with nothing required, an
        // entry whose reflections are all filled must still not read as complete,
        // or auto-close would sweep the entire back catalogue read-only.
        let with_minimal = replay_completeness(Some(RecordType::journal_minimal())).await;
        assert!(
            with_minimal.iter().all(|(_, complete)| !*complete),
            "nothing required means nothing finished: {with_minimal:?}"
        );
    }

    #[tokio::test]
    async fn a_declaration_applies_to_later_events_and_not_earlier_ones() {
        // Replay ordering is the whole reason `RecordTypeProjection` registers ahead
        // of `NotesProjection`. An entry written before a shape change keeps the
        // answer its own shape gave it until something re-applies it.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(RecordTypeProjection), Box::new(NotesProjection)],
        );
        runner.init_all().await.unwrap();

        let raw = "---\nmood: 7\n---\nbody";
        let mut events = vec![
            store
                .append(journal_event("2026-05-01", raw))
                .await
                .unwrap(),
        ];

        // Now declare `mood` required, and write the same shape on a later date.
        let mood_type = RecordType {
            properties: vec![PropertyDecl::required_text("mood", "Mood")],
            ..RecordType::journal_minimal()
        };
        events.push(
            store
                .append(NewEvent::record_type_declared("d1", mood_type).unwrap())
                .await
                .unwrap(),
        );
        events.push(
            store
                .append(journal_event("2026-05-02", raw))
                .await
                .unwrap(),
        );
        runner.apply_events(&events).await.unwrap();

        let complete_on = |date: &'static str| {
            let db = db.clone();
            async move {
                let mut resp = db
                    .query("SELECT complete FROM type::record('journal_entries', $date)")
                    .bind(("date", date.to_string()))
                    .await
                    .unwrap();
                let complete: Option<bool> = resp.take("complete").unwrap();
                complete.unwrap()
            }
        };

        assert!(
            !complete_on("2026-05-01").await,
            "an entry applied before the declaration is judged by the shape then in force"
        );
        assert!(
            complete_on("2026-05-02").await,
            "an entry applied after the declaration is judged by the new shape"
        );
    }

    #[tokio::test]
    async fn an_older_declaration_does_not_overwrite_a_newer_one() {
        // Sync is last-write-wins and events apply in arrival order, which differs
        // per device. A shape nobody revisits would otherwise sit differently on two
        // devices with nothing left to reconcile them — `config_projection`'s
        // reasoning, and it applies here for the same reason.
        let db = test_db().await;
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RecordTypeProjection)]);
        runner.init_all().await.unwrap();
        let store = SurrealEventStore::new(db.clone());

        let mut newer =
            NewEvent::record_type_declared("d1", RecordType::journal_reflective()).unwrap();
        newer.timestamp = Utc::now();
        let mut older =
            NewEvent::record_type_declared("d2", RecordType::journal_minimal()).unwrap();
        older.timestamp = newer.timestamp - chrono::Duration::minutes(5);

        // Append the newer one first, so the older arrives second.
        let e1 = store.append(newer).await.unwrap();
        let e2 = store.append(older).await.unwrap();
        runner.apply_events(&[e1, e2]).await.unwrap();

        let loaded = load_record_type(&db, crate::record_type::JOURNAL)
            .await
            .unwrap()
            .expect("a declaration must be materialized");
        assert_eq!(
            loaded.required_keys().len(),
            3,
            "the later-authored declaration wins regardless of arrival order"
        );
    }

    #[tokio::test]
    async fn seeding_a_fresh_log_picks_the_minimal_preset() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(RecordTypeProjection), Box::new(NotesProjection)],
        );
        runner.init_all().await.unwrap();

        let seeded = seed_journal_record_type(&db, &store, &runner, "d1")
            .await
            .unwrap()
            .expect("a fresh log must be seeded");
        assert_eq!(seeded, RecordType::journal_minimal());
        assert_eq!(
            journal_record_type(&db).await,
            RecordType::journal_minimal(),
            "the seed must be readable through the projection immediately"
        );
    }

    #[tokio::test]
    async fn seeding_an_existing_log_keeps_the_shape_its_entries_were_written_under() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(RecordTypeProjection), Box::new(NotesProjection)],
        );
        runner.init_all().await.unwrap();

        // An install that predates record types: entries in the log, no declaration.
        let raw = "---\nhomework_for_life: a\ngrateful_for: b\nlearnt_today: c\n---\nbody";
        let e = store
            .append(journal_event("2026-04-01", raw))
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();

        let seeded = seed_journal_record_type(&db, &store, &runner, "d1")
            .await
            .unwrap()
            .expect("an upgrading log must be seeded");
        assert_eq!(
            seeded,
            RecordType::journal_reflective(),
            "an upgrade must inherit the shape its entries already have, not the minimal preset"
        );
    }

    #[tokio::test]
    async fn seeding_is_idempotent() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(RecordTypeProjection)]);
        runner.init_all().await.unwrap();

        assert!(
            seed_journal_record_type(&db, &store, &runner, "d1")
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            seed_journal_record_type(&db, &store, &runner, "d1")
                .await
                .unwrap()
                .is_none(),
            "a second launch must not append a second declaration"
        );
    }

    #[tokio::test]
    async fn an_undeclared_log_reads_as_the_pre_record_type_shape() {
        let db = test_db().await;
        RecordTypeProjection.init_schema(&db).await.unwrap();
        assert!(
            load_record_type(&db, crate::record_type::JOURNAL)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            journal_record_type(&db).await,
            RecordType::journal_reflective(),
            "the fallback is what keeps an upgrading install's entries validating"
        );
    }
}
