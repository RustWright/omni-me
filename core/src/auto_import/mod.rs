//! Auto-import source implementations (Phase 2B).
//!
//! Each submodule implements `auto_import_scheduler::AutoImportSource` for a
//! specific upstream — WealthSimple via Python subprocess, Wise via REST API,
//! IMAP poller for emailed statements / receipts.
//!
//! All sources share two invariants:
//! - Pulled transactions emit `TransactionRecorded` events with one real-account
//!   posting + one mirror posting to `Unmatched` (per the unmatched-account
//!   pattern — the matching engine in Phase 5.6/5.7 collapses pairs later).
//! - Dedup happens via deterministic event ids derived from each upstream's
//!   stable external id, so re-runs after partial failure are idempotent.

pub mod wealthsimple;
pub mod wise;
