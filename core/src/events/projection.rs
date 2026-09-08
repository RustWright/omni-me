use async_trait::async_trait;

use crate::db::Database;

use super::store::{Event, EventError, EventStore, SurrealEventStore};

/// A projection transforms events into read-optimized views.
#[async_trait]
pub trait Projection: Send + Sync {
    /// Human-readable name for this projection.
    fn name(&self) -> &str;

    /// Schema version — bump when the projection logic changes.
    fn version(&self) -> u32;

    /// Apply a single event to this projection's read tables.
    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError>;

    /// Initialize any tables/indexes this projection requires.
    async fn init_schema(&self, db: &Database) -> Result<(), EventError>;

    /// Delete all data from this projection's tables (used before rebuild).
    async fn clear_tables(&self, db: &Database) -> Result<(), EventError>;
}

/// Runs projections over events, tracking which events have been processed.
///
/// Cheaply cloneable — the projection list is shared via `Arc`, and the
/// `Database` handle is its own `Clone`-able connection pool wrapper.
#[derive(Clone)]
pub struct ProjectionRunner {
    db: Database,
    projections: std::sync::Arc<Vec<Box<dyn Projection>>>,
}

impl ProjectionRunner {
    pub fn new(db: Database, projections: Vec<Box<dyn Projection>>) -> Self {
        Self {
            db,
            projections: std::sync::Arc::new(projections),
        }
    }

    /// The database these projections write into.
    ///
    /// Exposed so an auto-import source can *read* the projections it feeds —
    /// chiefly to ask what it has already contributed before proposing more.
    /// Sources hold a `ProjectionRunner` already, so this avoids threading a
    /// second `Database` through every source constructor.
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Initialize all projection schemas and the projection_versions tracking table.
    pub async fn init_all(&self) -> Result<(), EventError> {
        self.db
            .query(
                "DEFINE TABLE IF NOT EXISTS projection_versions SCHEMAFULL;
                 DEFINE FIELD IF NOT EXISTS name ON projection_versions TYPE string;
                 DEFINE FIELD IF NOT EXISTS version ON projection_versions TYPE int;
                 DEFINE FIELD IF NOT EXISTS last_event_id ON projection_versions TYPE string;
                 -- Catch-up watermark. `last_event_id` cannot serve as one: ids
                 -- are ULIDs minted by whichever device authored the event and
                 -- kept through sync, so they are not monotonic in *this* store's
                 -- insertion order. `received_at` is, because this node stamps it.
                 DEFINE FIELD IF NOT EXISTS last_received_at ON projection_versions TYPE option<datetime>;
                 DEFINE INDEX IF NOT EXISTS idx_pv_name ON projection_versions FIELDS name UNIQUE;",
            )
            .await
?;

        let mut stale: Vec<String> = Vec::new();

        for proj in self.projections.iter() {
            proj.init_schema(&self.db).await?;

            let name = proj.name().to_string();
            let version = proj.version();

            // Read the stored version BEFORE overwriting it. `version()` was
            // write-only: it was recorded and never compared, so bumping it did
            // nothing at all and stale rows produced by superseded logic survived
            // forever. `NotesProjection` and `RoutinesProjection` both sat at 2
            // with no rebuild behind them.
            let mut resp = self
                .db
                .query("SELECT version FROM projection_versions WHERE name = $name LIMIT 1")
                .bind(("name", name.clone()))
                .await?;
            let stored: Option<i64> = resp.take("version").unwrap_or(None);
            if let Some(stored) = stored
                && stored != i64::from(version)
            {
                stale.push(name.clone());
            }

            // Upsert the version record, seeding `last_received_at` on first
            // sight. The two first-sight cases need **opposite** seeds, and
            // `stored` is what tells them apart: `version` is a non-optional
            // field, so no version read back means no row at all.
            //
            // - **No row** — this projection has never run, so it has projected
            //   nothing and must replay whatever the log already holds. Seeding
            //   `now` here skipped the entire backfill, silently and
            //   permanently: the mark sat ahead of those events forever, so no
            //   later launch recovered them either. That is the cold-start path
            //   — `SyncClient::pull_only` appends without projecting, so an
            //   agent pulls the whole log *before* any projection exists, and
            //   booted on default features no matter what the user configured.
            //
            // - **Row present, field NONE** — an install upgrading into this
            //   field, which is as caught-up as it ever was. Epoch there would
            //   mean a surprise full replay on the upgrade launch.
            //
            // A full replay through already-caught-up projections is the
            // accepted cost either way: `catch_up` takes the min across
            // registered rows, so re-enabling a feature already replays through
            // all of them, and projections are idempotent by construction.
            let seed = if stored.is_none() {
                chrono::DateTime::UNIX_EPOCH
            } else {
                chrono::Utc::now()
            };
            self.db
                .query(
                    "UPSERT projection_versions SET
                        name = $name,
                        version = $version,
                        last_event_id = last_event_id ?? '',
                        last_received_at = last_received_at ?? type::datetime($seed)
                     WHERE name = $name",
                )
                .bind(("name", name))
                .bind(("version", version))
                .bind(("seed", seed.to_rfc3339()))
                .await?;
        }

        if !stale.is_empty() {
            tracing::info!(
                projections = ?stale,
                "projection version changed — rebuilding from the event log"
            );
            self.rebuild().await?;
            return Ok(());
        }

        let caught_up = self.catch_up().await?;
        if caught_up > 0 {
            tracing::info!(
                events = caught_up,
                "replayed events the projections had missed"
            );
        }

        Ok(())
    }

    /// Replay events that were durably appended but never projected.
    ///
    /// `commands::shared` appends to the event store and *then* folds through
    /// the projections, and nothing makes that pair atomic. A crash between the
    /// two — or an Android process kill, which is routine — left the event
    /// stored and its projection update permanently missing, because nothing
    /// ever retried: `last_event_id` was written and never read anywhere in
    /// production. The only path that recovered it was `wipe_all_data`, which
    /// is also the only caller of `rebuild()`.
    ///
    /// Resilient rather than fail-fast: one bad historical event must not stop
    /// the rest of the backlog from landing.
    async fn catch_up(&self) -> Result<usize, EventError> {
        // ⚠️ Filtered to the **registered** set, and the filtering happens in Rust
        // rather than in the query. A feature-gated build registers a subset
        // (`events::registry`), and `advance_bookmark` only advances rows it
        // registered — so a de-registered projection's row freezes while the rest
        // move on. Read unfiltered, `min()` would pick that frozen mark and replay
        // the entire log on every launch, forever. The freeze itself is correct:
        // it is what makes re-enabling a feature replay from the right point.
        // The derive expands to `impl SurrealValue`, so the trait has to be in
        // scope here rather than only being named in the attribute.
        use surrealdb::types::SurrealValue;

        /// One watermark row. Read as a row rather than as two column vectors so
        /// a name can never be paired with another projection's mark.
        #[derive(SurrealValue)]
        struct Watermark {
            name: Option<String>,
            lr: Option<String>,
        }

        let mut resp = self
            .db
            .query("SELECT name, <string> last_received_at AS lr FROM projection_versions")
            .await?;
        let rows: Vec<Watermark> = resp.take(0).unwrap_or_default();

        let registered: std::collections::BTreeSet<&str> =
            self.projections.iter().map(|p| p.name()).collect();

        // The oldest watermark across **registered** projections — they advance
        // together, so the min only matters when the registered set changes: a
        // newly-added or re-enabled projection can't skip history.
        let Some(since) = rows
            .into_iter()
            .filter(|row| {
                row.name
                    .as_deref()
                    .is_some_and(|name| registered.contains(name))
            })
            .filter_map(|row| row.lr)
            .filter_map(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            })
            .min()
        else {
            return Ok(0);
        };

        let store = SurrealEventStore::new(self.db.clone());
        let missed = store.get_since(since, None).await?;
        if missed.is_empty() {
            return Ok(0);
        }

        let failed = self.apply_events_resilient(&missed).await;
        if failed > 0 {
            tracing::warn!(failed, "some events could not be replayed during catch-up");
        }
        Ok(missed.len())
    }

    /// Apply a batch of events through all matching projections, **fail-fast**:
    /// the first projection error aborts the batch and propagates.
    ///
    /// This is the right semantics for **local single-event commands** (via
    /// `commands::shared::append_and_apply`): the caller is the user whose
    /// action just failed, so surfacing the error — and refusing to advance —
    /// is correct. For the **sync-pull path** use [`apply_events_resilient`]
    /// instead: there, one malformed/colliding *remote* event must not strand
    /// every later event in the batch.
    pub async fn apply_events(&self, events: &[Event]) -> Result<(), EventError> {
        for event in events {
            for proj in self.projections.iter() {
                proj.apply(event, &self.db).await?;
            }
        }

        // Update last_event_id once after all events are applied
        if let Some(last) = events.last() {
            self.advance_bookmark(last).await?;
        }

        Ok(())
    }

    /// Apply a batch **best-effort**: a single failing event is logged and
    /// skipped rather than aborting the batch. Returns the count of events that
    /// failed to apply (`0` = all clean).
    ///
    /// This is the sync-pull apply path. Two properties make it safe:
    /// - The pulled events are already **durably appended** to the event store
    ///   before this runs, so a skipped event is never *lost* — a `rebuild()`
    ///   (full replay) always recovers it, and with idempotent projections a
    ///   replay is a no-op for the events that did apply.
    /// - Not aborting means the pull cursor (already advanced past this batch)
    ///   stays consistent with "we applied everything we could," instead of the
    ///   old fail-fast behaviour where one bad event silently dropped every
    ///   later event in the batch **and** they were never re-pulled.
    pub async fn apply_events_resilient(&self, events: &[Event]) -> usize {
        let mut failed = 0usize;
        let mut last_applied: Option<&Event> = None;

        for event in events {
            let mut event_ok = true;
            for proj in self.projections.iter() {
                if let Err(e) = proj.apply(event, &self.db).await {
                    tracing::warn!(
                        event_id = %event.id,
                        event_type = %event.event_type,
                        aggregate_id = %event.aggregate_id,
                        projection = proj.name(),
                        error = %e,
                        "projection apply failed during sync; skipping event (best-effort)"
                    );
                    event_ok = false;
                    // Keep going — sibling projections are independent and may
                    // still apply this event cleanly.
                }
            }
            if event_ok {
                last_applied = Some(event);
            } else {
                failed += 1;
            }
        }

        // Bookkeeping only (not the sync cursor): point at the last fully-applied
        // event. Best-effort — a failure here shouldn't fail the whole apply.
        if let Some(ev) = last_applied
            && let Err(e) = self.advance_bookmark(ev).await
        {
            tracing::warn!(error = %e, "failed to advance projection last_event_id after sync apply");
        }

        failed
    }

    /// Point every projection's bookmark at `event`.
    ///
    /// `last_received_at` is the one that matters — it is what `catch_up` reads.
    /// `last_event_id` is kept for diagnostics.
    async fn advance_bookmark(&self, event: &Event) -> Result<(), EventError> {
        let received = event
            .received_at
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339();
        for proj in self.projections.iter() {
            let name = proj.name().to_string();
            self.db
                .query(
                    "UPDATE projection_versions SET
                        last_event_id = $event_id,
                        last_received_at = type::datetime($received)
                     WHERE name = $name",
                )
                .bind(("event_id", event.id.clone()))
                .bind(("received", received.clone()))
                .bind(("name", name))
                .await?;
        }
        Ok(())
    }

    /// Rebuild all projections from scratch by replaying all events.
    pub async fn rebuild(&self) -> Result<(), EventError> {
        let store = SurrealEventStore::new(self.db.clone());

        // Get all events from the beginning of time
        let epoch = chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let events = store.get_since(epoch, None).await?;

        // Clear all projection tables, then re-initialize schemas
        for proj in self.projections.iter() {
            proj.clear_tables(&self.db).await?;
            proj.init_schema(&self.db).await?;
        }

        // Replay resiliently, NOT fail-fast. `clear_tables` has already run, so
        // a mid-replay error under `apply_events` left every projection wiped —
        // the opposite of what a rebuild is for, and unrecoverable without a
        // second successful rebuild.
        let failed = self.apply_events_resilient(&events).await;
        if failed > 0 {
            tracing::warn!(
                failed,
                total = events.len(),
                "events skipped during rebuild"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::store::{EventStore, NewEvent};
    use chrono::Utc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct CountingProjection {
        applied: Arc<AtomicU32>,
    }

    #[async_trait]
    impl Projection for CountingProjection {
        fn name(&self) -> &str {
            "counting"
        }

        fn version(&self) -> u32 {
            1
        }

        async fn apply(&self, _event: &Event, _db: &Database) -> Result<(), EventError> {
            self.applied.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn init_schema(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }

        async fn clear_tables(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }
    }

    /// Counts good applies but returns an error for any event whose
    /// `event_type` is `"boom"` — models a single malformed/colliding event in
    /// a pulled batch.
    struct FlakyProjection {
        applied: Arc<AtomicU32>,
    }

    #[async_trait]
    impl Projection for FlakyProjection {
        fn name(&self) -> &str {
            "flaky"
        }
        fn version(&self) -> u32 {
            1
        }
        async fn apply(&self, event: &Event, _db: &Database) -> Result<(), EventError> {
            if event.event_type == "boom" {
                return Err(EventError::Validation("boom".into()));
            }
            self.applied.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn init_schema(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }
        async fn clear_tables(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }
    }

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn ev(id: &str, event_type: &str) -> Event {
        Event {
            id: id.into(),
            event_type: event_type.into(),
            aggregate_id: "agg".into(),
            timestamp: Utc::now(),
            device_id: "d1".into(),
            payload: serde_json::json!({}),
            received_at: None,
        }
    }

    /// A projection whose `version()` the test can move.
    struct VersionedProjection {
        applied: Arc<AtomicU32>,
        version: u32,
    }

    #[async_trait]
    impl Projection for VersionedProjection {
        fn name(&self) -> &str {
            "versioned"
        }
        fn version(&self) -> u32 {
            self.version
        }
        async fn apply(&self, _event: &Event, _db: &Database) -> Result<(), EventError> {
            self.applied.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn init_schema(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }
        async fn clear_tables(&self, _db: &Database) -> Result<(), EventError> {
            Ok(())
        }
    }

    /// An event appended but never projected must be replayed at next startup.
    ///
    /// Absence test for a silent durability hole: `commands::shared` appends and
    /// *then* applies, with nothing making the pair atomic, so a crash (or an
    /// Android process kill, which is routine) between them left the event
    /// durably stored and its projection update permanently missing. Nothing
    /// retried it — `last_event_id` was written and never read in production —
    /// so the edit was absent from every view until `wipe_all_data`, the only
    /// path that reaches `rebuild()`.
    #[tokio::test]
    async fn startup_replays_events_that_were_appended_but_never_projected() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let counter = Arc::new(AtomicU32::new(0));

        {
            let runner = ProjectionRunner::new(
                db.clone(),
                vec![Box::new(CountingProjection {
                    applied: counter.clone(),
                })],
            );
            runner.init_all().await.unwrap();
        }

        // Simulate the crash window: the event lands in the store, and the
        // process dies before `apply_events` runs.
        store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "n1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({}),
            })
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 0, "nothing projected yet");

        // Next launch.
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(CountingProjection {
                applied: counter.clone(),
            })],
        );
        runner.init_all().await.unwrap();

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "the un-projected event was never replayed"
        );

        // And it is not replayed a second time on the launch after that.
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(CountingProjection {
                applied: counter.clone(),
            })],
        );
        runner.init_all().await.unwrap();
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "catch-up replayed an already-applied event"
        );
    }

    /// Events already in the store when a projection is registered for the
    /// **first** time must be projected, not skipped.
    ///
    /// This is the cold-start order, and it is the reverse of the crash-window
    /// test above: there the projection is registered first and the event lands
    /// after, here the log is populated before the projection has ever existed.
    /// The agent does exactly this — `learn_config` pulls the whole log via
    /// `pull_only`, which appends without projecting, and only then builds the
    /// runner.
    ///
    /// It regressed because the first-sight watermark seed was `time::now()`,
    /// which lands *after* the events the pull just wrote: catch-up then asked
    /// for everything since a point already past them and got nothing. Silent
    /// and permanent — the mark stays ahead forever, so no later launch recovers
    /// it either, and an agent would boot on default features no matter what the
    /// user had configured.
    #[tokio::test]
    async fn a_first_registration_projects_a_log_that_already_has_events() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let counter = Arc::new(AtomicU32::new(0));

        // The backfill: events land before any projection has been registered.
        for i in 0..3 {
            store
                .append(NewEvent {
                    id: None,
                    event_type: "note_created".into(),
                    aggregate_id: format!("n{i}"),
                    timestamp: Utc::now(),
                    device_id: "other-device".into(),
                    payload: serde_json::json!({}),
                })
                .await
                .unwrap();
        }

        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(CountingProjection {
                applied: counter.clone(),
            })],
        );
        runner.init_all().await.unwrap();

        assert_eq!(
            counter.load(Ordering::SeqCst),
            3,
            "a backfilled log was never projected: the first-sight watermark was \
             seeded past the events already in the store"
        );

        // And the mark is now genuinely caught up, so the next launch is quiet.
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(CountingProjection {
                applied: counter.clone(),
            })],
        );
        runner.init_all().await.unwrap();
        assert_eq!(
            counter.load(Ordering::SeqCst),
            3,
            "the backfill replayed a second time"
        );
    }

    /// Feature gating means a launch can register a *subset* of the projections
    /// the previous launch did. `advance_bookmark` only advances rows it
    /// registered, so a de-registered projection's watermark freezes — and
    /// `catch_up` used to read every row unfiltered, so `min()` picked that
    /// frozen mark and replayed the whole log on **every** launch, forever.
    ///
    /// The second half asserts the freeze is still load-bearing: re-registering
    /// the projection brings its stale mark back into the min, and the history it
    /// missed replays. That round trip is what makes turning a feature off
    /// non-destructive, and it is why the fix filters the read rather than
    /// deleting or advancing the row.
    #[tokio::test]
    async fn a_deregistered_projection_neither_forces_nor_loses_a_replay() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let counting = Arc::new(AtomicU32::new(0));
        let flaky = Arc::new(AtomicU32::new(0));

        let both = || {
            ProjectionRunner::new(
                db.clone(),
                vec![
                    Box::new(CountingProjection {
                        applied: counting.clone(),
                    }) as Box<dyn Projection>,
                    Box::new(FlakyProjection {
                        applied: flaky.clone(),
                    }),
                ],
            )
        };
        let counting_only = || {
            ProjectionRunner::new(
                db.clone(),
                vec![Box::new(CountingProjection {
                    applied: counting.clone(),
                }) as Box<dyn Projection>],
            )
        };

        // Launch 1: both features on, so both rows exist and are current.
        both().init_all().await.unwrap();

        for i in 0..3 {
            store
                .append(NewEvent {
                    id: None,
                    event_type: "note_created".into(),
                    aggregate_id: format!("n{i}"),
                    timestamp: Utc::now(),
                    device_id: "d1".into(),
                    payload: serde_json::json!({}),
                })
                .await
                .unwrap();
        }

        // Launch 2: one feature has been turned off. The backlog lands in the
        // projection that is still registered, and only its bookmark advances.
        counting_only().init_all().await.unwrap();
        assert_eq!(counting.load(Ordering::SeqCst), 3);
        assert_eq!(flaky.load(Ordering::SeqCst), 0, "an off feature projected");

        // Launch 3: nothing new happened, so nothing should replay. This is the
        // assertion that fails without the registered-name filter.
        counting_only().init_all().await.unwrap();
        assert_eq!(
            counting.load(Ordering::SeqCst),
            3,
            "the frozen watermark of a de-registered projection forced a replay"
        );

        // Launch 4: the feature is switched back on. Its stale mark re-enters the
        // min, so the events it missed replay and its state reconstructs.
        both().init_all().await.unwrap();
        assert_eq!(
            flaky.load(Ordering::SeqCst),
            3,
            "re-enabling a feature did not replay the history it missed"
        );
        // The already-current projection sees those events a second time, which
        // is why every projection has to be idempotent — `JournalFile` keys its
        // appends for exactly this reason.
        assert_eq!(counting.load(Ordering::SeqCst), 6);
    }

    /// Bumping `version()` must actually rebuild. It used to be write-only —
    /// recorded and never compared — so a bump did nothing and rows produced by
    /// superseded logic survived indefinitely.
    #[tokio::test]
    async fn a_version_bump_triggers_a_rebuild() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let counter = Arc::new(AtomicU32::new(0));

        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(VersionedProjection {
                applied: counter.clone(),
                version: 1,
            })],
        );
        runner.init_all().await.unwrap();

        let stored = store
            .append(NewEvent {
                id: None,
                event_type: "note_created".into(),
                aggregate_id: "n1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({}),
            })
            .await
            .unwrap();
        runner.apply_events(&[stored]).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Same version again: no rebuild, no replay.
        let same = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(VersionedProjection {
                applied: counter.clone(),
                version: 1,
            })],
        );
        same.init_all().await.unwrap();
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "unchanged version rebuilt anyway"
        );

        // Bumped version: full replay.
        let bumped = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(VersionedProjection {
                applied: counter.clone(),
                version: 2,
            })],
        );
        bumped.init_all().await.unwrap();
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "a version bump did not replay the event log"
        );
    }

    #[tokio::test]
    async fn init_all_creates_version_table() {
        let db = test_db().await;
        let counter = Arc::new(AtomicU32::new(0));

        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(CountingProjection {
                applied: counter.clone(),
            })],
        );

        runner.init_all().await.unwrap();

        // Check version record exists
        let mut resp = db
            .query("SELECT * FROM projection_versions WHERE name = 'counting'")
            .await
            .unwrap();
        let name: Option<String> = resp.take("name").unwrap();
        assert_eq!(name.as_deref(), Some("counting"));
    }

    #[tokio::test]
    async fn apply_events_runs_projections() {
        let db = test_db().await;
        let counter = Arc::new(AtomicU32::new(0));

        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(CountingProjection {
                applied: counter.clone(),
            })],
        );

        runner.init_all().await.unwrap();

        let events = vec![
            Event {
                id: "e1".into(),
                event_type: "note_created".into(),
                aggregate_id: "n1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({}),
                received_at: None,
            },
            Event {
                id: "e2".into(),
                event_type: "note_updated".into(),
                aggregate_id: "n1".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({}),
                received_at: None,
            },
        ];

        runner.apply_events(&events).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);

        // Verify last_event_id points to the final event
        let mut resp = db
            .query("SELECT last_event_id FROM projection_versions WHERE name = 'counting'")
            .await
            .unwrap();
        let last_id: Option<String> = resp.take("last_event_id").unwrap();
        assert_eq!(last_id.as_deref(), Some("e2"));
    }

    #[tokio::test]
    async fn resilient_apply_skips_bad_event_and_continues() {
        let db = test_db().await;
        let applied = Arc::new(AtomicU32::new(0));
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(FlakyProjection {
                applied: applied.clone(),
            })],
        );
        runner.init_all().await.unwrap();

        // Middle event errors; the two good ones on either side must still apply.
        let events = vec![
            ev("g1", "note_created"),
            ev("bad", "boom"),
            ev("g2", "note_updated"),
        ];
        let failed = runner.apply_events_resilient(&events).await;

        assert_eq!(failed, 1, "one event should fail");
        assert_eq!(
            applied.load(Ordering::SeqCst),
            2,
            "both good events apply despite the bad one"
        );

        // last_event_id parks on the last *successfully* applied event, not the failing tail-1.
        let mut resp = db
            .query("SELECT last_event_id FROM projection_versions WHERE name = 'flaky'")
            .await
            .unwrap();
        let last_id: Option<String> = resp.take("last_event_id").unwrap();
        assert_eq!(last_id.as_deref(), Some("g2"));
    }
}
