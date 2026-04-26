use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};

/// Projection that maintains two read tables:
/// - `journal_entries` keyed by `date` (one per day), with `journal_id` for sync routing
/// - `generic_notes`   keyed by `note_id` (ULID), with user-supplied `title`
pub struct NotesProjection;

/// The three manual journal properties whose presence signals "day complete".
const COMPLETE_PROPERTIES: [&str; 3] = ["homework_for_life", "grateful_for", "learnt_today"];

#[async_trait]
impl Projection for NotesProjection {
    fn name(&self) -> &str {
        "notes"
    }

    fn version(&self) -> u32 {
        2
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS journal_entries SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS journal_id ON journal_entries TYPE string;
             DEFINE FIELD IF NOT EXISTS date ON journal_entries TYPE string;
             DEFINE FIELD IF NOT EXISTS raw_text ON journal_entries TYPE string;
             DEFINE FIELD IF NOT EXISTS tags ON journal_entries TYPE array;
             DEFINE FIELD IF NOT EXISTS tags.* ON journal_entries TYPE string;
             DEFINE FIELD IF NOT EXISTS summary ON journal_entries TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS closed ON journal_entries TYPE bool;
             DEFINE FIELD IF NOT EXISTS complete ON journal_entries TYPE bool;
             DEFINE FIELD IF NOT EXISTS legacy_properties ON journal_entries TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS created_at ON journal_entries TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON journal_entries TYPE datetime;
             DEFINE INDEX IF NOT EXISTS idx_journal_id ON journal_entries FIELDS journal_id UNIQUE;

             DEFINE TABLE IF NOT EXISTS generic_notes SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS title ON generic_notes TYPE string;
             DEFINE FIELD IF NOT EXISTS raw_text ON generic_notes TYPE string;
             DEFINE FIELD IF NOT EXISTS tags ON generic_notes TYPE array;
             DEFINE FIELD IF NOT EXISTS tags.* ON generic_notes TYPE string;
             DEFINE FIELD IF NOT EXISTS summary ON generic_notes TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS legacy_properties ON generic_notes TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS created_at ON generic_notes TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON generic_notes TYPE datetime;",
        )
        .await?;

        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DELETE FROM journal_entries;
             DELETE FROM generic_notes;",
        )
        .await?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "journal_entry_created" => self.on_journal_created(event, db).await,
            "journal_entry_updated" => self.on_journal_updated(event, db).await,
            "journal_entry_closed" => self.on_journal_closed(event, db, true).await,
            "journal_entry_reopened" => self.on_journal_closed(event, db, false).await,
            "generic_note_created" => self.on_generic_created(event, db).await,
            "generic_note_updated" => self.on_generic_updated(event, db).await,
            "generic_note_renamed" => self.on_generic_renamed(event, db).await,
            "note_llm_processed" => self.on_llm_processed(event, db).await,
            _ => Ok(()),
        }
    }
}

impl NotesProjection {
    async fn on_journal_created(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let journal_id = event.aggregate_id.clone();
        let date = event.payload["date"].as_str().unwrap_or_default().to_string();
        let raw_text = event.payload["raw_text"].as_str().unwrap_or_default().to_string();
        let legacy_properties = event.payload.get("legacy_properties").cloned();
        let complete = is_complete(&raw_text);
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "CREATE type::record('journal_entries', $date) CONTENT {
                journal_id: $journal_id,
                date: $date,
                raw_text: $raw_text,
                tags: [],
                summary: NONE,
                closed: false,
                complete: $complete,
                legacy_properties: $legacy_properties,
                created_at: type::datetime($ts),
                updated_at: type::datetime($ts)
            }",
        )
        .bind(("date", date))
        .bind(("journal_id", journal_id))
        .bind(("raw_text", raw_text))
        .bind(("complete", complete))
        .bind(("legacy_properties", legacy_properties))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_journal_updated(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let journal_id = event.aggregate_id.clone();
        let raw_text = event.payload["raw_text"].as_str().unwrap_or_default().to_string();
        let complete = is_complete(&raw_text);
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE journal_entries SET
                raw_text = $raw_text,
                complete = $complete,
                updated_at = type::datetime($ts)
             WHERE journal_id = $journal_id",
        )
        .bind(("journal_id", journal_id))
        .bind(("raw_text", raw_text))
        .bind(("complete", complete))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_journal_closed(
        &self,
        event: &Event,
        db: &Database,
        closed: bool,
    ) -> Result<(), EventError> {
        let journal_id = event.aggregate_id.clone();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE journal_entries SET
                closed = $closed,
                updated_at = type::datetime($ts)
             WHERE journal_id = $journal_id",
        )
        .bind(("journal_id", journal_id))
        .bind(("closed", closed))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_generic_created(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let note_id = event.aggregate_id.clone();
        let title = event.payload["title"].as_str().unwrap_or_default().to_string();
        let raw_text = event.payload["raw_text"].as_str().unwrap_or_default().to_string();
        let legacy_properties = event.payload.get("legacy_properties").cloned();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "CREATE type::record('generic_notes', $note_id) CONTENT {
                title: $title,
                raw_text: $raw_text,
                tags: [],
                summary: NONE,
                legacy_properties: $legacy_properties,
                created_at: type::datetime($ts),
                updated_at: type::datetime($ts)
            }",
        )
        .bind(("note_id", note_id))
        .bind(("title", title))
        .bind(("raw_text", raw_text))
        .bind(("legacy_properties", legacy_properties))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_generic_updated(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let note_id = event.aggregate_id.clone();
        let raw_text = event.payload["raw_text"].as_str().unwrap_or_default().to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('generic_notes', $note_id) SET
                raw_text = $raw_text,
                updated_at = type::datetime($ts)",
        )
        .bind(("note_id", note_id))
        .bind(("raw_text", raw_text))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_generic_renamed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let note_id = event.aggregate_id.clone();
        let title = event.payload["title"].as_str().unwrap_or_default().to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('generic_notes', $note_id) SET
                title = $title,
                updated_at = type::datetime($ts)",
        )
        .bind(("note_id", note_id))
        .bind(("title", title))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_llm_processed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let aggregate_id = event.payload["aggregate_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let derived = &event.payload["derived"];
        let tags: Vec<String> = derived["tags"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let summary = derived["summary"].as_str().map(String::from);
        let ts = event.timestamp.to_rfc3339();

        // aggregate_id matches journal_entries.journal_id OR generic_notes record id.
        // Update both — only one will match, the other no-ops.
        db.query(
            "UPDATE journal_entries SET
                tags = $tags,
                summary = $summary,
                updated_at = type::datetime($ts)
             WHERE journal_id = $aggregate_id;

             UPDATE type::record('generic_notes', $aggregate_id) SET
                tags = $tags,
                summary = $summary,
                updated_at = type::datetime($ts);",
        )
        .bind(("aggregate_id", aggregate_id))
        .bind(("tags", tags))
        .bind(("summary", summary))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }
}

/// A journal entry is "complete" when all three manual properties have
/// non-empty values in a YAML-style frontmatter block at the top of the note.
///
/// The parse is deliberately forgiving — any `key: value` line is accepted
/// regardless of whether it sits inside `---` fences, matching how users
/// actually type notes on a phone.
///
/// **Duplicate-key semantics:** YAML 1.2 says duplicate keys are undefined.
/// We use "any-non-empty wins" via `.any()` — if a property key appears more
/// than once and *any* occurrence has a non-empty value, the property counts
/// as filled. This differs from Python yaml / Obsidian (which are last-wins)
/// but is safe in practice because (1) duplicate keys essentially never
/// occur via normal user edits or property-panel UIs, and (2) the forgiving
/// rule favors "user typed it once, accidentally added a blank line later"
/// which is the realistic mistake mode.
fn is_complete(raw_text: &str) -> bool {
    // Single-pass scan over `&str` slices — no allocation. We track which of
    // the required properties have been seen with a non-empty value, and
    // short-circuit as soon as all three are satisfied. Stays cheap when this
    // runs on every keystroke-triggered auto-save.
    //
    // Termination is the same as the previous parser: `---` fences are
    // skipped; a blank line ends the scan only after at least one kv has
    // been consumed; a line without a colon ends the scan only after at
    // least one kv has been consumed.
    let mut found = [false; COMPLETE_PROPERTIES.len()];
    let mut seen_kv = false;

    for line in raw_text.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            continue;
        }
        if trimmed.is_empty() {
            if seen_kv {
                break;
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            if seen_kv {
                break;
            }
            continue;
        };
        seen_kv = true;

        let value = value.trim();
        if value.is_empty() {
            // Empty value can never satisfy a required property, but the line
            // still counts as kv for the termination heuristic above.
            continue;
        }
        let key = key.trim();
        for (i, required) in COMPLETE_PROPERTIES.iter().enumerate() {
            if !found[i] && key.eq_ignore_ascii_case(required) {
                found[i] = true;
                break; // a single key matches at most one required entry
            }
        }
        if found.iter().all(|&b| b) {
            return true;
        }
    }

    found.iter().all(|&b| b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::projection::ProjectionRunner;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use chrono::Utc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    #[test]
    fn is_complete_detects_all_three_properties() {
        let text = "---\nhomework_for_life: shipped event schema\ngrateful_for: coffee\nlearnt_today: surreal flexible types\n---\nBody goes here.";
        assert!(is_complete(text));
    }

    #[test]
    fn is_complete_false_when_any_property_empty() {
        let text = "homework_for_life: shipped\ngrateful_for:\nlearnt_today: things";
        assert!(!is_complete(text));
    }

    #[test]
    fn is_complete_false_when_property_missing() {
        let text = "homework_for_life: shipped\nlearnt_today: things";
        assert!(!is_complete(text));
    }

    #[test]
    fn is_complete_accepts_no_fences() {
        // Common mobile-entry shape: no leading ---
        let text = "homework_for_life: a\ngrateful_for: b\nlearnt_today: c\n\nbody";
        assert!(is_complete(text));
    }

    #[test]
    fn is_complete_recognizes_journal_template_when_filled() {
        // Regression for Cycle 2 Critical #2 (2026-04-24 logical-inconsistencies review):
        // the journal template (frontend/src/journal_template.rs) renders the
        // frontmatter that *this* parser must accept once the user fills in the
        // three reflection properties. If the template adds a YAML block-list
        // line (e.g. `tags:\n  - daily_note`), this parser silently terminates
        // before the properties are scanned, breaking auto-close.
        // Lock the contract: the parser must accept a filled-in template render.
        // If you change journal_template::render, update this fixture too.
        let filled = "---\n\
            date: 2026-04-25\n\
            tags: [daily_note]\n\
            homework_for_life: notice when assumptions diverge across crates\n\
            grateful_for: regression tests that lock invisible contracts\n\
            learnt_today: chrono encodes the entire calendar ruleset\n\
            ---\n\
            \n\
            ## What happened today? (Add as much detail as you want)\n\
            \n";
        assert!(
            is_complete(filled),
            "filled-in journal template must register as complete"
        );
    }

    #[tokio::test]
    async fn journal_created_projects_by_date() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let event = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: "01JKJ0000000000000000000AA".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": "01JKJ0000000000000000000AA",
                    "date": "2026-04-19",
                    "raw_text": "Opening entry."
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[event]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let raw_text: Option<String> = resp.take("raw_text").unwrap();
        assert_eq!(raw_text.as_deref(), Some("Opening entry."));
        let complete: Option<bool> = resp.take("complete").unwrap();
        assert_eq!(complete, Some(false));
    }

    #[tokio::test]
    async fn journal_updated_recomputes_complete() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: "01JKJ1111111111111111111AA".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": "01JKJ1111111111111111111AA",
                    "date": "2026-04-19",
                    "raw_text": "empty"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_updated".into(),
                aggregate_id: "01JKJ1111111111111111111AA".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": "01JKJ1111111111111111111AA",
                    "raw_text": "homework_for_life: a\ngrateful_for: b\nlearnt_today: c"
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let complete: Option<bool> = resp.take("complete").unwrap();
        assert_eq!(complete, Some(true), "complete flips to true once 3 properties are filled");
    }

    #[tokio::test]
    async fn journal_closed_then_reopened() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let jid = "01JKJCLOSE00000000000000AA";
        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": jid,
                    "date": "2026-04-19",
                    "raw_text": "x"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_closed".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({ "journal_id": jid, "trigger": "manual" }),
            })
            .await
            .unwrap();

        let e3 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_reopened".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({ "journal_id": jid }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2, e3]).await.unwrap();

        let mut resp = db
            .query("SELECT closed FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let closed: Option<bool> = resp.take("closed").unwrap();
        assert_eq!(closed, Some(false));
    }

    #[tokio::test]
    async fn generic_note_lifecycle() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let nid = "01JKNOTE00000000000000000A";
        let events = [
            ("generic_note_created", serde_json::json!({
                "note_id": nid, "title": "Ideas", "raw_text": "first"
            })),
            ("generic_note_updated", serde_json::json!({
                "note_id": nid, "raw_text": "second"
            })),
            ("generic_note_renamed", serde_json::json!({
                "note_id": nid, "title": "Renamed"
            })),
        ];

        for (et, payload) in events {
            let e = store
                .append(NewEvent {
                    id: None,
                    event_type: et.into(),
                    aggregate_id: nid.into(),
                    timestamp: Utc::now(),
                    device_id: "d1".into(),
                    payload,
                })
                .await
                .unwrap();
            runner.apply_events(&[e]).await.unwrap();
        }

        let mut resp = db
            .query("SELECT * FROM type::record('generic_notes', '01JKNOTE00000000000000000A')")
            .await
            .unwrap();
        let title: Option<String> = resp.take("title").unwrap();
        assert_eq!(title.as_deref(), Some("Renamed"));
        let raw: Option<String> = resp.take("raw_text").unwrap();
        assert_eq!(raw.as_deref(), Some("second"));
    }

    #[tokio::test]
    async fn llm_processed_routes_to_journal_by_journal_id() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let jid = "01JKJLLM000000000000000000";
        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": jid, "date": "2026-04-19", "raw_text": "body"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "note_llm_processed".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "aggregate_id": jid,
                    "prompt_version": "v1",
                    "model": "gemini-flash",
                    "derived": { "tags": ["focus"], "summary": "productive day" }
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT summary FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let summary: Option<String> = resp.take("summary").unwrap();
        assert_eq!(summary.as_deref(), Some("productive day"));
    }
}
