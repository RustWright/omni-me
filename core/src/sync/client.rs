use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::events::{Event, EventStore, NewEvent, SurrealEventStore};

/// The server caps `/sync/push` at 100 events AND a 256 KiB request body
/// (`DefaultBodyLimit`). Chunk pushes to stay under BOTH: a fixed count of 100
/// large transaction events overflows the byte limit (413 Payload Too Large),
/// which silently stranded a bulk re-import mid-stream. Budget well under
/// 256 KiB to leave headroom for the JSON envelope.
const MAX_EVENTS_PER_PUSH: usize = 100;
const MAX_PUSH_BYTES: usize = 200 * 1024;

/// Error type for sync operations.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("sync network error: {0}")]
    Network(String),
    #[error("sync server error: {0}")]
    Server(String),
    #[error("sync local error: {0}")]
    Local(String),
}

/// Result of a sync operation.
#[derive(Debug)]
pub struct SyncResult {
    pub pulled: usize,
    pub pushed: usize,
    pub pulled_events: Vec<Event>,
}

/// Request body for POST /sync/push
#[derive(Debug, Serialize, Deserialize)]
pub struct PushRequest {
    pub device_id: String,
    pub events: Vec<NewEvent>,
}

/// Response from POST /sync/push
#[derive(Debug, Serialize, Deserialize)]
pub struct PushResponse {
    pub count: usize,
}

/// Request body for POST /sync/pull
#[derive(Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub device_id: String,
    pub since: DateTime<Utc>,
}

/// Response from POST /sync/pull
#[derive(Debug, Serialize, Deserialize)]
pub struct PullResponse {
    pub events: Vec<Event>,
    pub sync_timestamp: DateTime<Utc>,
}

/// Outcome of a pull-only call. Exposes the raw pulled events so callers can
/// feed them into projections.
#[derive(Debug)]
pub struct PullOutcome {
    pub pulled: usize,
    pub pulled_events: Vec<Event>,
    pub new_timestamp: DateTime<Utc>,
}

/// Outcome of a push-only call.
#[derive(Debug)]
pub struct PushOutcome {
    pub pushed: usize,
}

/// Client that syncs local events with a remote server.
///
/// The client is decomposed into independent primitives — `pull_only`,
/// `push_only`, and `sync_state` accessors — so higher-level drivers (e.g.
/// the debounced push loop in `pusher.rs`) can orchestrate phases
/// independently. The legacy `sync()` method remains as a convenience wrapper
/// that runs pull→push atomically.
#[derive(Clone)]
pub struct SyncClient {
    server_url: String,
    device_id: String,
    http: reqwest::Client,
}

impl SyncClient {
    pub fn new(server_url: String, device_id: String) -> Self {
        Self {
            server_url,
            device_id,
            http: reqwest::Client::new(),
        }
    }

    /// The device ID this client is bound to.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// The server URL this client talks to.
    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    /// Perform a full sync: pull remote events, then push local events.
    /// Preserved for backward compatibility with integration tests and the
    /// `trigger_sync` Tauri command.
    pub async fn sync(&self, db: &Database) -> Result<SyncResult, SyncError> {
        let last_sync = self.last_sync_timestamp(db).await?;

        // 1. Pull + apply + update sync_state timestamp.
        let pull = self.pull_only(db).await?;

        // 2. Push any local events since the *pre-pull* timestamp (so we don't
        //    miss work created while the pull was in flight).
        let push = self.push_only(db, &last_sync).await?;

        Ok(SyncResult {
            pulled: pull.pulled,
            pushed: push.pushed,
            pulled_events: pull.pulled_events,
        })
    }

    /// Pull remote events since our last sync, append them locally (preserving
    /// server-assigned IDs), and advance `sync_state.last_sync_timestamp`.
    ///
    /// Does NOT push. Callers wanting a full sync should follow with
    /// `push_only`, or use `sync()`.
    pub async fn pull_only(&self, db: &Database) -> Result<PullOutcome, SyncError> {
        let store = SurrealEventStore::new(db.clone());
        let last_sync = self.last_sync_timestamp(db).await?;

        let pull_resp = self.pull_events(&last_sync).await?;
        let pulled = pull_resp.events.len();

        for event in &pull_resp.events {
            store
                .append(NewEvent::from(event))
                .await
                .map_err(|e| SyncError::Local(e.to_string()))?;
        }

        // Advance sync_state timestamp AFTER successful pull so a push-only
        // failure later doesn't cause us to re-pull the same events.
        let new_timestamp = pull_resp.sync_timestamp;
        self.update_last_sync_timestamp(db, &new_timestamp).await?;

        Ok(PullOutcome {
            pulled,
            pulled_events: pull_resp.events,
            new_timestamp,
        })
    }

    /// Push all local events from this device created after `since` to the
    /// server. Chunks at 100 events per HTTP request.
    pub async fn push_only(
        &self,
        db: &Database,
        since: &DateTime<Utc>,
    ) -> Result<PushOutcome, SyncError> {
        let store = SurrealEventStore::new(db.clone());
        let local_events = self.get_local_events_since(&store, since).await?;
        let pushed = local_events.len();

        if !local_events.is_empty() {
            self.push_events(&local_events).await?;
        }

        Ok(PushOutcome { pushed })
    }

    /// The last-sync timestamp recorded for this device (epoch if none).
    pub async fn last_sync_timestamp(
        &self,
        db: &Database,
    ) -> Result<DateTime<Utc>, SyncError> {
        let device_id = self.device_id.clone();
        let mut resp = db
            .query("SELECT * FROM sync_state WHERE device_id = $device_id")
            .bind(("device_id", device_id))
            .await
            .map_err(|e| SyncError::Local(e.to_string()))?;

        let raw: Vec<serde_json::Value> = resp
            .take(0)
            .map_err(|e| SyncError::Local(format!("take raw: {e}")))?;

        let ts = raw.first()
            .and_then(|r| r.get("last_sync_timestamp"))
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                _ => v.as_str().map(|s| s.to_string()),
            });

        match ts {
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| SyncError::Local(format!("invalid timestamp in sync_state: {e}"))),
            None => {
                // No sync state yet — use epoch
                Ok(DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc))
            }
        }
    }

    async fn update_last_sync_timestamp(
        &self,
        db: &Database,
        timestamp: &DateTime<Utc>,
    ) -> Result<(), SyncError> {
        let device_id = self.device_id.clone();
        let ts = timestamp.to_rfc3339();
        db.query(
            "UPSERT sync_state SET
                device_id = $device_id,
                last_sync_timestamp = type::datetime($ts)
             WHERE device_id = $device_id",
        )
        .bind(("device_id", device_id))
        .bind(("ts", ts))
        .await
        .map_err(|e| SyncError::Local(e.to_string()))?;

        Ok(())
    }

    async fn pull_events(
        &self,
        since: &DateTime<Utc>,
    ) -> Result<PullResponse, SyncError> {
        let url = format!("{}/sync/pull", self.server_url);
        let body = PullRequest {
            device_id: self.device_id.clone(),
            since: *since,
        };

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server(format!("pull failed ({status}): {body}")));
        }

        resp.json::<PullResponse>()
            .await
            .map_err(|e| SyncError::Network(format!("failed to parse pull response: {e}")))
    }

    async fn push_events(&self, events: &[Event]) -> Result<usize, SyncError> {
        let url = format!("{}/sync/push", self.server_url);
        let mut total = 0;
        for chunk in chunk_for_push(events) {
            total += self.post_push_chunk(&url, chunk).await?;
        }
        Ok(total)
    }

    /// POST one already-sized batch of events to `/sync/push`.
    async fn post_push_chunk(
        &self,
        url: &str,
        new_events: Vec<NewEvent>,
    ) -> Result<usize, SyncError> {
        let body = PushRequest {
            device_id: self.device_id.clone(),
            events: new_events,
        };

        let resp = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server(format!("push failed ({status}): {body}")));
        }

        let push_resp: PushResponse = resp
            .json()
            .await
            .map_err(|e| SyncError::Network(format!("failed to parse push response: {e}")))?;

        Ok(push_resp.count)
    }

    async fn get_local_events_since(
        &self,
        store: &SurrealEventStore,
        since: &DateTime<Utc>,
    ) -> Result<Vec<Event>, SyncError> {
        // Get events from this device only (filtered at the DB layer)
        store
            .get_since_by_device(*since, &self.device_id)
            .await
            .map_err(|e| SyncError::Local(e.to_string()))
    }
}

/// Pack events into push requests that respect BOTH caps the server enforces on
/// `/sync/push`: at most `MAX_EVENTS_PER_PUSH` events and at most `MAX_PUSH_BYTES`
/// serialized bytes per request. Pure (no I/O) so it is unit-testable. An event
/// larger than the byte budget is still emitted alone (best effort) rather than
/// dropped.
fn chunk_for_push(events: &[Event]) -> Vec<Vec<NewEvent>> {
    let mut chunks: Vec<Vec<NewEvent>> = Vec::new();
    let mut batch: Vec<NewEvent> = Vec::new();
    let mut bytes = 0usize;
    for ev in events {
        let ne = NewEvent::from(ev);
        let sz = serde_json::to_vec(&ne).map(|v| v.len()).unwrap_or(0);
        if !batch.is_empty()
            && (batch.len() >= MAX_EVENTS_PER_PUSH || bytes + sz > MAX_PUSH_BYTES)
        {
            chunks.push(std::mem::take(&mut batch));
            bytes = 0;
        }
        batch.push(ne);
        bytes += sz;
    }
    if !batch.is_empty() {
        chunks.push(batch);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, payload_len: usize) -> Event {
        Event {
            id: id.to_string(),
            event_type: "transaction_recorded".to_string(),
            aggregate_id: id.to_string(),
            timestamp: Utc::now(),
            device_id: "test-device".to_string(),
            payload: serde_json::json!({ "blob": "x".repeat(payload_len) }),
        }
    }

    fn chunk_bytes(c: &[NewEvent]) -> usize {
        c.iter().map(|ne| serde_json::to_vec(ne).unwrap().len()).sum()
    }

    #[test]
    fn respects_event_count_cap() {
        let events: Vec<Event> = (0..250).map(|i| ev(&format!("e{i}"), 10)).collect();
        let chunks = chunk_for_push(&events);
        assert!(chunks.iter().all(|c| c.len() <= MAX_EVENTS_PER_PUSH));
        assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), 250);
    }

    #[test]
    fn respects_byte_cap() {
        // ~50 KiB each; five of them (~250 KiB) must split into >1 chunk.
        let events: Vec<Event> = (0..5).map(|i| ev(&format!("e{i}"), 50 * 1024)).collect();
        let chunks = chunk_for_push(&events);
        assert!(chunks.len() > 1, "expected split, got {} chunk(s)", chunks.len());
        for c in &chunks {
            assert!(chunk_bytes(c) <= MAX_PUSH_BYTES || c.len() == 1);
        }
        assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), 5);
    }

    #[test]
    fn oversized_single_event_emitted_alone() {
        let events = vec![ev("big", MAX_PUSH_BYTES * 2)];
        let chunks = chunk_for_push(&events);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 1);
    }
}
