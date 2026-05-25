//! Background scheduler that periodically runs the recurring-pattern
//! detector (Phase 5.3). Same shape as `auto_close_scheduler` —
//! spawned on app boot, sleeps between ticks, emits new
//! `RecurringTransactionDetected` events for patterns not already in
//! the `recurring_patterns` table.
//!
//! Cadence: one warm-up tick 60s after boot (lets the UI settle + the
//! event store finish replay), then once every 24h. The scan is
//! idempotent — already-tracked patterns are skipped, so a re-run
//! never clobbers user confirmations or dismissals.

use std::collections::HashSet;
use std::time::Duration;

use chrono::Utc;
use omni_me_core::db::queries;
use omni_me_core::db::Database;
use omni_me_core::events::{EventStore, EventType, NewEvent, ProjectionRunner, SurrealEventStore};
use omni_me_core::recurring;

const WARMUP_DELAY: Duration = Duration::from_secs(60);
const PERIODIC_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const LOOKBACK_DAYS: i64 = 365;

pub fn spawn(
    db: Database,
    event_store: SurrealEventStore,
    projections: ProjectionRunner,
    device_id: String,
) {
    tauri::async_runtime::spawn(async move {
        // Warm-up — let the initial UI load + event replay settle before
        // touching the projection.
        tokio::time::sleep(WARMUP_DELAY).await;

        loop {
            match run_one_scan(&db, &event_store, &projections, &device_id).await {
                Ok(emitted) if emitted > 0 => tracing::info!(
                    emitted,
                    "recurring-scanner: emitted new pattern events"
                ),
                Ok(_) => tracing::debug!("recurring-scanner: no new patterns this tick"),
                Err(e) => tracing::warn!(error = %e, "recurring-scanner: tick failed"),
            }
            tokio::time::sleep(PERIODIC_INTERVAL).await;
        }
    });
}

async fn run_one_scan(
    db: &Database,
    event_store: &SurrealEventStore,
    projections: &ProjectionRunner,
    device_id: &str,
) -> Result<usize, String> {
    let cutoff = (chrono::Utc::now().date_naive() - chrono::Duration::days(LOOKBACK_DAYS))
        .to_string();
    let txn_rows = queries::list_transactions_since(db, &cutoff)
        .await
        .map_err(|e| e.to_string())?;
    let patterns = recurring::detect_patterns(&txn_rows);
    if patterns.is_empty() {
        return Ok(0);
    }

    let existing_rows = queries::list_recurring_patterns(db, None)
        .await
        .map_err(|e| e.to_string())?;
    let existing_ids: HashSet<String> =
        existing_rows.iter().map(|r| r.id.clone()).collect();

    let mut emitted = 0usize;
    for p in patterns {
        if existing_ids.contains(&p.pattern_id) {
            continue;
        }
        let payload = serde_json::json!({
            "pattern_id": p.pattern_id,
            "pattern": {
                "vendor": p.vendor,
                "amount": p.amount.to_string(),
                "commodity": p.commodity,
                "cadence_days": p.cadence_days,
                "occurrences": p.occurrences,
                "first_seen": p.first_seen.to_string(),
                "last_seen": p.last_seen.to_string(),
            }
        });
        let saved = event_store
            .append(NewEvent {
                id: None,
                event_type: EventType::RecurringTransactionDetected.to_string(),
                aggregate_id: p.pattern_id.clone(),
                timestamp: Utc::now(),
                device_id: device_id.to_string(),
                payload,
            })
            .await
            .map_err(|e| e.to_string())?;
        projections
            .apply_events(&[saved])
            .await
            .map_err(|e| e.to_string())?;
        emitted += 1;
    }
    Ok(emitted)
}
