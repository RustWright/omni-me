//! Auto-close scheduler for stale, complete journal entries.
//!
//! A journal entry is eligible to auto-close when:
//!   - `complete = true` (all three manual properties are filled), AND
//!   - `closed = false` (not already closed), AND
//!   - `date < today` (the day has passed)
//!
//! The "fill next morning" case is covered because `complete` is recomputed
//! on every `JournalEntryUpdated`, so a user filling properties after midnight
//! still triggers close on the *next* tick.

use chrono::{NaiveDate, Utc};

use crate::db::{Database, queries};
use crate::events::{EventType, EventWriter, NewEvent};

/// Scan for stale complete-but-not-closed journals and emit
/// `JournalEntryClosed { trigger: Auto }` for each. Returns the number of
/// entries closed so the caller can log / surface it.
///
/// Writes through [`EventWriter`] rather than a bare store: this used to take
/// the store, the projection runner and the device id as three separate
/// parameters, which made it invisible to the client's "nothing appends
/// directly" scan and left the feature guard to a hand-written check at the
/// scheduler's spawn site.
pub async fn auto_close_stale_journals(
    db: &Database,
    writer: &EventWriter,
    today: NaiveDate,
) -> Result<usize, AutoCloseError> {
    let yesterday = today
        .pred_opt()
        .ok_or(AutoCloseError::DateOutOfRange)?
        .format("%Y-%m-%d")
        .to_string();

    let candidates = queries::list_completable_unclosed_journals(db, &yesterday)
        .await
        .map_err(AutoCloseError::Db)?;

    // TOCTOU note: between this snapshot and each append below, a user could
    // manually close a candidate journal — the audit log would then contain
    // both `trigger: "manual"` and `trigger: "auto"` events for the same
    // journal. Accepted as-is: realistic likelihood is near zero (midnight
    // scheduler + active user clicking close in the same millisecond), the
    // projection's observable state (`closed = true`) is idempotent either
    // way, and journals can already be closed→reopened→closed legitimately
    // so the projection cannot reject duplicate close events outright.
    let mut closed = 0usize;
    for entry in candidates {
        writer
            .append_new(NewEvent {
                id: None,
                event_type: EventType::JournalEntryClosed.to_string(),
                aggregate_id: entry.journal_id.clone(),
                timestamp: Utc::now(),
                device_id: writer.device_id().to_string(),
                payload: serde_json::json!({
                    "journal_id": entry.journal_id,
                    "trigger": "auto"
                }),
            })
            .await?;

        closed += 1;
    }

    Ok(closed)
}

#[derive(Debug, thiserror::Error)]
pub enum AutoCloseError {
    #[error("database error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("event error: {0}")]
    Event(#[from] crate::events::EventError),
    #[error("{0}")]
    Write(#[from] crate::events::WriteError),
    #[error("date underflow — no predecessor date for given 'today'")]
    DateOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ALL_FEATURES;
    use crate::events::{EventStore, NotesProjection, ProjectionRunner, SurrealEventStore};
    use std::sync::Arc;

    /// Every feature on, so these tests exercise the close logic rather than the
    /// writer's feature guard — that has its own tests in `events::writer`.
    fn test_writer(store: &SurrealEventStore, runner: &ProjectionRunner) -> EventWriter {
        EventWriter::new(
            Arc::new(store.clone()),
            runner.clone(),
            ALL_FEATURES.iter().copied().collect(),
            "d1",
        )
    }

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Seed a journal entry for `date`. The journal aggregate identity IS the
    /// date (deterministic, device-independent), so that's the aggregate_id.
    async fn seed_journal(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        date: &str,
        raw_text: &str,
    ) {
        let e = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: date.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": date,
                    "date": date,
                    "raw_text": raw_text
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();
    }

    #[tokio::test]
    async fn closes_complete_past_day_only() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();
        let writer = test_writer(&store, &runner);

        let complete_body = "homework_for_life: a\ngrateful_for: b\nlearnt_today: c";

        // Two complete past-day journals — should both close.
        seed_journal(&store, &runner, "2026-04-17", complete_body).await;
        seed_journal(&store, &runner, "2026-04-18", complete_body).await;

        // Today's journal is complete too — must NOT close.
        seed_journal(&store, &runner, "2026-04-19", complete_body).await;

        // Incomplete past-day journal — must NOT close.
        seed_journal(&store, &runner, "2026-04-16", "just a note").await;

        let closed = auto_close_stale_journals(&db, &writer, ymd(2026, 4, 19))
            .await
            .unwrap();
        assert_eq!(closed, 2);

        let apr17 = queries::get_journal_by_date(&db, "2026-04-17")
            .await
            .unwrap()
            .unwrap();
        let apr18 = queries::get_journal_by_date(&db, "2026-04-18")
            .await
            .unwrap()
            .unwrap();
        let apr19 = queries::get_journal_by_date(&db, "2026-04-19")
            .await
            .unwrap()
            .unwrap();
        let apr16 = queries::get_journal_by_date(&db, "2026-04-16")
            .await
            .unwrap()
            .unwrap();

        assert!(apr17.closed);
        assert!(apr18.closed);
        assert!(!apr19.closed, "today's entry stays open even if complete");
        assert!(!apr16.closed, "incomplete past-day entry stays open");
    }

    #[tokio::test]
    async fn is_idempotent_on_repeated_runs() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();
        let writer = test_writer(&store, &runner);

        let body = "homework_for_life: a\ngrateful_for: b\nlearnt_today: c";
        seed_journal(&store, &runner, "2026-04-17", body).await;

        let first = auto_close_stale_journals(&db, &writer, ymd(2026, 4, 19))
            .await
            .unwrap();
        let second = auto_close_stale_journals(&db, &writer, ymd(2026, 4, 19))
            .await
            .unwrap();

        assert_eq!(first, 1, "closed on first run");
        assert_eq!(
            second, 0,
            "already-closed rows are filtered out on second run"
        );
    }

    #[tokio::test]
    async fn fill_next_morning_closes_on_next_tick() {
        // Scenario: user writes body on Apr 18 but fills manual properties
        // after midnight on Apr 19. First tick (midnight Apr 19 boundary) sees
        // incomplete → skips. After the user finishes, a later tick sees
        // complete → closes.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();
        let writer = test_writer(&store, &runner);

        seed_journal(&store, &runner, "2026-04-18", "just body, no properties").await;

        let first = auto_close_stale_journals(&db, &writer, ymd(2026, 4, 19))
            .await
            .unwrap();
        assert_eq!(first, 0, "incomplete past-day entry skipped");

        // User fills properties the next morning (update routes by date).
        let e = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_updated".into(),
                aggregate_id: "2026-04-18".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": "2026-04-18",
                    "raw_text": "homework_for_life: a\ngrateful_for: b\nlearnt_today: c\n\nbody"
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();

        let second = auto_close_stale_journals(&db, &writer, ymd(2026, 4, 19))
            .await
            .unwrap();
        assert_eq!(second, 1, "late-filled entry closes on next tick");
    }
}
