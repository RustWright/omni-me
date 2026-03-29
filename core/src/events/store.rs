use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{SurrealValue, Value as DbValue};
use std::fmt;

use crate::db::Database;

/// Error type for event store operations.
#[derive(Debug)]
pub enum EventError {
    Store(String),
    Validation(String),
    Projection(String),
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventError::Store(msg) => write!(f, "event store error: {msg}"),
            EventError::Validation(msg) => write!(f, "event validation error: {msg}"),
            EventError::Projection(msg) => write!(f, "projection error: {msg}"),
        }
    }
}

impl std::error::Error for EventError {}

impl From<surrealdb::Error> for EventError {
    fn from(err: surrealdb::Error) -> Self {
        EventError::Store(err.to_string())
    }
}

/// A persisted event with a generated ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event_type: String,
    pub aggregate_id: String,
    pub timestamp: DateTime<Utc>,
    pub device_id: String,
    pub payload: serde_json::Value,
}

/// An event to be appended — supply an ID to preserve it, or leave as None to auto-generate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEvent {
    #[serde(default)]
    pub id: Option<String>,
    pub event_type: String,
    pub aggregate_id: String,
    pub timestamp: DateTime<Utc>,
    pub device_id: String,
    pub payload: serde_json::Value,
}

/// Event store trait — append-only event log.
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Append a new event, generating a ULID for the id.
    async fn append(&self, event: NewEvent) -> Result<Event, EventError>;

    /// Get all events since a given timestamp, optionally excluding a specific device.
    async fn get_since(
        &self,
        since: DateTime<Utc>,
        exclude_device: Option<&str>,
    ) -> Result<Vec<Event>, EventError>;

    /// Get all events for a given aggregate, ordered by timestamp.
    async fn get_by_aggregate(&self, aggregate_id: &str) -> Result<Vec<Event>, EventError>;
}

/// SurrealDB-backed event store implementation.
#[derive(Clone)]
pub struct SurrealEventStore {
    db: Database,
}

impl SurrealEventStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl EventStore for SurrealEventStore {
    async fn append(&self, event: NewEvent) -> Result<Event, EventError> {
        let id = event.id.clone().unwrap_or_else(|| ulid::Ulid::new().to_string());
        let ts_str = event.timestamp.to_rfc3339();

        // INSERT IGNORE silently skips if the record already exists (idempotent for sync).
        self.db
            .query(
                "INSERT INTO events {
                    id: type::record('events', $id),
                    event_type: $event_type,
                    aggregate_id: $aggregate_id,
                    timestamp: type::datetime($timestamp),
                    device_id: $device_id,
                    payload: $payload
                } ON DUPLICATE KEY UPDATE id = id",
            )
            .bind(("id", id.clone()))
            .bind(("event_type", event.event_type.clone()))
            .bind(("aggregate_id", event.aggregate_id.clone()))
            .bind(("timestamp", ts_str))
            .bind(("device_id", event.device_id.clone()))
            .bind(("payload", event.payload.clone()))
            .await
            .map_err(|e| EventError::Store(e.to_string()))?;

        Ok(Event {
            id,
            event_type: event.event_type,
            aggregate_id: event.aggregate_id,
            timestamp: event.timestamp,
            device_id: event.device_id,
            payload: event.payload,
        })
    }

    async fn get_since(
        &self,
        since: DateTime<Utc>,
        exclude_device: Option<&str>,
    ) -> Result<Vec<Event>, EventError> {
        let since_str = since.to_rfc3339();
        let exclude = exclude_device.unwrap_or("").to_string();

        let query = match exclude_device {
            Some(_) => {
                "SELECT meta::id(id) AS eid, event_type, aggregate_id,
                        <string> timestamp AS ts, timestamp,
                        device_id, payload
                 FROM events
                 WHERE timestamp > type::datetime($since) AND device_id != $exclude_device
                 ORDER BY timestamp ASC"
            }
            None => {
                "SELECT meta::id(id) AS eid, event_type, aggregate_id,
                        <string> timestamp AS ts, timestamp,
                        device_id, payload
                 FROM events
                 WHERE timestamp > type::datetime($since)
                 ORDER BY timestamp ASC"
            }
        };

        let mut response = self
            .db
            .query(query)
            .bind(("since", since_str))
            .bind(("exclude_device", exclude))
            .await
            .map_err(|e| EventError::Store(e.to_string()))?;

        let rows: Vec<EventRow> = response
            .take(0)
            .map_err(|e| EventError::Store(e.to_string()))?;

        rows.into_iter().map(Event::try_from).collect()
    }

    async fn get_by_aggregate(&self, aggregate_id: &str) -> Result<Vec<Event>, EventError> {
        let agg_id = aggregate_id.to_string();

        let mut response = self
            .db
            .query(
                "SELECT meta::id(id) AS eid, event_type, aggregate_id,
                        <string> timestamp AS ts, timestamp,
                        device_id, payload
                 FROM events
                 WHERE aggregate_id = $aggregate_id
                 ORDER BY timestamp ASC",
            )
            .bind(("aggregate_id", agg_id))
            .await
            .map_err(|e| EventError::Store(e.to_string()))?;

        let rows: Vec<EventRow> = response
            .take(0)
            .map_err(|e| EventError::Store(e.to_string()))?;

        rows.into_iter().map(Event::try_from).collect()
    }
}

/// Internal row struct for SurrealQL query deserialization.
/// Timestamp is cast to string (RFC3339 parseable), payload kept as native SurrealDB Value.
#[derive(Debug, SurrealValue)]
struct EventRow {
    eid: String,
    event_type: String,
    aggregate_id: String,
    ts: String,
    device_id: String,
    payload: DbValue,
}

impl TryFrom<EventRow> for Event {
    type Error = EventError;

    fn try_from(row: EventRow) -> Result<Self, Self::Error> {
        let timestamp = DateTime::parse_from_rfc3339(&row.ts)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| EventError::Store(format!("invalid timestamp '{}': {e}", row.ts)))?;

        let payload = row.payload.into_json_value();

        Ok(Event {
            id: row.eid,
            event_type: row.event_type,
            aggregate_id: row.aggregate_id,
            timestamp,
            device_id: row.device_id,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    #[tokio::test]
    async fn append_and_retrieve_by_aggregate() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db);

        let new_event = NewEvent {
            id: None,
            event_type: "note_created".into(),
            aggregate_id: "note-1".into(),
            timestamp: Utc::now(),
            device_id: "device-a".into(),
            payload: serde_json::json!({"raw_text": "hello", "date": "2026-03-27"}),
        };

        let event = store.append(new_event).await.unwrap();
        assert_eq!(event.event_type, "note_created");
        assert_eq!(event.aggregate_id, "note-1");
        assert!(!event.id.is_empty());

        let events = store.get_by_aggregate("note-1").await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event.id);
        // Verify payload roundtrips
        assert_eq!(events[0].payload["raw_text"], "hello");
    }

    #[tokio::test]
    async fn get_since_filters_by_timestamp() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db);

        let t1 = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let t2 = chrono::DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "n1".into(),
                timestamp: t1,
                device_id: "d1".into(),
                payload: serde_json::json!({"raw_text": "old", "date": "2026-01-01"}),
            })
            .await
            .unwrap();

        store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "n2".into(),
                timestamp: t2,
                device_id: "d1".into(),
                payload: serde_json::json!({"raw_text": "new", "date": "2026-06-01"}),
            })
            .await
            .unwrap();

        let cutoff = chrono::DateTime::parse_from_rfc3339("2026-03-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let events = store.get_since(cutoff, None).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].aggregate_id, "n2");
    }

    #[tokio::test]
    async fn get_since_excludes_device() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db);

        let ts = Utc::now();

        store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "n1".into(),
                timestamp: ts,
                device_id: "device-a".into(),
                payload: serde_json::json!({"raw_text": "from A", "date": "2026-03-27"}),
            })
            .await
            .unwrap();

        store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "n2".into(),
                timestamp: ts,
                device_id: "device-b".into(),
                payload: serde_json::json!({"raw_text": "from B", "date": "2026-03-27"}),
            })
            .await
            .unwrap();

        let early = ts - chrono::Duration::seconds(10);
        let events = store.get_since(early, Some("device-a")).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].device_id, "device-b");
    }
}
