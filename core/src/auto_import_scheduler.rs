//! Background scheduler for auto-import sources (WS, Wise, IMAP).
//!
//! One tokio task per source — independent intervals, independent backoff.
//! Mirrors the autonomous-tick pattern from `auto_close_scheduler` but lives
//! in core (not tauri-app) because auto-import runs server-side, not on the
//! Tauri client (per `feedback_llm_server_side.md`).
//!
//! Per-tick contract:
//! - `pull()` returns `Ok(ImportSummary)` → reset backoff, sleep for the
//!   configured interval, then tick again.
//! - `pull()` returns `Err(...)` → sleep for the current backoff (1s start,
//!   doubling, capped at 1h), then retry.
//!
//! Real sources plug in by impl-ing `AutoImportSource`. `NullSource` exists
//! for tests + as a placeholder when a real source isn't yet configured.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportSummary {
    /// Number of events the source actually appended in this tick. Zero is
    /// not a failure — it just means "no new data."
    pub events_appended: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("source not configured: {0}")]
    NotConfigured(String),
    #[error("upstream error: {0}")]
    Upstream(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Object-safe — sources can be held as `Arc<dyn AutoImportSource>`.
#[async_trait]
pub trait AutoImportSource: Send + Sync {
    /// Human-readable name shown in tracing + status reports.
    /// (`"wealthsimple-snaptrade"`, `"wise"`, `"imap-standardchartered"`, etc.)
    fn name(&self) -> &str;

    /// Run one import pass. Implementations should be idempotent — re-running
    /// after a partial failure must not duplicate events.
    async fn pull(&self) -> Result<ImportSummary, ImportError>;
}

/// Initial backoff after a failed tick. Doubles per consecutive failure,
/// capped at `MAX_BACKOFF`.
pub const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// Upper bound on backoff between failed ticks. One hour matches the
/// project-wide convention used in `sync::retry`.
pub const MAX_BACKOFF: Duration = Duration::from_secs(3600);

/// Spawn a perpetual scheduler task for one source. Returns immediately; the
/// task lives as long as the tokio runtime. `interval` is the *success-case*
/// inter-tick sleep; failures use exponential backoff instead.
pub fn spawn(source: Arc<dyn AutoImportSource>, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            match source.pull().await {
                Ok(summary) => {
                    tracing::info!(
                        source = source.name(),
                        events = summary.events_appended,
                        "auto-import tick"
                    );
                    backoff = INITIAL_BACKOFF;
                    tokio::time::sleep(interval).await;
                }
                Err(e) => {
                    tracing::warn!(
                        source = source.name(),
                        error = %e,
                        backoff_secs = backoff.as_secs(),
                        "auto-import tick failed"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = next_backoff(backoff);
                }
            }
        }
    })
}

/// Pure helper for backoff progression — testable without spawning tasks.
pub fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_BACKOFF)
}

// ---------------------------------------------------------------------------
// Status registry (Phase 3.9 — observability surface for the Settings panel)
// ---------------------------------------------------------------------------

/// What the source did on its most recent tick. `NotYetRun` is the
/// post-spawn / pre-first-tick state; once a tick completes (success or
/// failure) this transitions and stays in `Success` / `Failure` until the
/// next tick.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TickOutcome {
    NotYetRun,
    Success { events_appended: usize },
    Failure { error: String },
}

/// Per-source observability snapshot. Cloned on read so callers (HTTP route
/// handlers) don't hold the registry lock across awaits.
#[derive(Debug, Clone, Serialize)]
pub struct SourceStatus {
    pub name: String,
    pub last_tick_at: Option<DateTime<Utc>>,
    pub last_outcome: TickOutcome,
    /// Configured success-tick interval (seconds). Exposed so the UI can
    /// reason about "is it overdue?" without re-deriving the policy.
    pub interval_secs: u64,
}

/// Wraps the source impl with its mutable status. Held inside the
/// `SourceRegistry` map.
struct RegisteredSource {
    source: Arc<dyn AutoImportSource>,
    status: SourceStatus,
}

/// Shared map of source-name → `(source, status)`. The scheduler tasks hold
/// a clone and update status on every tick; the server route handlers hold
/// a clone and read status + trigger manual ticks.
#[derive(Clone, Default)]
pub struct SourceRegistry {
    inner: Arc<RwLock<HashMap<String, RegisteredSource>>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a source. Called by `spawn_sources` before spawning
    /// the scheduler task — `spawn_with_registry` then updates status here on
    /// every tick.
    pub async fn register(&self, source: Arc<dyn AutoImportSource>, interval: Duration) {
        let name = source.name().to_string();
        let status = SourceStatus {
            name: name.clone(),
            last_tick_at: None,
            last_outcome: TickOutcome::NotYetRun,
            interval_secs: interval.as_secs(),
        };
        let mut guard = self.inner.write().await;
        guard.insert(name, RegisteredSource { source, status });
    }

    /// Snapshot every registered source's current status. Order is not
    /// guaranteed; the UI sorts.
    pub async fn snapshot(&self) -> Vec<SourceStatus> {
        let guard = self.inner.read().await;
        guard.values().map(|r| r.status.clone()).collect()
    }

    /// Trigger a one-off tick for `name`. Status is updated on completion
    /// just like a scheduled tick. Returns the pulled summary or the error
    /// the source emitted.
    pub async fn trigger_manual(&self, name: &str) -> Result<ImportSummary, ImportError> {
        let source = {
            let guard = self.inner.read().await;
            guard
                .get(name)
                .map(|r| r.source.clone())
                .ok_or_else(|| ImportError::NotConfigured(format!("unknown source: {name}")))?
        };
        let outcome = source.pull().await;
        self.record_tick(name, &outcome).await;
        outcome
    }

    /// Internal: write the result of a tick into the registry. Used by
    /// both scheduled ticks (via `spawn_with_registry`) and manual ticks.
    async fn record_tick(&self, name: &str, outcome: &Result<ImportSummary, ImportError>) {
        let mut guard = self.inner.write().await;
        if let Some(r) = guard.get_mut(name) {
            r.status.last_tick_at = Some(Utc::now());
            r.status.last_outcome = match outcome {
                Ok(s) => TickOutcome::Success {
                    events_appended: s.events_appended,
                },
                Err(e) => TickOutcome::Failure {
                    error: e.to_string(),
                },
            };
        }
    }
}

/// Spawn variant that records each tick's outcome into the registry.
/// Behaviorally identical to `spawn` aside from the status side-effect.
/// Callers using `SourceRegistry` should prefer this over plain `spawn`.
pub fn spawn_with_registry(
    registry: SourceRegistry,
    source: Arc<dyn AutoImportSource>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            let result = source.pull().await;
            registry.record_tick(source.name(), &result).await;
            match result {
                Ok(summary) => {
                    tracing::info!(
                        source = source.name(),
                        events = summary.events_appended,
                        "auto-import tick"
                    );
                    backoff = INITIAL_BACKOFF;
                    tokio::time::sleep(interval).await;
                }
                Err(e) => {
                    tracing::warn!(
                        source = source.name(),
                        error = %e,
                        backoff_secs = backoff.as_secs(),
                        "auto-import tick failed"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = next_backoff(backoff);
                }
            }
        }
    })
}

/// Health classification for a source. `Healthy` = recent success;
/// `Stale` = succeeded once but hasn't ticked in a while; `Degraded` = last
/// tick was a failure; `Unknown` = never ticked. The cutoff between Healthy
/// and Stale is a policy decision — see `classify_source_health`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceHealth {
    Unknown,
    Healthy,
    Stale,
    Degraded,
}

/// Classify a source's health from its current status snapshot.
///
/// Policy: the UI uses this to render the dot/badge next to each source
/// (green/yellow/red/grey). The classification deliberately ignores
/// `events_appended` — zero events is a legitimate Healthy outcome (no new
/// data upstream).
///
/// `now` is injected so tests can pin time without freezing the clock; in
/// production callers pass `Utc::now()`.
pub fn classify_source_health(
    status: &SourceStatus,
    now: DateTime<Utc>,
) -> SourceHealth {
    // Stale cutoff: after 3× the configured interval without a completed
    // tick, a previously-successful source flips Healthy → Stale. Survives
    // one slow/missed tick silently; flags two-in-a-row.
    const STALE_MULTIPLIER: u64 = 3;

    match &status.last_outcome {
        TickOutcome::NotYetRun => SourceHealth::Unknown,
        TickOutcome::Failure { .. } => SourceHealth::Degraded,
        TickOutcome::Success { .. } => match status.last_tick_at {
            None => SourceHealth::Unknown,
            Some(at) => {
                let stale_after = chrono::Duration::seconds(
                    (status.interval_secs * STALE_MULTIPLIER) as i64,
                );
                if now.signed_duration_since(at) > stale_after {
                    SourceHealth::Stale
                } else {
                    SourceHealth::Healthy
                }
            }
        },
    }
}

/// Test-only / placeholder source — returns a configurable result so tests
/// can drive scheduler behavior.
pub mod null {
    use super::*;
    use std::sync::Mutex;

    pub struct NullSource {
        name: String,
        /// Scripted responses popped in order. Each call to `pull()` takes
        /// the front element. When empty, returns `Ok(zero events)` forever.
        scripted: Mutex<std::collections::VecDeque<Result<ImportSummary, ImportError>>>,
        call_count: Mutex<usize>,
    }

    impl NullSource {
        pub fn new(name: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                scripted: Mutex::new(std::collections::VecDeque::new()),
                call_count: Mutex::new(0),
            }
        }

        pub fn with_script(
            mut self,
            script: Vec<Result<ImportSummary, ImportError>>,
        ) -> Self {
            self.scripted = Mutex::new(script.into());
            self
        }

        pub fn call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl AutoImportSource for NullSource {
        fn name(&self) -> &str {
            &self.name
        }

        async fn pull(&self) -> Result<ImportSummary, ImportError> {
            *self.call_count.lock().unwrap() += 1;
            let next = self.scripted.lock().unwrap().pop_front();
            next.unwrap_or(Ok(ImportSummary { events_appended: 0 }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_each_step() {
        let b1 = INITIAL_BACKOFF;
        let b2 = next_backoff(b1);
        let b3 = next_backoff(b2);
        assert_eq!(b2, Duration::from_secs(2));
        assert_eq!(b3, Duration::from_secs(4));
    }

    #[test]
    fn backoff_caps_at_max() {
        let huge = Duration::from_secs(5000);
        assert_eq!(next_backoff(huge), MAX_BACKOFF);
    }

    #[test]
    fn backoff_at_max_stays_at_max() {
        assert_eq!(next_backoff(MAX_BACKOFF), MAX_BACKOFF);
    }

    #[tokio::test]
    async fn null_source_returns_default_when_unscripted() {
        let src = null::NullSource::new("test");
        let result = src.pull().await.unwrap();
        assert_eq!(result.events_appended, 0);
        assert_eq!(src.call_count(), 1);
    }

    #[tokio::test]
    async fn null_source_returns_scripted_sequence() {
        let src = null::NullSource::new("test").with_script(vec![
            Ok(ImportSummary { events_appended: 5 }),
            Err(ImportError::Upstream("oh no".into())),
            Ok(ImportSummary { events_appended: 2 }),
        ]);

        let first = src.pull().await.unwrap();
        assert_eq!(first.events_appended, 5);

        let second = src.pull().await;
        assert!(second.is_err());

        let third = src.pull().await.unwrap();
        assert_eq!(third.events_appended, 2);

        // Past the scripted end → defaults to Ok(0).
        let fourth = src.pull().await.unwrap();
        assert_eq!(fourth.events_appended, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn scheduler_calls_source_repeatedly_on_success() {
        // start_paused lets us advance virtual time without real sleeping.
        let src = Arc::new(
            null::NullSource::new("happy-path").with_script(vec![
                Ok(ImportSummary { events_appended: 1 }),
                Ok(ImportSummary { events_appended: 1 }),
                Ok(ImportSummary { events_appended: 1 }),
            ]),
        );
        let handle = spawn(src.clone(), Duration::from_secs(60));

        // Advance enough virtual time for ~3 ticks at 60s interval.
        tokio::time::sleep(Duration::from_secs(200)).await;
        handle.abort();

        // Don't assert exact count (virtual time + async ordering can land
        // 3 or 4 calls) — assert "ticked more than once."
        assert!(src.call_count() >= 3, "got {} calls", src.call_count());
    }

    // ----- SourceRegistry + classify_source_health -----

    fn status_with(
        outcome: TickOutcome,
        last_tick_at: Option<DateTime<Utc>>,
        interval_secs: u64,
    ) -> SourceStatus {
        SourceStatus {
            name: "test-src".into(),
            last_tick_at,
            last_outcome: outcome,
            interval_secs,
        }
    }

    #[test]
    fn health_unknown_when_never_run() {
        let s = status_with(TickOutcome::NotYetRun, None, 1800);
        assert_eq!(
            classify_source_health(&s, Utc::now()),
            SourceHealth::Unknown,
            "a source that has never ticked should be Unknown, not Healthy"
        );
    }

    #[test]
    fn health_healthy_when_recent_success() {
        let now = Utc::now();
        let s = status_with(
            TickOutcome::Success { events_appended: 0 },
            Some(now - chrono::Duration::seconds(60)),
            1800,
        );
        assert_eq!(
            classify_source_health(&s, now),
            SourceHealth::Healthy,
            "a success 60s ago with a 30min interval should be Healthy — zero events is fine"
        );
    }

    #[test]
    fn health_degraded_when_last_was_failure() {
        let now = Utc::now();
        let s = status_with(
            TickOutcome::Failure {
                error: "oh no".into(),
            },
            Some(now - chrono::Duration::seconds(30)),
            1800,
        );
        assert_eq!(
            classify_source_health(&s, now),
            SourceHealth::Degraded,
            "a failure on the most recent tick is Degraded regardless of how recent"
        );
    }

    #[test]
    fn health_stale_when_success_long_past_interval() {
        let now = Utc::now();
        // Last success was 3 hours ago; interval is 30 minutes. That's
        // 6× the interval — definitely past any reasonable Healthy window.
        let s = status_with(
            TickOutcome::Success { events_appended: 7 },
            Some(now - chrono::Duration::hours(3)),
            1800,
        );
        assert_eq!(
            classify_source_health(&s, now),
            SourceHealth::Stale,
            "a success >> N×interval ago should downgrade to Stale"
        );
    }

    #[tokio::test]
    async fn registry_register_then_snapshot() {
        let registry = SourceRegistry::new();
        let src = Arc::new(null::NullSource::new("snap-src"));
        registry
            .register(src.clone(), Duration::from_secs(60))
            .await;

        let snap = registry.snapshot().await;
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].name, "snap-src");
        assert!(matches!(snap[0].last_outcome, TickOutcome::NotYetRun));
        assert_eq!(snap[0].interval_secs, 60);
    }

    #[tokio::test]
    async fn registry_manual_trigger_updates_status() {
        let registry = SourceRegistry::new();
        let src = Arc::new(
            null::NullSource::new("manual-src")
                .with_script(vec![Ok(ImportSummary { events_appended: 4 })]),
        );
        registry
            .register(src.clone(), Duration::from_secs(60))
            .await;

        let outcome = registry.trigger_manual("manual-src").await.unwrap();
        assert_eq!(outcome.events_appended, 4);
        assert_eq!(src.call_count(), 1);

        let snap = registry.snapshot().await;
        assert_eq!(
            snap[0].last_outcome,
            TickOutcome::Success { events_appended: 4 }
        );
        assert!(snap[0].last_tick_at.is_some());
    }

    #[tokio::test]
    async fn registry_manual_trigger_unknown_source() {
        let registry = SourceRegistry::new();
        let err = registry.trigger_manual("nope").await.unwrap_err();
        assert!(matches!(err, ImportError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn registry_manual_trigger_records_failure() {
        let registry = SourceRegistry::new();
        let src = Arc::new(
            null::NullSource::new("flaky-src")
                .with_script(vec![Err(ImportError::Upstream("upstream down".into()))]),
        );
        registry
            .register(src.clone(), Duration::from_secs(60))
            .await;

        let err = registry.trigger_manual("flaky-src").await.unwrap_err();
        assert!(matches!(err, ImportError::Upstream(_)));

        let snap = registry.snapshot().await;
        assert!(matches!(snap[0].last_outcome, TickOutcome::Failure { .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn scheduler_applies_backoff_on_failure() {
        let src = Arc::new(null::NullSource::new("flaky").with_script(vec![
            Err(ImportError::Upstream("1".into())),
            Err(ImportError::Upstream("2".into())),
            Err(ImportError::Upstream("3".into())),
        ]));
        let handle = spawn(src.clone(), Duration::from_secs(60));

        // After 1s + 2s + 4s = 7s of virtual time → all 3 failure ticks fired.
        // Use 8s to give headroom.
        tokio::time::sleep(Duration::from_secs(8)).await;
        handle.abort();

        assert!(src.call_count() >= 3, "got {} calls", src.call_count());
    }
}
