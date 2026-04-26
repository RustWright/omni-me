use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::events::{Event, EventStore, NewEvent, SurrealEventStore};

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
            let new_event = NewEvent {
                id: Some(event.id.clone()),
                event_type: event.event_type.clone(),
                aggregate_id: event.aggregate_id.clone(),
                timestamp: event.timestamp,
                device_id: event.device_id.clone(),
                payload: event.payload.clone(),
            };
            store
                .append(new_event)
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

        for chunk in events.chunks(100) {
            let new_events: Vec<NewEvent> = chunk
                .iter()
                .map(|e| NewEvent {
                    id: Some(e.id.clone()),
                    event_type: e.event_type.clone(),
                    aggregate_id: e.aggregate_id.clone(),
                    timestamp: e.timestamp,
                    device_id: e.device_id.clone(),
                    payload: e.payload.clone(),
                })
                .collect();

            let body = PushRequest {
                device_id: self.device_id.clone(),
                events: new_events,
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
                return Err(SyncError::Server(format!("push failed ({status}): {body}")));
            }

            let push_resp: PushResponse = resp
                .json()
                .await
                .map_err(|e| SyncError::Network(format!("failed to parse push response: {e}")))?;

            total += push_resp.count;
        }

        Ok(total)
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
