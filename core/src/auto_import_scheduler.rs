//! Background scheduler for auto-import sources (Northwind, Globepay, IMAP).
//!
//! One tokio task per source — independent intervals, independent backoff.
//! Mirrors the autonomous-tick pattern from `auto_close_scheduler` but lives
//! in core (not tauri-app) because auto-import runs server-side, not on the
//! Tauri client (per `feedback_llm_server_side.md`).
//!
//! Per-tick contract:
//! - `pull()` returns `Ok(ImportSummary)` → reset backoff, sleep for the
//!   configured interval, then tick again.
//! - `pull()` returns `Err(ImportError::NeedsReauth(_))` → **halt this source's
//!   loop** (circuit-breaker). The credential is expired and only the user can
//!   fix it, so re-`pull()`-ing on a schedule would just re-attempt login on
//!   every tick — for a bank source that means repeated real login attempts
//!   (lockout / fraud-flag risk). The source goes dormant in `NeedsReauth`; a
//!   successful `Reconnect` (`SourceRegistry::reauth`) re-arms the loop.
//! - `pull()` returns any other `Err(...)` → sleep for the current backoff
//!   (1s start, doubling, capped at 1h), then retry. Transient upstream blips
//!   are worth retrying; an expired credential is not.
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
    /// The source's stored credential expired/was rejected and it needs an
    /// interactive re-auth (the user supplies an OTP via the Reconnect flow).
    /// Distinct from `Upstream` so the registry can flip the source to
    /// `AuthState::NeedsReauth` instead of treating it as a transient blip —
    /// the difference between "user must act" and "wait it out".
    #[error("needs re-auth: {0}")]
    NeedsReauth(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Whether a source can refresh an expired credential interactively, and — once
/// it can't import — whether it's waiting on the user. Serialized into
/// `GET /auto_import/status` so the client can render a "Reconnect {source}"
/// affordance. Tagged `kind` to match `TickOutcome`'s wire shape.
///
/// `AwaitingOtp` (the provider-sends-a-code two-step) is intentionally absent:
/// the one real consumer authenticates with TOTP, which collapses re-auth to a
/// single `submit` (see `SOURCE_REAUTH_DESIGN.md`). Adding it later is a
/// non-breaking enum extension if an SMS/email-2FA source ever appears.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthState {
    /// Credential is believed good (the default, and where a successful tick
    /// returns the source).
    #[default]
    Active,
    /// Credential expired/invalid; auto-import is paused for this source until
    /// the user reconnects. `reason` is human-readable for the status surface.
    NeedsReauth { reason: String },
}

/// Outcome of an interactive re-auth attempt. Serialized verbatim as the
/// `POST /auto_import/reauth` response body (`{"status":"active"}`, …), and the
/// return of [`AutoImportSource::reauth`]. Tagged `status` to read naturally in
/// the client.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReauthOutcome {
    /// Login succeeded; credential refreshed + persisted server-side.
    Active,
    /// The supplied code was rejected — the user can try again.
    InvalidOtp,
    /// This source has no interactive re-auth (the trait default). A caller
    /// reaching this hit the route for a source that can't reconnect.
    NotSupported,
    /// Anything else went wrong; `message` carries detail.
    Error { message: String },
}

/// Object-safe — sources can be held as `Arc<dyn AutoImportSource>`.
#[async_trait]
pub trait AutoImportSource: Send + Sync {
    /// Human-readable name shown in tracing + status reports.
    /// (`"northwind-sync"`, `"globepay"`, `"imap-meridian"`, etc.)
    fn name(&self) -> &str;

    /// Run one import pass. Implementations should be idempotent — re-running
    /// after a partial failure must not duplicate events.
    async fn pull(&self) -> Result<ImportSummary, ImportError>;

    /// This source's preferred success-case poll interval, if it declares one.
    /// Default `None` → the source inherits the engine's global interval (set at
    /// the composition root). Config-declared sources (CSV / subprocess) override
    /// this with their `schedule_secs`, since for them the cadence *is* part of
    /// the declaration. Bank adapters in the private overlay don't implement it
    /// and keep the global default — so adding this method is non-breaking.
    fn poll_interval(&self) -> Option<Duration> {
        None
    }

    /// Whether this source supports interactive re-auth (the Reconnect flow).
    /// Default `false`: most sources hold a stable credential and never need a
    /// user-supplied OTP. Surfaced in the status snapshot so the client knows
    /// which sources *can* be reconnected.
    fn reauth_capable(&self) -> bool {
        false
    }

    /// Refresh an expired credential with a single-use `otp`. Default
    /// `NotSupported` — a non-reauth source reaching this means a caller hit the
    /// reauth route for the wrong source. Implementors persist the refreshed
    /// credential themselves (server-side); the engine only relays the code.
    async fn reauth(&self, _otp: &str) -> ReauthOutcome {
        ReauthOutcome::NotSupported
    }
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
                Err(ImportError::NeedsReauth(reason)) => {
                    // Circuit-breaker: an expired credential won't fix itself by
                    // retrying — each retry re-attempts login. Halt the loop and
                    // leave the source dormant until the user reconnects.
                    tracing::warn!(
                        source = source.name(),
                        reason = %reason,
                        "auto-import source needs re-auth — halting its scheduler (no login-retry loop)"
                    );
                    break;
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
    /// Whether the credential is good or the user must reconnect. Independent
    /// of `last_outcome`: a `NeedsReauth` source is actionable by the user,
    /// whereas a transient `Failure` is just "wait it out".
    pub auth_state: AuthState,
    /// Whether this source exposes the Reconnect flow at all (`reauth_capable`
    /// on the source). The client only renders a Reconnect affordance for
    /// capable sources.
    pub reauth_capable: bool,
    /// Whether the user has *paused* this source — its scheduler loop is
    /// live-aborted and it will not auto-poll until resumed. Distinct from
    /// `NeedsReauth` (the source *wants* to run but can't) and from a config
    /// `enabled=false` (which prevents the source from ever spawning): a paused
    /// source keeps its config + registry entry, it's just switched off at
    /// runtime. Persisted server-side so a pause survives a restart (see
    /// `auto_import::paused`) — critical for stopping a runaway bank source.
    #[serde(default)]
    pub paused: bool,
}

/// Wraps the source impl with its mutable status. Held inside the
/// `SourceRegistry` map.
struct RegisteredSource {
    source: Arc<dyn AutoImportSource>,
    status: SourceStatus,
    /// Handle to the perpetual scheduler task, when this source was spawned via
    /// [`SourceRegistry::spawn_one`]. `None` for status-only registrations (the
    /// `register` path used by tests). Owning the handle here is what makes
    /// [`SourceRegistry::remove`] able to *abort* a running source live —
    /// dropping a `JoinHandle` only detaches, so removal must abort explicitly.
    task: Option<tokio::task::JoinHandle<()>>,
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
            // A fresh source is assumed good until a tick proves otherwise; the
            // first `needs_reauth` tick flips this.
            auth_state: AuthState::Active,
            reauth_capable: source.reauth_capable(),
            // A freshly-registered source is running; a persisted pause is
            // re-applied by the composition root *after* spawn (see
            // `SourceRegistry::pause`), so registration itself never starts
            // paused.
            paused: false,
        };
        let mut guard = self.inner.write().await;
        guard.insert(
            name,
            RegisteredSource {
                source,
                status,
                task: None,
            },
        );
    }

    /// Register a source, spawn its perpetual scheduler task, and store the task
    /// handle so the source can be torn down live via [`remove`]. The effective
    /// interval is the source's own [`AutoImportSource::poll_interval`] if it
    /// declares one, else `default_interval`.
    ///
    /// Replaces cleanly: any prior source of the same name is aborted + dropped
    /// first, so a live re-add (an edit) never leaks a detached task still
    /// polling the old config. The entry is inserted *before* the task spawns so
    /// the first `record_tick` lookup finds it; the handle is written back in a
    /// second lock acquisition (the `task` field isn't read during ticks).
    pub async fn spawn_one(&self, source: Arc<dyn AutoImportSource>, default_interval: Duration) {
        let interval = source.poll_interval().unwrap_or(default_interval);
        let name = source.name().to_string();
        self.remove(&name).await;
        self.register(source.clone(), interval).await;
        let handle = spawn_with_registry(self.clone(), source, interval);
        let mut guard = self.inner.write().await;
        if let Some(r) = guard.get_mut(&name) {
            r.task = Some(handle);
        }
    }

    /// Tear down a source live: abort its scheduler task and drop it from the
    /// registry. Returns whether a source by that name existed. Aborting is
    /// explicit because dropping a `JoinHandle` only detaches the task — it
    /// would keep polling otherwise.
    pub async fn remove(&self, name: &str) -> bool {
        let mut guard = self.inner.write().await;
        match guard.remove(name) {
            Some(r) => {
                if let Some(h) = r.task {
                    h.abort();
                }
                true
            }
            None => false,
        }
    }

    /// Pause a source live: abort its scheduler task but **keep** its registry
    /// entry (and thus its config). Returns whether a source by that name was
    /// registered. This is the runtime off-switch (#367) — for a compiled
    /// overlay bank source it's the only way to stop it from the app, and for a
    /// config source it stops polling without touching `sources.toml`.
    ///
    /// The handle is dropped (set to `None`) rather than kept-aborted on
    /// purpose: `rearm_if_dormant` treats a *finished* handle as "resume me",
    /// so leaving an aborted handle here would let a later Reconnect silently
    /// un-pause the source. `None` means "no loop, and none wanted" — a
    /// successful reauth clears `auth_state` but won't spawn a loop for a paused
    /// source. [`resume`] is the only path back to running.
    pub async fn pause(&self, name: &str) -> bool {
        let mut guard = self.inner.write().await;
        match guard.get_mut(name) {
            Some(r) => {
                if let Some(h) = r.task.take() {
                    h.abort();
                }
                r.status.paused = true;
                true
            }
            None => false,
        }
    }

    /// Resume a paused source: re-spawn its scheduler loop and clear the paused
    /// flag. Returns whether a source by that name was registered. A no-op
    /// (still `true`) if the source exists but isn't paused, so a double-resume
    /// never spawns a second polling task. The re-spawned loop pulls immediately
    /// on its first iteration, so resume also yields a fresh fetch.
    pub async fn resume(&self, name: &str) -> bool {
        let mut guard = self.inner.write().await;
        match guard.get_mut(name) {
            Some(r) if r.status.paused => {
                let interval = Duration::from_secs(r.status.interval_secs);
                let handle = spawn_with_registry(self.clone(), r.source.clone(), interval);
                r.task = Some(handle);
                r.status.paused = false;
                true
            }
            Some(_) => true, // exists but wasn't paused — nothing to do
            None => false,
        }
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

    /// Drive interactive re-auth for `name` with a single-use `otp`. Clones the
    /// source under the read lock (like `trigger_manual`), runs its `reauth`,
    /// and — on success — flips the stored `auth_state` back to `Active` so the
    /// status surface clears the Reconnect prompt without waiting for the next
    /// scheduled tick. `Err(NotConfigured)` only for an unknown source name;
    /// `invalid_otp` / `not_supported` are normal `Ok` outcomes the caller
    /// renders, not transport errors.
    pub async fn reauth(&self, name: &str, otp: &str) -> Result<ReauthOutcome, ImportError> {
        let source = {
            let guard = self.inner.read().await;
            guard
                .get(name)
                .map(|r| r.source.clone())
                .ok_or_else(|| ImportError::NotConfigured(format!("unknown source: {name}")))?
        };
        let outcome = source.reauth(otp).await;
        if matches!(outcome, ReauthOutcome::Active) {
            {
                let mut guard = self.inner.write().await;
                if let Some(r) = guard.get_mut(name) {
                    r.status.auth_state = AuthState::Active;
                }
            }
            // A source that hit `NeedsReauth` halted its scheduler loop (the
            // circuit-breaker). A successful Reconnect must resume it, or the
            // source would sit `Active`-but-idle until the next server restart.
            self.rearm_if_dormant(name).await;
        }
        Ok(outcome)
    }

    /// Re-spawn a source's scheduler loop, but only if it previously had one that
    /// has since exited (the `NeedsReauth` circuit-breaker halts the loop). A
    /// source that never had a loop (status-only `register`, used by tests) or
    /// whose loop is still running is left untouched — so this never
    /// double-spawns a healthy source into a second polling task.
    async fn rearm_if_dormant(&self, name: &str) {
        let mut guard = self.inner.write().await;
        if let Some(r) = guard.get_mut(name) {
            let dormant = matches!(&r.task, Some(h) if h.is_finished());
            if dormant {
                // Reuse the interval the source was originally spawned with
                // (already resolves the per-source `poll_interval` override).
                let interval = Duration::from_secs(r.status.interval_secs);
                let handle = spawn_with_registry(self.clone(), r.source.clone(), interval);
                r.task = Some(handle);
            }
        }
    }

    /// Internal: write the result of a tick into the registry. Used by
    /// both scheduled ticks (via `spawn_with_registry`) and manual ticks.
    async fn record_tick(&self, name: &str, outcome: &Result<ImportSummary, ImportError>) {
        let mut guard = self.inner.write().await;
        if let Some(r) = guard.get_mut(name) {
            r.status.last_tick_at = Some(Utc::now());
            match outcome {
                Ok(s) => {
                    r.status.last_outcome = TickOutcome::Success {
                        events_appended: s.events_appended,
                    };
                    // A clean tick means the credential works — clear any prior
                    // reconnect requirement (covers a local re-prime too).
                    r.status.auth_state = AuthState::Active;
                }
                Err(ImportError::NeedsReauth(reason)) => {
                    r.status.last_outcome = TickOutcome::Failure {
                        error: reason.clone(),
                    };
                    // The actionable case: surface it as state, not just a log.
                    r.status.auth_state = AuthState::NeedsReauth {
                        reason: reason.clone(),
                    };
                }
                Err(e) => {
                    r.status.last_outcome = TickOutcome::Failure {
                        error: e.to_string(),
                    };
                    // Leave auth_state untouched: a transient upstream blip must
                    // not masquerade as a reconnect prompt, and an already-
                    // NeedsReauth source stays that way until a real refresh.
                }
            }
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
                Err(ImportError::NeedsReauth(reason)) => {
                    // Circuit-breaker: `record_tick` above already flipped the
                    // status to `NeedsReauth`. Halting the loop (rather than
                    // retrying with backoff) is what stops a bank source from
                    // re-attempting login every tick while it waits for the
                    // user's OTP. `SourceRegistry::reauth` re-arms the loop on a
                    // successful Reconnect.
                    tracing::warn!(
                        source = source.name(),
                        reason = %reason,
                        "auto-import source needs re-auth — halting its scheduler (no login-retry loop); resumes on Reconnect"
                    );
                    break;
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
        /// Scripted `reauth` outcome. `Some` also makes the source report
        /// `reauth_capable`, so a plain `NullSource` stays non-reauth (testing
        /// the trait defaults) while `.with_reauth(...)` opts in.
        scripted_reauth: Mutex<Option<ReauthOutcome>>,
        /// Optional per-source poll interval override (read-only after build).
        /// `None` keeps the trait default so a plain `NullSource` inherits the
        /// global interval; `.with_poll_interval(...)` exercises the override.
        poll_interval: Option<Duration>,
    }

    impl NullSource {
        pub fn new(name: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                scripted: Mutex::new(std::collections::VecDeque::new()),
                call_count: Mutex::new(0),
                scripted_reauth: Mutex::new(None),
                poll_interval: None,
            }
        }

        /// Declare a per-source poll interval (overrides the global default in
        /// `spawn_one`).
        pub fn with_poll_interval(mut self, interval: Duration) -> Self {
            self.poll_interval = Some(interval);
            self
        }

        pub fn with_script(
            mut self,
            script: Vec<Result<ImportSummary, ImportError>>,
        ) -> Self {
            self.scripted = Mutex::new(script.into());
            self
        }

        /// Make this source reauth-capable and script the outcome its `reauth`
        /// returns.
        pub fn with_reauth(mut self, outcome: ReauthOutcome) -> Self {
            self.scripted_reauth = Mutex::new(Some(outcome));
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

        fn poll_interval(&self) -> Option<Duration> {
            self.poll_interval
        }

        fn reauth_capable(&self) -> bool {
            self.scripted_reauth.lock().unwrap().is_some()
        }

        async fn reauth(&self, _otp: &str) -> ReauthOutcome {
            self.scripted_reauth
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(ReauthOutcome::NotSupported)
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
            auth_state: AuthState::Active,
            reauth_capable: false,
            paused: false,
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

    // ----- Live lifecycle: spawn_one / remove (3.7 fast-follow) -----

    #[tokio::test]
    async fn spawn_one_then_remove() {
        let registry = SourceRegistry::new();
        registry
            .spawn_one(
                Arc::new(null::NullSource::new("live")),
                Duration::from_secs(60),
            )
            .await;
        assert_eq!(registry.snapshot().await.len(), 1);

        assert!(registry.remove("live").await, "remove reports it existed");
        assert_eq!(registry.snapshot().await.len(), 0);
        assert!(
            !registry.remove("live").await,
            "a second remove finds nothing"
        );
    }

    #[tokio::test]
    async fn spawn_one_honors_source_poll_interval() {
        let registry = SourceRegistry::new();
        // The source declares 120s; the global default is 1800s — the source's
        // own interval wins and is what the status surface records.
        let src =
            Arc::new(null::NullSource::new("fast").with_poll_interval(Duration::from_secs(120)));
        registry.spawn_one(src, Duration::from_secs(1800)).await;

        let snap = registry.snapshot().await;
        assert_eq!(snap[0].interval_secs, 120);
        registry.remove("fast").await;
    }

    #[tokio::test]
    async fn spawn_one_without_override_uses_default_interval() {
        let registry = SourceRegistry::new();
        registry
            .spawn_one(
                Arc::new(null::NullSource::new("plain")),
                Duration::from_secs(900),
            )
            .await;
        assert_eq!(registry.snapshot().await[0].interval_secs, 900);
        registry.remove("plain").await;
    }

    #[tokio::test]
    async fn spawn_one_replaces_existing_by_name() {
        let registry = SourceRegistry::new();
        registry
            .spawn_one(
                Arc::new(null::NullSource::new("dup")),
                Duration::from_secs(60),
            )
            .await;
        // Re-adding the same name (an edit) replaces rather than duplicates.
        registry
            .spawn_one(
                Arc::new(null::NullSource::new("dup").with_poll_interval(Duration::from_secs(300))),
                Duration::from_secs(60),
            )
            .await;
        let snap = registry.snapshot().await;
        assert_eq!(snap.len(), 1, "re-add replaced, not duplicated");
        assert_eq!(snap[0].interval_secs, 300, "the new config took effect");
        registry.remove("dup").await;
    }

    // ----- Pause / resume off-switch (#367) -----

    #[tokio::test(start_paused = true)]
    async fn pause_aborts_the_loop_but_keeps_the_entry() {
        // A healthy, freely-ticking source. Pausing must stop it from polling
        // AND leave it in the registry (config preserved), flagged paused.
        let registry = SourceRegistry::new();
        let src = Arc::new(null::NullSource::new("bank"));
        registry
            .spawn_one(src.clone(), Duration::from_secs(60))
            .await;

        // Let it tick a couple of times, then pause.
        tokio::time::sleep(Duration::from_secs(130)).await;
        let before = src.call_count();
        assert!(before >= 2, "should have ticked before pause; got {before}");

        assert!(registry.pause("bank").await, "pause reports it existed");
        let snap = registry.snapshot().await;
        assert_eq!(snap.len(), 1, "paused source stays registered (config kept)");
        assert!(snap[0].paused, "status reflects the pause");

        // No further ticks after pause, however long we wait.
        tokio::time::sleep(Duration::from_secs(600)).await;
        assert_eq!(
            src.call_count(),
            before,
            "paused source kept polling — the loop wasn't aborted"
        );
        registry.remove("bank").await;
    }

    #[tokio::test]
    async fn pause_unknown_source_reports_false() {
        let registry = SourceRegistry::new();
        assert!(!registry.pause("ghost").await);
        assert!(!registry.resume("ghost").await);
    }

    #[tokio::test(start_paused = true)]
    async fn resume_respawns_and_clears_the_flag() {
        let registry = SourceRegistry::new();
        let src = Arc::new(null::NullSource::new("bank"));
        registry
            .spawn_one(src.clone(), Duration::from_secs(60))
            .await;
        registry.pause("bank").await;
        let paused_at = src.call_count();

        // Resume re-arms the loop (which pulls immediately) and clears the flag.
        assert!(registry.resume("bank").await);
        assert!(!registry.snapshot().await[0].paused, "flag cleared on resume");

        tokio::time::sleep(Duration::from_secs(200)).await; // ~3 ticks @ 60s
        assert!(
            src.call_count() > paused_at,
            "resume didn't restart polling; count stuck at {paused_at}"
        );
        registry.remove("bank").await;
    }

    #[tokio::test]
    async fn resume_on_running_source_is_a_noop_not_a_double_spawn() {
        // Resuming a source that isn't paused must not spawn a second loop.
        let registry = SourceRegistry::new();
        registry
            .spawn_one(
                Arc::new(null::NullSource::new("live")),
                Duration::from_secs(60),
            )
            .await;
        assert!(registry.resume("live").await, "exists → true");
        assert!(!registry.snapshot().await[0].paused);
        registry.remove("live").await;
    }

    #[tokio::test(start_paused = true)]
    async fn reauth_does_not_unpause_a_paused_source() {
        // A source can be paused while also credential-expired. A successful
        // Reconnect clears auth_state but must NOT resurrect the scheduler loop
        // of a source the user deliberately paused — only `resume` does that.
        let registry = SourceRegistry::new();
        let src = Arc::new(
            null::NullSource::new("bank")
                .with_script(vec![Err(ImportError::NeedsReauth("expired".into()))])
                .with_reauth(ReauthOutcome::Active),
        );
        registry
            .spawn_one(src.clone(), Duration::from_secs(60))
            .await;

        // First tick → NeedsReauth halts the loop; then the user pauses.
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(registry.pause("bank").await);
        let count_at_pause = src.call_count();

        // Reconnect succeeds → auth_state Active, but paused stays true and no
        // loop is spawned.
        let outcome = registry.reauth("bank", "123456").await.unwrap();
        assert_eq!(outcome, ReauthOutcome::Active);
        let snap = registry.snapshot().await;
        assert_eq!(snap[0].auth_state, AuthState::Active);
        assert!(snap[0].paused, "reauth must not clear the user's pause");

        tokio::time::sleep(Duration::from_secs(600)).await;
        assert_eq!(
            src.call_count(),
            count_at_pause,
            "reauth silently un-paused the source (spawned a loop)"
        );
        registry.remove("bank").await;
    }

    // ----- AuthState tracking + reauth (3.5a) -----

    #[tokio::test]
    async fn needs_reauth_tick_sets_auth_state() {
        let registry = SourceRegistry::new();
        let src = Arc::new(null::NullSource::new("northwind").with_script(vec![Err(
            ImportError::NeedsReauth("session expired".into()),
        )]));
        registry.register(src, Duration::from_secs(60)).await;

        let _ = registry.trigger_manual("northwind").await; // returns the Err; we read state
        let snap = registry.snapshot().await;
        // The needs-reauth signal surfaces as actionable state, AND as a
        // (degraded) tick outcome — both, by design.
        assert_eq!(
            snap[0].auth_state,
            AuthState::NeedsReauth {
                reason: "session expired".into()
            }
        );
        assert!(matches!(snap[0].last_outcome, TickOutcome::Failure { .. }));
    }

    #[tokio::test]
    async fn successful_tick_clears_needs_reauth() {
        let registry = SourceRegistry::new();
        // First tick needs reauth; second tick succeeds (as if reauth happened
        // out of band / a local re-prime).
        let src = Arc::new(null::NullSource::new("northwind").with_script(vec![
            Err(ImportError::NeedsReauth("expired".into())),
            Ok(ImportSummary { events_appended: 3 }),
        ]));
        registry.register(src, Duration::from_secs(60)).await;

        let _ = registry.trigger_manual("northwind").await;
        assert!(matches!(
            registry.snapshot().await[0].auth_state,
            AuthState::NeedsReauth { .. }
        ));

        registry.trigger_manual("northwind").await.unwrap();
        assert_eq!(registry.snapshot().await[0].auth_state, AuthState::Active);
    }

    #[tokio::test]
    async fn transient_failure_leaves_needs_reauth_intact() {
        let registry = SourceRegistry::new();
        // Needs reauth, then a *transient* upstream blip. The blip must not
        // clear the reconnect requirement (still NeedsReauth).
        let src = Arc::new(null::NullSource::new("northwind").with_script(vec![
            Err(ImportError::NeedsReauth("expired".into())),
            Err(ImportError::Upstream("502 from upstream".into())),
        ]));
        registry.register(src, Duration::from_secs(60)).await;

        let _ = registry.trigger_manual("northwind").await;
        let _ = registry.trigger_manual("northwind").await;
        assert!(
            matches!(
                registry.snapshot().await[0].auth_state,
                AuthState::NeedsReauth { .. }
            ),
            "a transient failure should not clear an existing NeedsReauth state"
        );
    }

    #[tokio::test]
    async fn registry_reauth_active_clears_state() {
        let registry = SourceRegistry::new();
        // Source goes NeedsReauth on its tick, then its scripted reauth succeeds.
        let src = Arc::new(
            null::NullSource::new("northwind")
                .with_script(vec![Err(ImportError::NeedsReauth("expired".into()))])
                .with_reauth(ReauthOutcome::Active),
        );
        registry.register(src, Duration::from_secs(60)).await;

        let _ = registry.trigger_manual("northwind").await;
        assert!(matches!(
            registry.snapshot().await[0].auth_state,
            AuthState::NeedsReauth { .. }
        ));

        let outcome = registry.reauth("northwind", "123456").await.unwrap();
        assert_eq!(outcome, ReauthOutcome::Active);
        // A successful reauth flips state back without waiting for the next tick.
        assert_eq!(registry.snapshot().await[0].auth_state, AuthState::Active);
    }

    #[tokio::test]
    async fn registry_reauth_invalid_otp_keeps_state() {
        let registry = SourceRegistry::new();
        let src = Arc::new(
            null::NullSource::new("northwind")
                .with_script(vec![Err(ImportError::NeedsReauth("expired".into()))])
                .with_reauth(ReauthOutcome::InvalidOtp),
        );
        registry.register(src, Duration::from_secs(60)).await;

        let _ = registry.trigger_manual("northwind").await;
        let outcome = registry.reauth("northwind", "000000").await.unwrap();
        assert_eq!(outcome, ReauthOutcome::InvalidOtp);
        // A rejected code leaves the source still needing reconnect.
        assert!(matches!(
            registry.snapshot().await[0].auth_state,
            AuthState::NeedsReauth { .. }
        ));
    }

    #[tokio::test]
    async fn registry_reauth_unknown_source_is_not_configured() {
        let registry = SourceRegistry::new();
        let err = registry.reauth("nope", "123456").await.unwrap_err();
        assert!(matches!(err, ImportError::NotConfigured(_)));
    }

    #[tokio::test]
    async fn registry_reauth_non_capable_source_is_not_supported() {
        let registry = SourceRegistry::new();
        // Plain NullSource (no scripted reauth) is not reauth-capable.
        let src = Arc::new(null::NullSource::new("globepay"));
        registry.register(src, Duration::from_secs(60)).await;

        let outcome = registry.reauth("globepay", "123456").await.unwrap();
        assert_eq!(outcome, ReauthOutcome::NotSupported);
    }

    #[tokio::test]
    async fn register_records_reauth_capability() {
        let registry = SourceRegistry::new();
        registry
            .register(
                Arc::new(null::NullSource::new("globepay")),
                Duration::from_secs(60),
            )
            .await;
        registry
            .register(
                Arc::new(
                    null::NullSource::new("northwind").with_reauth(ReauthOutcome::Active),
                ),
                Duration::from_secs(60),
            )
            .await;

        let snap = registry.snapshot().await;
        let globepay = snap.iter().find(|s| s.name == "globepay").unwrap();
        let northwind = snap.iter().find(|s| s.name == "northwind").unwrap();
        assert!(!globepay.reauth_capable, "globepay has no interactive reauth");
        assert!(northwind.reauth_capable, "northwind is reauth-capable");
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

    // ----- Circuit-breaker: NeedsReauth halts the loop (no login-hammer) -----

    #[tokio::test(start_paused = true)]
    async fn needs_reauth_halts_the_scheduler_loop() {
        // A source awaiting OTP must NOT keep re-attempting login on a schedule
        // (bank lockout / fraud-flag risk). The loop halts after the first
        // NeedsReauth tick instead of backing off and retrying forever.
        let registry = SourceRegistry::new();
        let src = Arc::new(
            null::NullSource::new("ws")
                .with_script(vec![Err(ImportError::NeedsReauth("otp".into()))]),
        );
        registry
            .spawn_one(src.clone(), Duration::from_secs(60))
            .await;

        // Advance far past many would-be ticks. A retrying loop would exhaust
        // the script and climb call_count well past 1.
        tokio::time::sleep(Duration::from_secs(3600)).await;

        assert_eq!(
            src.call_count(),
            1,
            "scheduler kept polling after NeedsReauth (login-hammer); should have halted"
        );
        assert_eq!(
            registry.snapshot().await[0].auth_state,
            AuthState::NeedsReauth {
                reason: "otp".into()
            }
        );
        registry.remove("ws").await;
    }

    #[tokio::test(start_paused = true)]
    async fn successful_reauth_rearms_the_scheduler() {
        // After the circuit-breaker halts a source, a successful Reconnect must
        // resume auto-polling — not leave it Active-but-idle until a restart.
        let registry = SourceRegistry::new();
        let src = Arc::new(
            null::NullSource::new("ws")
                .with_script(vec![Err(ImportError::NeedsReauth("otp".into()))])
                .with_reauth(ReauthOutcome::Active),
        );
        registry
            .spawn_one(src.clone(), Duration::from_secs(60))
            .await;

        // First tick hits NeedsReauth and halts the loop.
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(src.call_count(), 1, "loop should have halted after NeedsReauth");

        // Reconnect succeeds → re-arms the loop, which resumes polling (the
        // script is now exhausted, so further pulls return Ok(0)).
        let outcome = registry.reauth("ws", "123456").await.unwrap();
        assert_eq!(outcome, ReauthOutcome::Active);
        assert_eq!(registry.snapshot().await[0].auth_state, AuthState::Active);

        tokio::time::sleep(Duration::from_secs(200)).await; // ~3 ticks @ 60s
        assert!(
            src.call_count() >= 2,
            "reauth should have resumed polling; got {} calls",
            src.call_count()
        );
        registry.remove("ws").await;
    }

    #[tokio::test(start_paused = true)]
    async fn transient_failures_still_retry_with_backoff() {
        // The circuit-breaker is scoped to NeedsReauth only — a transient
        // upstream blip must still retry (this is the pre-existing behavior we
        // must not regress).
        let registry = SourceRegistry::new();
        let src = Arc::new(null::NullSource::new("flaky").with_script(vec![
            Err(ImportError::Upstream("502".into())),
            Err(ImportError::Upstream("502".into())),
            Err(ImportError::Upstream("502".into())),
        ]));
        registry
            .spawn_one(src.clone(), Duration::from_secs(60))
            .await;

        // 1s + 2s + 4s backoff = 7s → all three retries fire.
        tokio::time::sleep(Duration::from_secs(8)).await;
        assert!(
            src.call_count() >= 3,
            "transient failures should keep retrying with backoff; got {}",
            src.call_count()
        );
        registry.remove("flaky").await;
    }
}
