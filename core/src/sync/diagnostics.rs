//! Durability guardrail 3 — sync orphan-device-id self-check.
//!
//! The sync push is device-id filtered: `push_only` sends only events whose
//! `device_id` matches the client's bound id (correct anti-re-push-storm
//! behaviour — you don't re-push what you pulled from a peer). The invariant that
//! keeps local data syncable is therefore: **every event this installation
//! *authors* must carry the bound `device_id`.** If a broken authoring path
//! stamps some other id, those events sit in `local.db` forever — never pushed,
//! never on the server (the headless-import stranding bug that made the whole
//! imported ledger invisible on mobile).
//!
//! The canonical event-builder removes the *authoring-time* cause. This is the
//! runtime backstop: a cheap startup audit that flags a db which was created by a
//! broken authoring path (wrong-id import, restore-from-wrong-backup, a
//! regenerated `device_id` file). It never mutates anything and never blocks boot.

use chrono::{DateTime, Utc};

use crate::db::Database;
use crate::sync::SyncError;

/// Per-`device_id` breakdown of the local event log, plus whether this device has
/// ever completed a pull. Cheap to compute — one grouped count and one lookup.
#[derive(Debug, Clone)]
pub struct DeviceIdAudit {
    /// The device id the sync client is bound to (the "own" id).
    pub own_device_id: String,
    /// How many local events were authored under the own id.
    pub own_count: usize,
    /// Every other device id present in the log with its event count, sorted
    /// descending by count. On a healthy device these are events pulled from
    /// peers; on an orphaned db they were authored locally under the wrong id.
    pub foreign: Vec<(String, usize)>,
    /// True once this device has recorded a successful pull (its `sync_state`
    /// timestamp is past epoch). This is what separates a healthy fresh-pulled
    /// device (foreign events legitimately came from the server) from an
    /// orphan-stranded one (foreign events were authored locally and never sent).
    pub ever_synced: bool,
}

impl DeviceIdAudit {
    /// Total events in the local log.
    pub fn total(&self) -> usize {
        self.own_count + self.foreign.iter().map(|(_, n)| n).sum::<usize>()
    }

    /// The orphan signature: the log is non-empty, this device authored **nothing**
    /// under its bound id, and it has **never** pulled — so everything present was
    /// authored locally under a foreign id and can never be pushed.
    ///
    /// This is false-positive-free against the states that *look* similar:
    /// * a healthy fresh-pulled device has `ever_synced == true` (it DID pull), so
    ///   its server-origin foreign events don't trip it;
    /// * a brand-new empty device has `total() == 0`.
    ///
    /// It only detects the *pure* orphan case (own authored nothing). A mixed db
    /// (some own events + stranded foreign events) can't be told apart from
    /// own-plus-pulled without per-event provenance, so it is left to the
    /// always-logged distribution rather than auto-flagged.
    pub fn orphan_signature(&self) -> bool {
        self.total() > 0 && self.own_count == 0 && !self.ever_synced
    }

    /// A compact one-line summary for logs.
    pub fn summary(&self) -> String {
        let foreign = if self.foreign.is_empty() {
            "none".to_string()
        } else {
            self.foreign
                .iter()
                .map(|(id, n)| format!("{id}={n}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "device_id audit: own({})={}, foreign=[{}], ever_synced={}, total={}",
            self.own_device_id,
            self.own_count,
            foreign,
            self.ever_synced,
            self.total()
        )
    }
}

/// Audit the local event log's `device_id` distribution against the client's
/// bound id. Read-only; safe to call at startup.
pub async fn audit_device_ids(
    db: &Database,
    own_device_id: &str,
) -> Result<DeviceIdAudit, SyncError> {
    let mut resp = db
        .query("SELECT device_id, count() AS n FROM events GROUP BY device_id")
        .await
        .map_err(|e| SyncError::Local(e.to_string()))?;
    let rows: Vec<serde_json::Value> = resp
        .take(0)
        .map_err(|e| SyncError::Local(format!("take device counts: {e}")))?;

    let mut own_count = 0usize;
    let mut foreign: Vec<(String, usize)> = Vec::new();
    for row in rows {
        let id = row
            .get("device_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let n = row
            .get("n")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        if id == own_device_id {
            own_count = n;
        } else if n > 0 {
            foreign.push((id, n));
        }
    }
    foreign.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let ever_synced = has_ever_synced(db, own_device_id).await?;

    Ok(DeviceIdAudit {
        own_device_id: own_device_id.to_string(),
        own_count,
        foreign,
        ever_synced,
    })
}

/// True when this device's `sync_state` row carries a `last_sync_timestamp`
/// strictly after the Unix epoch — i.e. it has completed at least one pull.
async fn has_ever_synced(db: &Database, own_device_id: &str) -> Result<bool, SyncError> {
    let own = own_device_id.to_string();
    let mut resp = db
        .query(
            "SELECT <string> last_sync_timestamp AS ts
             FROM sync_state WHERE device_id = $own",
        )
        .bind(("own", own))
        .await
        .map_err(|e| SyncError::Local(e.to_string()))?;
    let rows: Vec<serde_json::Value> = resp
        .take(0)
        .map_err(|e| SyncError::Local(format!("take sync_state: {e}")))?;

    let epoch = DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    Ok(rows.iter().any(|r| {
        r.get("ts")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc) > epoch)
            .unwrap_or(false)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit(own: usize, foreign: &[(&str, usize)], ever_synced: bool) -> DeviceIdAudit {
        DeviceIdAudit {
            own_device_id: "own".into(),
            own_count: own,
            foreign: foreign
                .iter()
                .map(|(id, n)| ((*id).to_string(), *n))
                .collect(),
            ever_synced,
        }
    }

    #[test]
    fn empty_device_is_not_orphaned() {
        assert!(!audit(0, &[], false).orphan_signature());
    }

    #[test]
    fn healthy_fresh_pulled_device_is_not_orphaned() {
        // Own authored nothing yet, but the log is full of server-origin foreign
        // events AND we have pulled — the exact desktop dress-rehearsal state.
        let a = audit(0, &[("seeder", 11_844)], true);
        assert_eq!(a.total(), 11_844);
        assert!(!a.orphan_signature());
    }

    #[test]
    fn pure_orphan_import_is_flagged() {
        // ~10k events authored locally under a phantom id, never synced.
        let a = audit(0, &[("headless-import", 10_207)], false);
        assert!(a.orphan_signature());
    }

    #[test]
    fn normal_multi_device_is_not_orphaned() {
        let a = audit(47, &[("phone", 300)], true);
        assert!(!a.orphan_signature());
        assert_eq!(a.total(), 347);
    }

    #[test]
    fn own_events_present_never_trips_even_without_sync() {
        // Own authored events → not the pure-orphan case, left to the log.
        assert!(!audit(5, &[("headless-import", 10_000)], false).orphan_signature());
    }

    #[test]
    fn summary_is_readable() {
        let s = audit(2, &[("phone", 3)], true).summary();
        assert!(s.contains("own(own)=2"));
        assert!(s.contains("phone=3"));
        assert!(s.contains("ever_synced=true"));
    }
}
