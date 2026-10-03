//! Does a cold backfill of a realistic event count wedge the runtime?
//!
//! The dev phone sat on `Restoring 16052 events…` for eight hours with every
//! `tokio-rt-worker` and every `surrealdb-thread` in state `S`, ~68s of worker
//! CPU spent, and every Tauri command hanging forever. This drives the same
//! `apply_events_resilient` path against a real SurrealKV directory so the
//! stall can be watched somewhere a debugger can reach.
//!
//! Ignored by default: it writes tens of thousands of rows and is measured in
//! minutes, which is not what the unit suite is for. Run it by name.

use std::time::{Duration, Instant};

use chrono::Utc;
use omni_me_core::config::ResolvedConfig;
use omni_me_core::db;
use omni_me_core::events::registry::build_projections;
use omni_me_core::events::{Event, ProjectionRunner};

/// Matches the dev server's log size at the time of the wedge.
const EVENT_COUNT: usize = 16_052;
/// Report cadence. A stall shows up as a gap between two of these lines.
const REPORT_EVERY: usize = 250;

/// One in this many events is an archived document carrying its whole source
/// text inline. The real `.eml` rows measured on the dev server are ~38 KB, and
/// a mailbox backfill is where they come from — a uniform stream of small notes
/// is not what the phone was actually applying.
const DOCUMENT_EVERY: usize = 40;
/// Matches the archived Walmart/Instacart `.eml` payloads.
const DOCUMENT_TEXT_BYTES: usize = 38_000;

fn synthetic_events(count: usize) -> Vec<Event> {
    let now = Utc::now();
    let body = "Subject: a forwarded receipt\nFrom: someone@example.com\n"
        .repeat(DOCUMENT_TEXT_BYTES / 56 + 1);
    (0..count)
        .map(|i| {
            if i % DOCUMENT_EVERY == 0 {
                let document_id = format!("doc-{:06}", i / DOCUMENT_EVERY);
                return Event {
                    id: format!("ev-{i:06}"),
                    event_type: "document_archived".to_string(),
                    aggregate_id: document_id.clone(),
                    timestamp: now,
                    device_id: "wedge-probe".to_string(),
                    payload: serde_json::json!({
                        "document_id": document_id,
                        "archived_at": now.to_rfc3339(),
                        "filename": format!("Fwd: a receipt {i}.eml"),
                        "mime_type": "message/rfc822",
                        "sha256": format!("{:064x}", i),
                        "size": DOCUMENT_TEXT_BYTES,
                        "source": "email",
                        "text": body,
                    }),
                    received_at: Some(now),
                };
            }
            // A spread of aggregate ids, so projections that read-modify-write
            // their own row exercise both the insert and the update path.
            let note_id = format!("note-{:06}", i / 3);
            Event {
                id: format!("ev-{i:06}"),
                event_type: "generic_note_created".to_string(),
                aggregate_id: note_id.clone(),
                timestamp: now,
                device_id: "wedge-probe".to_string(),
                payload: serde_json::json!({
                    "note_id": note_id,
                    "title": format!("Note {i}"),
                    "raw_text": format!("body for event {i}\n\nwith a second paragraph."),
                }),
                received_at: Some(now),
            }
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "minutes-long backfill probe; run by name"]
async fn a_cold_backfill_of_sixteen_thousand_events_completes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("wedge.db");
    let database = db::connect(path.to_str().unwrap()).await.expect("connect");

    let config = ResolvedConfig::default();
    let projections = build_projections(&config, dir.path().join("journal.ledger"));
    let projection_count = projections.len();
    let runner = ProjectionRunner::new(database, projections);

    let events = synthetic_events(EVENT_COUNT);
    eprintln!("applying {EVENT_COUNT} events through {projection_count} projections");

    let started = Instant::now();
    let mut last_report = Instant::now();
    let mut failed_total = 0usize;

    // Chunked so progress is observable. `apply_events_resilient` over the whole
    // slice is what the puller does, and it prints nothing until it returns —
    // which is exactly why the phone gave no signal for eight hours.
    for (chunk_index, chunk) in events.chunks(REPORT_EVERY).enumerate() {
        failed_total += runner.apply_events_resilient(chunk).await;
        let done = (chunk_index + 1) * REPORT_EVERY;
        let since_last = last_report.elapsed();
        last_report = Instant::now();
        eprintln!(
            "{done:>6} events  total={:>7.1}s  chunk={:>6.2}s  failed={failed_total}",
            started.elapsed().as_secs_f64(),
            since_last.as_secs_f64(),
        );
        assert!(
            since_last < Duration::from_secs(300),
            "a {REPORT_EVERY}-event chunk took over 5 minutes — this is the wedge"
        );
    }

    eprintln!(
        "done in {:.1}s, {failed_total} failed",
        started.elapsed().as_secs_f64()
    );
    assert_eq!(failed_total, 0, "no synthetic event should fail to project");
}
