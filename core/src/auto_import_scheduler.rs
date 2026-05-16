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
use std::sync::Arc;
use std::time::Duration;

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
