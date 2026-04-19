//! OS network event listener.
//!
//! Emits "online" hints the retry engine can use to nudge its retry schedule.
//! The emitted signal is edge-triggered: `NetworkEvent::Online` fires on
//! transitions from `Unknown|Offline -> Online`.
//!
//! ## Platform coverage
//!
//! Phase 2 ships with a simple probe-based monitor that works on every Rust
//! platform we support (Linux, macOS, Windows, Android, iOS). It polls TCP
//! reachability to a configurable probe host on an interval and emits an
//! `Online` event on reconnect edges.
//!
//! The `tauri-plugin-network` crate was evaluated but only exposes *static*
//! network info (adapters, IPs), not push-based connectivity events. A
//! richer implementation using Android's `ConnectivityManager.NetworkCallback`
//! is deferred until a JNI bridge is wired — see TODO below. The probe
//! monitor is sufficient as a Phase 2 hint source on all platforms.
//!
//! TODO(android-native-callback): Replace the probe on Android with
//! `ConnectivityManager.NetworkCallback` once a JNI bridge is available.
//! Filed as Cycle 3 backlog candidate.

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;

/// Default probe interval. Faster than a typical sync interval so
/// reconnection is detected promptly without hammering the probe host.
pub const DEFAULT_PROBE_INTERVAL: Duration = Duration::from_secs(10);

/// Default TCP connect timeout.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

const EVENT_CHANNEL_CAPACITY: usize = 8;

/// Edge-triggered network event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkEvent {
    /// Transitioned from unknown/offline to online.
    Online,
    /// Transitioned from online to offline.
    Offline,
}

/// Last observed reachability state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    Unknown,
    Online,
    Offline,
}

/// Probe-based network monitor. Cloneable handle; background task owns state.
#[derive(Clone)]
pub struct NetworkMonitor {
    inner: Arc<Inner>,
}

struct Inner {
    probe_target: String,
    interval: Duration,
    timeout: Duration,
    events: broadcast::Sender<NetworkEvent>,
    state: Mutex<NetworkState>,
    shutdown: tokio::sync::Notify,
}

impl NetworkMonitor {
    /// Spawn a monitor that probes the given `host:port` target (e.g.
    /// `"1.1.1.1:53"` or `"localhost:3000"`).
    pub fn spawn(probe_target: impl Into<String>) -> (Self, JoinHandle<()>) {
        Self::spawn_with(probe_target, DEFAULT_PROBE_INTERVAL, DEFAULT_PROBE_TIMEOUT)
    }

    pub fn spawn_with(
        probe_target: impl Into<String>,
        interval: Duration,
        timeout: Duration,
    ) -> (Self, JoinHandle<()>) {
        let (events_tx, _rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            probe_target: probe_target.into(),
            interval,
            timeout,
            events: events_tx,
            state: Mutex::new(NetworkState::Unknown),
            shutdown: tokio::sync::Notify::new(),
        });
        let monitor = Self { inner: inner.clone() };
        let handle = tokio::spawn(probe_loop(inner));
        (monitor, handle)
    }

    /// Subscribe to network events.
    pub fn subscribe(&self) -> broadcast::Receiver<NetworkEvent> {
        self.inner.events.subscribe()
    }

    /// Current observed state.
    pub async fn current(&self) -> NetworkState {
        *self.inner.state.lock().await
    }

    /// Stop the monitor.
    pub fn shutdown(&self) {
        self.inner.shutdown.notify_one();
    }

    /// Probe the target once and update state (useful for tests and forced
    /// checks without waiting for the interval). Returns the new state.
    pub async fn probe_now(&self) -> NetworkState {
        probe_and_transition(&self.inner).await
    }
}

async fn probe_loop(inner: Arc<Inner>) {
    loop {
        let _ = probe_and_transition(&inner).await;

        tokio::select! {
            _ = inner.shutdown.notified() => return,
            _ = tokio::time::sleep(inner.interval) => {}
        }
    }
}

async fn probe_and_transition(inner: &Arc<Inner>) -> NetworkState {
    let online = reachable(&inner.probe_target, inner.timeout).await;
    let new_state = if online {
        NetworkState::Online
    } else {
        NetworkState::Offline
    };

    let mut state = inner.state.lock().await;
    let prev = *state;
    *state = new_state;

    // Edge detection
    match (prev, new_state) {
        (NetworkState::Online, NetworkState::Offline) => {
            let _ = inner.events.send(NetworkEvent::Offline);
        }
        (NetworkState::Offline, NetworkState::Online)
        | (NetworkState::Unknown, NetworkState::Online) => {
            let _ = inner.events.send(NetworkEvent::Online);
        }
        _ => {}
    }

    new_state
}

async fn reachable(target: &str, timeout_dur: Duration) -> bool {
    match tokio::time::timeout(timeout_dur, TcpStream::connect(target)).await {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn probe_emits_online_on_reachable_target() {
        // Bind a listener we know is reachable.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        // Keep the listener alive for the duration of the test.
        let _accept_task = tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });

        let (monitor, _h) = NetworkMonitor::spawn_with(
            addr,
            Duration::from_secs(60),
            Duration::from_millis(500),
        );
        let mut sub = monitor.subscribe();

        // Allow the initial probe to run.
        let ev = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .expect("should get Online")
            .unwrap();
        assert_eq!(ev, NetworkEvent::Online);
        assert_eq!(monitor.current().await, NetworkState::Online);
        monitor.shutdown();
    }

    #[tokio::test]
    async fn probe_emits_offline_on_unreachable_target() {
        // :1 is reserved, no service listens by default.
        let (monitor, _h) = NetworkMonitor::spawn_with(
            "127.0.0.1:1",
            Duration::from_secs(60),
            Duration::from_millis(200),
        );

        // Wait for the first probe to settle.
        tokio::time::sleep(Duration::from_millis(300)).await;

        // No Online event should be emitted because first transition is
        // Unknown -> Offline, which we don't broadcast (edge rule). Verify
        // the internal state instead.
        assert_eq!(monitor.current().await, NetworkState::Offline);
        monitor.shutdown();
    }

    #[tokio::test]
    async fn transitions_are_edge_triggered() {
        // Start offline (no listener yet), then spin up listener, manually probe.
        let (monitor, _h) = NetworkMonitor::spawn_with(
            "127.0.0.1:2", // unreachable initially
            Duration::from_secs(60),
            Duration::from_millis(200),
        );
        let mut sub = monitor.subscribe();
        // First probe (unreachable) — no event.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let pending = tokio::time::timeout(Duration::from_millis(50), sub.recv()).await;
        assert!(pending.is_err(), "no event on initial offline");

        // Bind a listener on the target.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:2").await;
        if listener.is_err() {
            // Port might be in use on the CI host; skip.
            monitor.shutdown();
            return;
        }
        let listener = listener.unwrap();
        let _accept_task = tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });

        // Force a probe — should now see Online.
        let new_state = monitor.probe_now().await;
        assert_eq!(new_state, NetworkState::Online);
        let ev = tokio::time::timeout(Duration::from_secs(1), sub.recv())
            .await
            .expect("should get Online on transition")
            .unwrap();
        assert_eq!(ev, NetworkEvent::Online);
        monitor.shutdown();
    }
}
