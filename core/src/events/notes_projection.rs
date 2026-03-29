use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};

/// Projection that maintains the `notes` read table from note events.
pub struct NotesProjection;

#[async_trait]
impl Projection for NotesProjection {
    fn name(&self) -> &str {
        "notes"
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS notes SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS raw_text ON notes TYPE string;
             DEFINE FIELD IF NOT EXISTS date ON notes TYPE string;
             DEFINE FIELD IF NOT EXISTS tags ON notes TYPE array;
             DEFINE FIELD IF NOT EXISTS tags.* ON notes TYPE string;
             DEFINE FIELD IF NOT EXISTS summary ON notes TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS mood ON notes TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS created_at ON notes TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON notes TYPE datetime;",
        )
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "note_created" => self.on_note_created(event, db).await,
            "note_updated" => self.on_note_updated(event, db).await,
            "note_llm_processed" => self.on_note_llm_processed(event, db).await,
            _ => Ok(()), // Ignore events this projection doesn't care about
        }
    }
}

impl NotesProjection {
    async fn on_note_created(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let raw_text = event.payload["raw_text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let date = event.payload["date"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let note_id = event.aggregate_id.clone();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "CREATE type::record('notes', $note_id) CONTENT {
                raw_text: $raw_text,
                date: $date,
                tags: [],
                summary: NONE,
                mood: NONE,
                created_at: type::datetime($ts),
                updated_at: type::datetime($ts)
            }",
        )
        .bind(("note_id", note_id))
        .bind(("raw_text", raw_text))
        .bind(("date", date))
        .bind(("ts", ts))
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }

    async fn on_note_updated(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let raw_text = event.payload["raw_text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let note_id = event.aggregate_id.clone();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('notes', $note_id) SET
                raw_text = $raw_text,
                updated_at = type::datetime($ts)",
        )
        .bind(("note_id", note_id))
        .bind(("raw_text", raw_text))
        .bind(("ts", ts))
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }

    async fn on_note_llm_processed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
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
        let mood = derived["mood"].as_str().map(String::from);

        // Use the note_id from the payload (the aggregate this LLM processing is for)
        let note_id = event.payload["note_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('notes', $note_id) SET
                tags = $tags,
                summary = $summary,
                mood = $mood,
                updated_at = type::datetime($ts)",
        )
        .bind(("note_id", note_id))
        .bind(("tags", tags))
        .bind(("summary", summary))
        .bind(("mood", mood))
        .bind(("ts", ts))
        .await
        .map_err(|e| EventError::Projection(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use crate::events::projection::ProjectionRunner;
    use chrono::Utc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    #[tokio::test]
    async fn note_created_projects_to_read_table() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());

        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let event = store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "note-abc".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "raw_text": "Today I learned Rust.",
                    "date": "2026-03-27"
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[event]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('notes', 'note-abc')")
            .await
            .unwrap();
        let raw_text: Option<String> = resp.take("raw_text").unwrap();
        assert_eq!(raw_text.as_deref(), Some("Today I learned Rust."));
    }

    #[tokio::test]
    async fn note_updated_changes_text() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());

        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "note-upd".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "raw_text": "Original text",
                    "date": "2026-03-27"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "note_updated".into(),
                aggregate_id: "note-upd".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "note_id": "note-upd",
                    "raw_text": "Updated text"
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('notes', 'note-upd')")
            .await
            .unwrap();
        let raw_text: Option<String> = resp.take("raw_text").unwrap();
        assert_eq!(raw_text.as_deref(), Some("Updated text"));
    }

    #[tokio::test]
    async fn note_llm_processed_adds_derived_fields() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());

        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "note-llm".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "raw_text": "Feeling great today!",
                    "date": "2026-03-27"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "note_llm_processed".into(),
                aggregate_id: "note-llm".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "note_id": "note-llm",
                    "prompt_version": "v1",
                    "model": "gemini-flash",
                    "derived": {
                        "tags": ["journal", "mood"],
                        "summary": "A positive journal entry.",
                        "mood": "happy"
                    }
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('notes', 'note-llm')")
            .await
            .unwrap();
        let summary: Option<String> = resp.take("summary").unwrap();
        assert_eq!(summary.as_deref(), Some("A positive journal entry."));

        let mut resp = db
            .query("SELECT * FROM type::record('notes', 'note-llm')")
            .await
            .unwrap();
        let mood: Option<String> = resp.take("mood").unwrap();
        assert_eq!(mood.as_deref(), Some("happy"));
    }
}
