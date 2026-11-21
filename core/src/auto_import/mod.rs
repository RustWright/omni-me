//! Auto-import source implementations.
//!
//! `AutoImportSource` implementations (subprocess, REST, CSV, IMAP). Every source proposes one
//! `AutoImportBatchProposed` per productive tick, committed as a real posting plus an `Unmatched`
//! mirror, and dedupes on a `dedup_key` from the upstream's stable id.

use crate::events::{AutoImportBatchProposedPayload, DraftTransaction, EventType, NewEvent};
use chrono::Utc;
use serde_json::Value;

pub mod backfill;
pub mod config;
pub mod csv;
pub mod duplicates;
pub mod imap;
pub mod imap_real;
pub mod imap_source;
// Moved to `crate::mime` so the archive can read an .eml without this
// feature. Re-exported so `super::mime::…` keeps resolving in here.
pub use crate::mime;
pub mod order_ref;
pub mod paused;
pub mod receipts;
pub mod rest;
pub mod sender_auth;
pub mod setup;
pub mod subprocess;
pub mod triage;

/// What a record-oriented parse produced, including what it could not
/// parse.
///
/// `rows_seen` beside the drafts gives the caller both sides of the identity; a bare `Vec` hid
/// dropped rows. A failed row stays non-fatal, but is counted and named, not only logged.
#[derive(Debug, Default)]
pub struct ParseOutcome {
    /// Records the reader yielded, before any filtering.
    pub rows_seen: usize,
    pub drafts: Vec<DraftTransaction>,
    /// `(row identity, reason)` for each row that could not become a draft.
    /// The identity is positional (`row 12`) when the source has no id column,
    /// which is still enough to find it in the file.
    pub failures: Vec<(String, String)>,
}

/// A `dedup_key` derived from the drafts themselves, so a polling source that
/// re-fetches an overlapping window collapses onto the row it already proposed.
///
/// The projection UPSERTs on `source-dedup_key`, so a stable key makes re-proposal idempotent;
/// never key on the clock, which adds a review row per tick. Sorted ids, since row order is not
/// promised; FNV-1a, since `DefaultHasher` is not stable across releases.
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

/// Wrap drafts into one `AutoImportBatchProposed` with a fresh ULID batch id. `dedup_key` is the
/// source's idempotency token (per email `{source}-uid-{uid}`, or a watermark); `source_metadata`
/// is opaque JSON the review UI renders.
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
