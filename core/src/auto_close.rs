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

use crate::db::{queries, Database};
use crate::events::{EventStore, EventType, NewEvent, ProjectionRunner};

/// Scan for stale complete-but-not-closed journals and emit
/// `JournalEntryClosed { trigger: Auto }` for each. Returns the number of
/// entries closed so the caller can log / surface it.
pub async fn auto_close_stale_journals<S: EventStore + ?Sized>(
    db: &Database,
    event_store: &S,
    projections: &ProjectionRunner,
    device_id: &str,
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

    let mut closed = 0usize;
    for entry in candidates {
        let event = event_store
            .append(NewEvent {
                id: None,
                event_type: EventType::JournalEntryClosed.to_string(),
                aggregate_id: entry.journal_id.clone(),
                timestamp: Utc::now(),
                device_id: device_id.to_string(),
                payload: serde_json::json!({
                    "journal_id": entry.journal_id,
                    "trigger": "auto"
                }),
            })
            .await
            .map_err(AutoCloseError::Event)?;

        projections
            .apply_events(&[event])
            .await
            .map_err(AutoCloseError::Event)?;

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
    #[error("date underflow — no predecessor date for given 'today'")]
    DateOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{NotesProjection, SurrealEventStore};

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

    async fn seed_journal(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        journal_id: &str,
        date: &str,
        raw_text: &str,
    ) {
        let e = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: journal_id.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": journal_id,
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

        let complete_body = "homework_for_life: a\ngrateful_for: b\nlearnt_today: c";

        // Two complete past-day journals — should both close.
        seed_journal(&store, &runner, "j-apr17", "2026-04-17", complete_body).await;
        seed_journal(&store, &runner, "j-apr18", "2026-04-18", complete_body).await;

        // Today's journal is complete too — must NOT close.
        seed_journal(&store, &runner, "j-apr19", "2026-04-19", complete_body).await;

        // Incomplete past-day journal — must NOT close.
        seed_journal(&store, &runner, "j-apr16", "2026-04-16", "just a note").await;

        let closed = auto_close_stale_journals(&db, &store, &runner, "d1", ymd(2026, 4, 19))
            .await
            .unwrap();
        assert_eq!(closed, 2);

        let apr17 = queries::get_journal_by_date(&db, "2026-04-17").await.unwrap().unwrap();
        let apr18 = queries::get_journal_by_date(&db, "2026-04-18").await.unwrap().unwrap();
        let apr19 = queries::get_journal_by_date(&db, "2026-04-19").await.unwrap().unwrap();
        let apr16 = queries::get_journal_by_date(&db, "2026-04-16").await.unwrap().unwrap();

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

        let body = "homework_for_life: a\ngrateful_for: b\nlearnt_today: c";
        seed_journal(&store, &runner, "j1", "2026-04-17", body).await;

        let first = auto_close_stale_journals(&db, &store, &runner, "d1", ymd(2026, 4, 19))
            .await
            .unwrap();
        let second = auto_close_stale_journals(&db, &store, &runner, "d1", ymd(2026, 4, 19))
            .await
            .unwrap();

        assert_eq!(first, 1, "closed on first run");
        assert_eq!(second, 0, "already-closed rows are filtered out on second run");
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

        seed_journal(&store, &runner, "j1", "2026-04-18", "just body, no properties").await;

        let first = auto_close_stale_journals(&db, &store, &runner, "d1", ymd(2026, 4, 19))
            .await
            .unwrap();
        assert_eq!(first, 0, "incomplete past-day entry skipped");

        // User fills properties the next morning.
        let e = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_updated".into(),
                aggregate_id: "j1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": "j1",
                    "raw_text": "homework_for_life: a\ngrateful_for: b\nlearnt_today: c\n\nbody"
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();

        let second = auto_close_stale_journals(&db, &store, &runner, "d1", ymd(2026, 4, 19))
            .await
            .unwrap();
        assert_eq!(second, 1, "late-filled entry closes on next tick");
    }
}
