//! Background scheduler that wakes around local midnight and invokes
//! `omni_me_core::auto_close::auto_close_stale_journals`.
//!
//! Timing: on start, sleep until `next_local_midnight + GRACE_SECONDS`. After
//! a tick fires, sleep until the next local midnight. If the timezone string
//! can't be parsed, fall back to UTC and log a warning — the app should never
//! panic on a bad timezone setting.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use tokio::sync::RwLock;

use omni_me_core::auto_close::auto_close_stale_journals;
use omni_me_core::db::Database;
use omni_me_core::events::{ProjectionRunner, SurrealEventStore};

/// Offset past midnight before the tick fires — avoids race with any
/// midnight-adjacent write.
const GRACE_SECONDS: i64 = 30;

/// Spawn the auto-close scheduler on the Tauri async runtime. Returns
/// immediately; the task lives as long as the runtime.
pub fn spawn(
    db: Database,
    event_store: SurrealEventStore,
    projections: ProjectionRunner,
    device_id: String,
    timezone: Arc<RwLock<String>>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let tz_name = timezone.read().await.clone();
            let now_utc = Utc::now();
            let sleep_for = duration_until_next_tick(&tz_name, now_utc, GRACE_SECONDS);

            tracing::debug!(
                tz = %tz_name,
                sleep_secs = sleep_for.as_secs(),
                "auto-close scheduler: sleeping until next local midnight",
            );
            tokio::time::sleep(sleep_for).await;

            let tz_name = timezone.read().await.clone();
            let today = today_in_tz(&tz_name, Utc::now());
            match auto_close_stale_journals(&db, &event_store, &projections, &device_id, today).await
            {
                Ok(0) => tracing::debug!("auto-close: no stale journals"),
                Ok(n) => tracing::info!(closed = n, "auto-close: closed stale journals"),
                Err(e) => tracing::warn!(error = %e, "auto-close: tick failed"),
            }
        }
    });
}

fn parse_tz(tz_name: &str) -> Tz {
    tz_name.parse().unwrap_or_else(|_| {
        tracing::warn!(tz = %tz_name, "failed to parse timezone, falling back to UTC");
        Tz::UTC
    })
}

fn today_in_tz(tz_name: &str, now_utc: DateTime<Utc>) -> chrono::NaiveDate {
    let tz = parse_tz(tz_name);
    now_utc.with_timezone(&tz).date_naive()
}

fn duration_until_next_tick(
    tz_name: &str,
    now_utc: DateTime<Utc>,
    grace_seconds: i64,
) -> Duration {
    let tz = parse_tz(tz_name);
    let local_now = now_utc.with_timezone(&tz);
    let tomorrow = local_now.date_naive().succ_opt().unwrap_or(local_now.date_naive());

    // Midnight in the local zone.
    let midnight = tomorrow
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always valid");
    // `.earliest()` handles both `Single` (the normal case) and `Ambiguous`
    // (DST fall-back overlap, where 00:00 happens twice — pick the earlier).
    // `None` is the gap case (DST spring-forward at midnight) and is
    // essentially impossible in modern zones, since transitions are at 02:00
    // not 00:00. If it ever fires, we skip today entirely and try again in
    // 24h — auto-close just runs a day late, which is benign because
    // closing a journal is a fully reversible single-button action.
    let midnight_local = match tz.from_local_datetime(&midnight).earliest() {
        Some(dt) => dt,
        None => return Duration::from_secs(24 * 60 * 60),
    };
    let target_utc = midnight_local.with_timezone(&Utc) + chrono::Duration::seconds(grace_seconds);
    let delta = target_utc.signed_duration_since(now_utc);
    delta
        .to_std()
        .unwrap_or_else(|_| Duration::from_secs(60 * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_rolls_to_next_day_past_midnight() {
        // 2026-04-19 23:00:00 UTC, tz = UTC → next tick is 2026-04-20 00:00:30 UTC
        let now = chrono::DateTime::parse_from_rfc3339("2026-04-19T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d = duration_until_next_tick("UTC", now, 30);
        assert_eq!(d.as_secs(), 60 * 60 + 30);
    }

    #[test]
    fn bad_timezone_falls_back_to_utc() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-04-19T23:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d = duration_until_next_tick("Not/A/Zone", now, 30);
        assert_eq!(
            d.as_secs(),
            60 * 60 + 30,
            "falls back to UTC so the schedule still ticks",
        );
    }

    #[test]
    fn today_in_tz_uses_local_calendar_date() {
        // 2026-04-19 23:30:00 UTC is already 2026-04-20 in Tokyo.
        let now = chrono::DateTime::parse_from_rfc3339("2026-04-19T23:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let today = today_in_tz("Asia/Tokyo", now);
        assert_eq!(today.to_string(), "2026-04-20");
    }
}
