//! Background scheduler for `omni_me_core::document_enrichment`.
//!
//! Why the pass exists, why its candidate query is the work queue, and why it
//! is off by default and capped when on: `docs/src/archive.md`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use omni_me_core::db::Database;
use omni_me_core::document_enrichment::{
    DEFAULT_MAX_PER_TICK, EnrichSummary, RetryHolds, enrich_fields_once, enrich_text_once,
};
use omni_me_core::events::EventWriter;
use omni_me_core::extraction::document::DocumentReader;
use omni_me_core::extraction::transcribe::DocumentTranscriber;

const DEFAULT_INTERVAL: Duration = Duration::from_secs(30 * 60);
const BACKOFF_START: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(60 * 60);

/// How the pass is configured for this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub max_per_tick: usize,
}

impl Default for EnrichConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: DEFAULT_INTERVAL,
            max_per_tick: DEFAULT_MAX_PER_TICK,
        }
    }
}

/// Read the pass's configuration from the environment.
///
/// Off unless asked for: configuring a role-C model is not the same act as
/// deciding to spend it across every document filed to date.
pub fn config_from_env() -> EnrichConfig {
    EnrichConfig {
        enabled: enabled_from_env("OMNI_ENRICH_ENABLED"),
        interval: std::env::var("OMNI_ENRICH_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(|s| s.clamp(60, 3600))
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_INTERVAL),
        max_per_tick: std::env::var("OMNI_ENRICH_MAX_PER_TICK")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|n| n.clamp(1, 50))
            .unwrap_or(DEFAULT_MAX_PER_TICK),
    }
}

/// Parse an on/off switch, failing closed but never silently.
///
/// A misspelling reads as off, which is the safe direction for a paid pass —
/// but it is also how a switch the operator did set goes unnoticed, so an
/// unrecognized value is logged at error rather than shrugged off.
fn enabled_from_env(key: &str) -> bool {
    let Ok(raw) = std::env::var(key) else {
        return false;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => false,
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        other => {
            tracing::error!(
                key,
                value = %other,
                "unrecognized on/off value — treating as off; use 1 or 0"
            );
            false
        }
    }
}

/// Spawn the enrichment loop, or explain why it is not running.
///
/// Takes the reader as an `Option` so the one place that knows a model is
/// configured is the one place that decides whether a tick can do anything.
pub fn spawn(
    db: Database,
    writer: Arc<EventWriter>,
    blob_dir: PathBuf,
    reader: Option<Arc<dyn DocumentReader>>,
    transcriber: Option<Arc<dyn DocumentTranscriber>>,
    cfg: EnrichConfig,
) {
    if !cfg.enabled {
        tracing::info!("document enrichment disabled — set OMNI_ENRICH_ENABLED=1 to turn it on");
        return;
    }
    if reader.is_none() && transcriber.is_none() {
        // Name the seats this actually reads. It used to say [llm.extractor],
        // which is the one table neither half consults, so acting on the
        // message reproduced it exactly.
        tracing::warn!(
            "document enrichment enabled but neither [llm.reader] nor [llm.transcriber] \
             resolves a vision endpoint — not spawning"
        );
        return;
    }

    tracing::info!(
        interval_secs = cfg.interval.as_secs(),
        max_per_tick = cfg.max_per_tick,
        cataloguing = reader.is_some(),
        transcribing = transcriber.is_some(),
        "document enrichment scheduler initialized"
    );

    tokio::spawn(async move {
        let mut backoff = BACKOFF_START;
        let mut cataloguing_holds = RetryHolds::default();
        let mut transcription_holds = RetryHolds::default();
        loop {
            // Both halves per tick, each capped separately. They compete for
            // nothing: one selects documents with no kind, the other documents
            // with no text, and a scan is usually both.
            let mut failed = None;
            if let Some(reader) = reader.as_ref() {
                match enrich_fields_once(
                    &db,
                    &writer,
                    &blob_dir,
                    reader.as_ref(),
                    cfg.max_per_tick,
                    &mut cataloguing_holds,
                )
                .await
                {
                    Ok(s) => log_tick("cataloguing", &s),
                    Err(e) => failed = Some(e.to_string()),
                }
            }
            if let Some(transcriber) = transcriber.as_ref()
                && failed.is_none()
            {
                match enrich_text_once(
                    &db,
                    &writer,
                    &blob_dir,
                    transcriber.as_ref(),
                    cfg.max_per_tick,
                    &mut transcription_holds,
                )
                .await
                {
                    Ok(s) => log_tick("transcription", &s),
                    Err(e) => failed = Some(e.to_string()),
                }
            }

            match failed {
                None => {
                    backoff = BACKOFF_START;
                    tokio::time::sleep(cfg.interval).await;
                }
                Some(error) => {
                    tracing::warn!(
                        error,
                        backoff_secs = backoff.as_secs(),
                        "enrichment tick failed"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(BACKOFF_CAP);
                }
            }
        }
    });
}

/// One tick's outcome, named rather than counted — a count alone tells nobody
/// which documents to go and look at.
fn log_tick(half: &'static str, summary: &EnrichSummary) {
    if summary.seen == 0 {
        tracing::debug!(
            half,
            held = summary.held,
            unreadable_mime = summary.unreadable_mime,
            "enrichment: nothing waiting"
        );
        return;
    }
    tracing::info!(
        half,
        seen = summary.seen,
        read = summary.read,
        no_bytes = summary.no_bytes,
        no_readable_form = summary.no_readable_form,
        not_read = summary.not_read,
        held = summary.held,
        unreadable_mime = summary.unreadable_mime,
        skipped = ?summary.sample_skipped(3),
        "enrichment tick"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env access is process-global, so these run under one lock rather than as
    /// separate tests that would interleave.
    #[test]
    fn the_switch_fails_closed_and_names_a_typo() {
        unsafe {
            std::env::remove_var("OMNI_TEST_SWITCH");
        }
        assert!(!enabled_from_env("OMNI_TEST_SWITCH"), "unset is off");

        for on in ["1", "true", "YES", " on "] {
            unsafe { std::env::set_var("OMNI_TEST_SWITCH", on) };
            assert!(enabled_from_env("OMNI_TEST_SWITCH"), "{on} should be on");
        }
        for off in ["0", "false", "no", "off", ""] {
            unsafe { std::env::set_var("OMNI_TEST_SWITCH", off) };
            assert!(!enabled_from_env("OMNI_TEST_SWITCH"), "{off} should be off");
        }

        // The case this exists for: a typo must not read as on, and must not
        // pass unremarked either.
        unsafe { std::env::set_var("OMNI_TEST_SWITCH", "ture") };
        assert!(!enabled_from_env("OMNI_TEST_SWITCH"));
        unsafe { std::env::remove_var("OMNI_TEST_SWITCH") };
    }

    #[test]
    fn the_default_is_off_with_a_conservative_cap() {
        let cfg = EnrichConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.max_per_tick, DEFAULT_MAX_PER_TICK);
        assert_eq!(cfg.interval, DEFAULT_INTERVAL);
    }
}
