//! Cold-open startup / first-read timing probe (DIAGNOSTIC — remove once the
//! fresh-install "Loading…" hang is root-caused; v1 gate).
//!
//! The fresh-install cold-open hang (journal stuck on "Loading…" for ~2min,
//! fine on reopen) has an *unverified* cause. The backend's `tracing` output
//! goes to stdout, which on Android never reaches logcat — so there's been no
//! way to see where the time goes on-device. This module writes timestamped
//! checkpoints to **`<app_data>/startup-timing.log`** (pullable via
//! `adb pull …/startup-timing.log`) AND mirrors them to `tracing` (visible on
//! desktop stdout immediately). The last checkpoint before the multi-minute gap
//! localizes the hang: setup `block_on`, the first DB read, or the workspace read.
//!
//! Checkpoints during the boot sequence + the *first* journal/workspace read
//! fire; after the first journal read completes the probe deactivates, so normal
//! navigation doesn't append to the file. The file is append-only across process
//! launches (each run is delimited by a `==== boot …` header), so a fresh-install
//! run and a reopen run sit side-by-side for comparison.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

/// Monotonic origin for this process's elapsed-time column (set on first use).
static START: OnceLock<Instant> = OnceLock::new();
/// Whether checkpoints still fire. Boot + first cold read run with this true;
/// the first completed journal read flips it off (see [`deactivate`]).
static ACTIVE: AtomicBool = AtomicBool::new(true);

/// Timing-log filename inside the app data dir.
pub const TIMING_FILE: &str = "startup-timing.log";

/// Record a boot/first-read checkpoint: `<utc> +<elapsed>ms <label>` appended to
/// the timing file and emitted via `tracing`. No-op once [`deactivate`] has run.
pub fn checkpoint(app_data: &Path, label: &str) {
    if !ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let is_first = START.get().is_none();
    let start = *START.get_or_init(Instant::now);
    let ms = start.elapsed().as_millis();

    // No custom `target:` — use the module path (`omni_me_app::startup_probe`) so
    // the default `omni_me_app=debug` env filter actually lets it reach stdout on
    // desktop (a custom target like "startup_probe" is filtered out).
    tracing::info!(elapsed_ms = ms as u64, label, "cold-open checkpoint");

    let path = app_data.join(TIMING_FILE);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        if is_first {
            let _ = writeln!(
                f,
                "\n==== boot pid={} at {} ====",
                std::process::id(),
                chrono::Utc::now().to_rfc3339()
            );
        }
        let _ = writeln!(f, "{} +{}ms {}", chrono::Utc::now().to_rfc3339(), ms, label);
        let _ = f.flush();
    }
}

/// Stop recording checkpoints — called after the first journal read resolves, so
/// the probe captures the cold-open path then goes quiet for normal use.
pub fn deactivate() {
    ACTIVE.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_checkpoints_then_goes_quiet_after_deactivate() {
        let dir = std::env::temp_dir().join(format!(
            "omni-probe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        checkpoint(&dir, "setup:begin");
        checkpoint(&dir, "cmd:get_journal_by_date:end");
        deactivate();
        checkpoint(&dir, "should-not-appear");

        let content = std::fs::read_to_string(dir.join(TIMING_FILE)).unwrap();
        assert!(content.contains("==== boot"), "run header written on first checkpoint");
        assert!(content.contains("setup:begin"));
        assert!(content.contains("cmd:get_journal_by_date:end"));
        assert!(
            !content.contains("should-not-appear"),
            "deactivate() silences further checkpoints"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
