use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

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
    /// The server rejected the *content* of a push (HTTP 400): an unknown
    /// `event_type` or a payload that fails `validate_payload`. Distinct from
    /// [`SyncError::Server`] because retrying is pointless — the same bytes will
    /// be rejected forever — so this drives isolation, not backoff.
    #[error("sync rejected by server: {0}")]
    Rejected(String),
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
#[derive(Debug, Default)]
pub struct PushOutcome {
    /// Events the server acknowledged.
    pub pushed: usize,
    /// Events the server rejected outright and that were skipped so the rest of
    /// the queue could drain. They remain in the local store.
    pub quarantined: usize,
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
    /// Serializes `pull_only` across every clone of this client.
    ///
    /// Nothing else stops two pulls overlapping: the Settings "Sync Now" button
    /// is a bare `spawn` with no disabled state, and the 20s pull scheduler runs
    /// independently, so a double tap — or one tap landing inside a scheduled
    /// tick — used to apply the same pulled batch twice concurrently. Every
    /// projection but one is an UPSERT and absorbed that; `JournalFile` appended,
    /// so the money numbers parsed back out of `budget.journal` silently drifted
    /// from the Ledger list. That handler is now anchor-idempotent as well —
    /// this is the other half, and it also stops the duplicated work.
    ///
    /// A waiting caller re-pulls once the first finishes; the cursor has moved
    /// by then, so the second pull simply returns nothing.
    pull_lock: Arc<Mutex<()>>,
}

impl SyncClient {
    pub fn new(server_url: String, device_id: String) -> Self {
        Self {
            server_url,
            device_id,
            http: crate::http::client(),
            pull_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Attach the box's bearer token to every request this client makes.
    ///
    /// A builder rather than a `new` parameter for two reasons: the 17 test
    /// call sites of `new` stay untouched, and the token rides on the
    /// `reqwest::Client` itself via `default_headers` instead of being applied
    /// per-`send`. A request builder that forgets `.bearer_auth()` is a bug
    /// waiting on the next call site; a client that carries the credential
    /// cannot forget it.
    ///
    /// An empty/blank token is a no-op, matching the server's fail-open
    /// posture when `[server]` is unconfigured.
    pub fn with_token(mut self, token: &str) -> Self {
        let token = token.trim();
        if token.is_empty() {
            return self;
        }
        let mut headers = reqwest::header::HeaderMap::new();
        match reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
            Ok(mut value) => {
                value.set_sensitive(true);
                headers.insert(reqwest::header::AUTHORIZATION, value);
                // Through `http::builder`, NOT `reqwest::Client::builder`. This
                // replaces the client `new()` just built, so a bare builder here
                // would hand the *authenticated* path — the only one that runs
                // in production — a client with no timeout, while `new()`'s
                // timeout survived only in tests.
                match crate::http::builder(crate::http::DEFAULT_TIMEOUT)
                    .default_headers(headers)
                    .build()
                {
                    Ok(client) => self.http = client,
                    Err(e) => tracing::error!(error = %e, "sync: auth client build failed"),
                }
            }
            // Non-ASCII in the token would be a corrupt config, not a live
            // condition worth failing startup over — log and stay unauthed so
            // the failure surfaces as a 401 rather than a silent no-sync.
            Err(e) => tracing::error!(error = %e, "sync: auth token is not a valid header value"),
        }
        self
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
        // The push watermark is this device's own clock and is unaffected by the
        // pull, so it no longer needs snapshotting beforehand — `push_only` reads
        // it itself. Capturing the *pull* cursor pre-pull used to be the guard
        // against missing work created mid-pull; separate watermarks make that
        // structural instead.
        let pull = self.pull_only(db).await?;
        let push = self.push_only(db).await?;

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
        let _in_flight = self.pull_lock.lock().await;
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

    /// Push local events this device has authored but not yet pushed.
    /// Chunks at 100 events per HTTP request.
    ///
    /// Keyed on `last_push_received_at` — **this device's** clock — against each
    /// event's locally-stamped `received_at`. It used to filter the author
    /// timestamp against the *server's* pull cursor, so a device whose clock
    /// trailed the server by more than one pull interval authored events already
    /// below the cursor and never pushed them: silent, permanent, and invisible
    /// to the orphan audit because the `device_id` was correct.
    ///
    /// The window is bounded above by `hi`, captured before the read. Anything
    /// stamped after `hi` simply lands in the next push rather than being
    /// skipped by a watermark that outran it.
    pub async fn push_only(&self, db: &Database) -> Result<PushOutcome, SyncError> {
        let store = SurrealEventStore::new(db.clone());
        let since = self.last_push_watermark(db).await?;
        let hi = Utc::now();

        let local_events: Vec<Event> = self
            .get_local_events_since(&store, &since)
            .await?
            .into_iter()
            .filter(|e| e.received_at.is_none_or(|r| r <= hi))
            .collect();
        let mut outcome = if local_events.is_empty() {
            PushOutcome::default()
        } else {
            self.push_events(&local_events).await?
        };
        // Report what the *server* acknowledged, not the pre-push local count.
        // The two diverge whenever an event is quarantined, and "N up" claiming
        // events the server never counted is exactly the kind of quiet lie this
        // review is trying to remove.
        outcome.pushed = outcome.pushed.min(local_events.len());

        // Advance only after the server accepted everything, and only as far as
        // the events actually pushed — never to `hi`, which would step over an
        // event stamped inside the window but written after the read.
        if let Some(hw) = local_events.iter().filter_map(|e| e.received_at).max() {
            self.update_push_watermark(db, &hw).await?;
        }

        Ok(outcome)
    }

    /// This device's push watermark (epoch if never pushed).
    pub async fn last_push_watermark(
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

        let ts = raw
            .first()
            .and_then(|r| r.get("last_push_received_at"))
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                _ => v.as_str().map(|s| s.to_string()),
            });

        match ts {
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| SyncError::Local(format!("invalid push watermark: {e}"))),
            None => Ok(DateTime::UNIX_EPOCH),
        }
    }

    async fn update_push_watermark(
        &self,
        db: &Database,
        timestamp: &DateTime<Utc>,
    ) -> Result<(), SyncError> {
        let device_id = self.device_id.clone();
        let ts = timestamp.to_rfc3339();
        db.query(
            "UPSERT sync_state SET
                device_id = $device_id,
                last_push_received_at = type::datetime($ts)
             WHERE device_id = $device_id",
        )
        .bind(("device_id", device_id))
        .bind(("ts", ts))
        .await
        .map_err(|e| SyncError::Local(e.to_string()))?;

        Ok(())
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

    /// Push every chunk, isolating any event the server rejects.
    ///
    /// The push path had no poison-pill escape. The server validates the whole
    /// request and returns 400 for the *entire chunk* on one bad payload or one
    /// unknown `event_type`, and `retry_until_success` then resent the identical
    /// chunk forever at a 60s cap while the cursor never advanced — so **all**
    /// outbound sync for that device was permanently wedged, not just the bad
    /// event. Reachable from a legacy-shaped event still above the cursor, or a
    /// client that self-updates ahead of the box. This is precisely the
    /// asymmetry the pull side already fixed with `apply_events_resilient`.
    ///
    /// On a rejection the chunk is bisected until the offending event is alone,
    /// then that one event is skipped and logged. It stays in the local store,
    /// so nothing is destroyed — it simply stops holding the queue hostage.
    async fn push_events(&self, events: &[Event]) -> Result<PushOutcome, SyncError> {
        let url = format!("{}/sync/push", self.server_url);
        let mut out = PushOutcome::default();

        // Explicit stack rather than recursion: an async fn that calls itself
        // needs boxing, and this stays flat and easy to reason about.
        let mut stack: Vec<Vec<NewEvent>> = chunk_for_push(events);
        stack.reverse();

        while let Some(chunk) = stack.pop() {
            match self.post_push_chunk(&url, chunk.clone()).await {
                Ok(count) => out.pushed += count,
                Err(SyncError::Rejected(msg)) => {
                    if chunk.len() == 1 {
                        let bad = &chunk[0];
                        tracing::error!(
                            event_id = ?bad.id,
                            event_type = %bad.event_type,
                            aggregate_id = %bad.aggregate_id,
                            reason = %msg,
                            "server rejected this event; skipping it so the rest of the \
                             push queue can drain. It remains in the local event store."
                        );
                        out.quarantined += 1;
                    } else {
                        let mid = chunk.len() / 2;
                        let (head, tail) = chunk.split_at(mid);
                        // Pushed in reverse so `head` is processed first.
                        stack.push(tail.to_vec());
                        stack.push(head.to_vec());
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Ok(out)
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
            if status == reqwest::StatusCode::BAD_REQUEST {
                return Err(SyncError::Rejected(body));
            }
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
            received_at: None,
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
