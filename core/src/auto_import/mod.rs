//! Auto-import source implementations.
//!
//! Each submodule implements `auto_import_scheduler::AutoImportSource` for a
//! specific upstream — Northwind via Python subprocess, Globepay via REST API,
//! IMAP poller for emailed statements / receipts.
//!
//! All sources share two invariants:
//! - Pulled transactions are bundled into a single `AutoImportBatchProposed`
//!   event (one event per tick that produced new data). On user-side commit
//!   the batch fans out into `TransactionRecorded` events with one real-account
//!   posting + one mirror posting to `Unmatched` (per the unmatched-account
//!   pattern — the reconciliation matching engine collapses pairs later).
//! - Dedup happens via the `dedup_key` field on the proposed event, derived
//!   from each upstream's stable external id (e.g. Meridian AED message UID, Globepay
//!   transfer-id watermark, etc.).

use crate::events::{AutoImportBatchProposedPayload, DraftTransaction, EventType, NewEvent};
use chrono::Utc;
use serde_json::Value;

pub mod config;
pub mod csv;
pub mod imap;
pub mod imap_real;
pub mod imap_source;
pub mod mime;
pub mod paused;
pub mod receipts;
pub mod rest;
pub mod setup;
pub mod subprocess;

/// A `dedup_key` derived from the drafts themselves, so a polling source that
/// re-fetches an overlapping window collapses onto the row it already proposed.
///
/// The projection keys `pending_auto_import_batches` on `source-dedup_key` and
/// UPSERTs, so a *stable* key is what makes re-proposal idempotent. A key built
/// from the wall clock defeats that by construction — it is unique every tick,
/// so each poll adds another review row for data the user has already seen.
/// That is not hypothetical: a timestamp key did exactly this to a polling
/// source in production, adding a review row every tick for data already in
/// the journal.
///
/// Order-insensitive (the ids are sorted first) because upstream APIs do not
/// promise a stable row order, and a reordered-but-identical window is the same
/// batch. FNV-1a rather than `DefaultHasher`, whose output is explicitly not
/// stable across releases — this key has to survive a process restart.
pub fn content_dedup_key(source: &str, drafts: &[DraftTransaction]) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut ids: Vec<&str> = drafts.iter().map(|d| d.external_id.as_str()).collect();
    ids.sort_unstable();

    let mut hash = FNV_OFFSET;
    for id in ids {
        for &b in id.as_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        // Separator, so ["ab","c"] and ["a","bc"] don't collide.
        hash ^= 0x1f;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{source}-content-{hash:016x}")
}

/// Wrap a vec of draft transactions into a single `AutoImportBatchProposed`
/// event. Called by each handler's `pull()` after fetching + per-row dedup.
/// Generates a fresh ULID for the batch_id so cross-event correlation
/// (commit / dismiss reference back to the proposal) is unambiguous.
///
/// `dedup_key` is the source's natural idempotency token — what the scheduler
/// checks to avoid re-proposing the same upstream data. Shape is source-defined:
/// - Meridian AED / receipts → `format!("{source}-uid-{message_uid}")` (per-email)
/// - Globepay / Northwind → `format!("{source}-watermark-{max_external_id}")` (or similar)
///
/// `source_metadata` is opaque JSON the review UI can render (e.g. IMAP
/// `from`/`subject` for context, Globepay statement window dates, etc.).
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

#[cfg(test)]
mod dedup_key_tests {
    use super::content_dedup_key;
    use crate::events::DraftTransaction;

    fn draft(external_id: &str) -> DraftTransaction {
        DraftTransaction {
            external_id: external_id.to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(),
            description: "irrelevant to the key".into(),
            postings: vec![],
        }
    }

    /// The whole point: re-polling an unchanged window must land on the same
    /// row rather than stacking a new one every tick.
    #[test]
    fn same_drafts_yield_same_key() {
        let a = content_dedup_key("globepay", &[draft("globepay-T1"), draft("globepay-T2")]);
        let b = content_dedup_key("globepay", &[draft("globepay-T1"), draft("globepay-T2")]);
        assert_eq!(a, b);
    }

    #[test]
    fn order_does_not_change_the_key() {
        let a = content_dedup_key("globepay", &[draft("globepay-T1"), draft("globepay-T2")]);
        let b = content_dedup_key("globepay", &[draft("globepay-T2"), draft("globepay-T1")]);
        assert_eq!(a, b);
    }

    #[test]
    fn a_new_upstream_row_changes_the_key() {
        let a = content_dedup_key("globepay", &[draft("globepay-T1")]);
        let b = content_dedup_key("globepay", &[draft("globepay-T1"), draft("globepay-T2")]);
        assert_ne!(a, b);
    }

    /// Guards the separator: without it these two sets hash identically.
    #[test]
    fn boundary_between_ids_is_significant() {
        let a = content_dedup_key("globepay", &[draft("ab"), draft("c")]);
        let b = content_dedup_key("globepay", &[draft("a"), draft("bc")]);
        assert_ne!(a, b);
    }

    #[test]
    fn source_is_part_of_the_key() {
        let a = content_dedup_key("globepay", &[draft("T1")]);
        let b = content_dedup_key("northwind", &[draft("T1")]);
        assert_ne!(a, b);
    }
}
