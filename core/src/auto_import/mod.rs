//! Auto-import source implementations (Phase 2B + 3.10).
//!
//! Each submodule implements `auto_import_scheduler::AutoImportSource` for a
//! specific upstream — WealthSimple via Python subprocess, Wise via REST API,
//! IMAP poller for emailed statements / receipts.
//!
//! All sources share two invariants:
//! - Pulled transactions are bundled into a single `AutoImportBatchProposed`
//!   event (one event per tick that produced new data). On user-side commit
//!   the batch fans out into `TransactionRecorded` events with one real-account
//!   posting + one mirror posting to `Unmatched` (per the unmatched-account
//!   pattern — the matching engine in Phase 5.6/5.7 collapses pairs later).
//! - Dedup happens via the `dedup_key` field on the proposed event, derived
//!   from each upstream's stable external id (e.g. SC NGN message UID, Wise
//!   transfer-id watermark, etc.).

use crate::events::{AutoImportBatchProposedPayload, DraftTransaction, EventType, NewEvent};
use chrono::Utc;
use serde_json::Value;

pub mod imap;
pub mod imap_real;
pub mod imap_source;
pub mod mime;
pub mod receipts;
pub mod setup;
pub mod subprocess;

/// Wrap a vec of draft transactions into a single `AutoImportBatchProposed`
/// event. Called by each handler's `pull()` after fetching + per-row dedup.
/// Generates a fresh ULID for the batch_id so cross-event correlation
/// (commit / dismiss reference back to the proposal) is unambiguous.
///
/// `dedup_key` is the source's natural idempotency token — what the scheduler
/// checks to avoid re-proposing the same upstream data. Shape is source-defined:
/// - SC NGN / receipts → `format!("{source}-uid-{message_uid}")` (per-email)
/// - Wise / WS → `format!("{source}-watermark-{max_external_id}")` (or similar)
///
/// `source_metadata` is opaque JSON the review UI can render (e.g. IMAP
/// `from`/`subject` for context, Wise statement window dates, etc.).
pub fn to_proposed_event(
    source: &str,
    dedup_key: String,
    drafts: Vec<DraftTransaction>,
    source_metadata: Option<Value>,
    device_id: String,
) -> NewEvent {
    let batch_id = ulid::Ulid::new().to_string();
    let payload = AutoImportBatchProposedPayload {
        batch_id: batch_id.clone(),
        source: source.to_string(),
        dedup_key,
        fetched_at: Utc::now(),
        draft_postings: drafts,
        source_metadata,
    };
    NewEvent {
        id: Some(batch_id.clone()),
        event_type: EventType::AutoImportBatchProposed.to_string(),
        aggregate_id: batch_id,
        timestamp: Utc::now(),
        device_id,
        payload: serde_json::to_value(&payload).expect("payload is always serializable"),
    }
}
