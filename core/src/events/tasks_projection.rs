use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use surrealdb::types::SurrealValue;

use crate::db::Database;
use crate::tasks::{TaskField, TaskStatus, parse_due};

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::{
    DeadlineFeedReceivedPayload, HubIndexReceivedPayload, TaskCreatedPayload, TaskModifiedPayload,
    TaskProjectClosedPayload, TaskProjectCreatedPayload, TaskStatusSetPayload, TaskTimerPayload,
};

/// Projection over task events.
///
/// `tasks` and `task_projects` are native and owned here; `hub_tasks` mirrors the last index
/// pushed for each project in `hub_indexes`, replaced whole on every push.
pub struct TasksProjection;

impl TasksProjection {
    pub const NAME: &'static str = "tasks";
}

#[async_trait]
impl Projection for TasksProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS tasks SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS task_id ON tasks TYPE string;
             DEFINE FIELD IF NOT EXISTS title ON tasks TYPE string;
             DEFINE FIELD IF NOT EXISTS project_id ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS estimate_min ON tasks TYPE option<int>;
             DEFINE FIELD IF NOT EXISTS due_date ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS due_time ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS scheduled ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS repeat_rule ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS series_id ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS hub_id ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS deadline_uid ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS note ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS status ON tasks TYPE string;
             DEFINE FIELD IF NOT EXISTS actual_min ON tasks TYPE option<int>;
             DEFINE FIELD IF NOT EXISTS closed_at ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS created_at ON tasks TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON tasks TYPE datetime;
             -- The timer: a session runs from its first start to its stop, and pauses split
             -- it into running spans. Active is the spans, elapsed the whole session: ruling 93.
             -- Instants here and in closed_at are RFC 3339 UTC strings, so they sort as text.
             DEFINE FIELD IF NOT EXISTS timer_state ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS timer_session_start ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS timer_running_since ON tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS timer_active_secs ON tasks TYPE int;
             DEFINE FIELD IF NOT EXISTS active_secs_total ON tasks TYPE int;
             DEFINE FIELD IF NOT EXISTS elapsed_secs_total ON tasks TYPE int;
             DEFINE INDEX IF NOT EXISTS idx_task_title_fts ON tasks
                 FIELDS title FULLTEXT ANALYZER omni_text BM25 HIGHLIGHTS;

             DEFINE TABLE IF NOT EXISTS task_projects SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS project_id ON task_projects TYPE string;
             DEFINE FIELD IF NOT EXISTS name ON task_projects TYPE string;
             DEFINE FIELD IF NOT EXISTS course ON task_projects TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS term ON task_projects TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS ends_on ON task_projects TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS closed ON task_projects TYPE bool;
             DEFINE FIELD IF NOT EXISTS created_at ON task_projects TYPE datetime;
             DEFINE INDEX IF NOT EXISTS idx_task_project_name_fts ON task_projects
                 FIELDS name FULLTEXT ANALYZER omni_text BM25 HIGHLIGHTS;

             DEFINE TABLE IF NOT EXISTS hub_indexes SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS project ON hub_indexes TYPE string;
             DEFINE FIELD IF NOT EXISTS content_hash ON hub_indexes TYPE string;
             DEFINE FIELD IF NOT EXISTS updated ON hub_indexes TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS cycle ON hub_indexes TYPE option<int>;
             DEFINE FIELD IF NOT EXISTS received_at ON hub_indexes TYPE datetime;
             DEFINE FIELD IF NOT EXISTS problems ON hub_indexes TYPE array<string>;
             DEFINE FIELD IF NOT EXISTS task_count ON hub_indexes TYPE int;

             DEFINE TABLE IF NOT EXISTS hub_tasks SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS hub_id ON hub_tasks TYPE string;
             DEFINE FIELD IF NOT EXISTS project ON hub_tasks TYPE string;
             DEFINE FIELD IF NOT EXISTS title ON hub_tasks TYPE string;
             DEFINE FIELD IF NOT EXISTS status ON hub_tasks TYPE string;
             DEFINE FIELD IF NOT EXISTS size ON hub_tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS due ON hub_tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS note ON hub_tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS source_ref ON hub_tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS origin ON hub_tasks TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS tags ON hub_tasks TYPE array<string>;
             DEFINE FIELD IF NOT EXISTS blocked_by ON hub_tasks TYPE array<string>;
             DEFINE FIELD IF NOT EXISTS needs_you ON hub_tasks TYPE bool;
             DEFINE FIELD IF NOT EXISTS is_open ON hub_tasks TYPE bool;
             DEFINE INDEX IF NOT EXISTS idx_hub_task_title_fts ON hub_tasks
                 FIELDS title FULLTEXT ANALYZER omni_text BM25 HIGHLIGHTS;

             -- The last read of each deadline feed, replaced whole per read like hub_tasks.
             DEFINE TABLE IF NOT EXISTS feed_deadlines SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS uid ON feed_deadlines TYPE string;
             DEFINE FIELD IF NOT EXISTS source ON feed_deadlines TYPE string;
             DEFINE FIELD IF NOT EXISTS course ON feed_deadlines TYPE string;
             DEFINE FIELD IF NOT EXISTS term ON feed_deadlines TYPE string;
             DEFINE FIELD IF NOT EXISTS title ON feed_deadlines TYPE string;
             DEFINE FIELD IF NOT EXISTS due_date ON feed_deadlines TYPE string;
             DEFINE FIELD IF NOT EXISTS due_time ON feed_deadlines TYPE option<string>;
             DEFINE INDEX IF NOT EXISTS idx_feed_deadline_title_fts ON feed_deadlines
                 FIELDS title FULLTEXT ANALYZER omni_text BM25 HIGHLIGHTS;

             DEFINE TABLE IF NOT EXISTS deadline_feeds SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS source ON deadline_feeds TYPE string;
             DEFINE FIELD IF NOT EXISTS content_hash ON deadline_feeds TYPE string;
             DEFINE FIELD IF NOT EXISTS received_at ON deadline_feeds TYPE datetime;
             DEFINE FIELD IF NOT EXISTS problems ON deadline_feeds TYPE array<string>;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DELETE FROM tasks;
             DELETE FROM task_projects;
             DELETE FROM hub_indexes;
             DELETE FROM hub_tasks;
             DELETE FROM feed_deadlines;
             DELETE FROM deadline_feeds;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "task_created" => match payload::<TaskCreatedPayload>(event) {
                Some(p) => on_created(&p, event, db).await,
                None => Ok(()),
            },
            "task_modified" => match payload::<TaskModifiedPayload>(event) {
                Some(p) => on_modified(&p, event, db).await,
                None => Ok(()),
            },
            "task_status_set" => match payload::<TaskStatusSetPayload>(event) {
                Some(p) => on_status(&p, event, db).await,
                None => Ok(()),
            },
            kind @ ("task_timer_started" | "task_timer_paused" | "task_timer_stopped") => {
                match payload::<TaskTimerPayload>(event) {
                    Some(p) => on_timer(kind, &p.task_id, event.timestamp, db).await,
                    None => Ok(()),
                }
            }
            "task_project_created" => match payload::<TaskProjectCreatedPayload>(event) {
                Some(p) => on_project_created(&p, event, db).await,
                None => Ok(()),
            },
            "task_project_closed" => match payload::<TaskProjectClosedPayload>(event) {
                Some(p) => {
                    db.query(
                        "UPDATE type::record('task_projects', $id) SET closed = true
                         WHERE project_id = $id",
                    )
                    .bind(("id", p.project_id))
                    .await?
                    .check()?;
                    Ok(())
                }
                None => Ok(()),
            },
            "hub_index_received" => match payload::<HubIndexReceivedPayload>(event) {
                Some(p) => on_hub_index(&p, event, db).await,
                None => Ok(()),
            },
            "deadline_feed_received" => match payload::<DeadlineFeedReceivedPayload>(event) {
                Some(p) => on_deadline_feed(&p, event, db).await,
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }
}

/// A payload this build cannot read is skipped with a warning, never fatal: a newer build's
/// event must not stop every later one from applying.
fn payload<T: DeserializeOwned>(event: &Event) -> Option<T> {
    match serde_json::from_value(event.payload.clone()) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!(event_id = %event.id, kind = %event.event_type, error = %e,
                "skipping a task event this build cannot read");
            None
        }
    }
}

/// Splits a due string into its columns. Validated on the way in, so a bad one is dropped.
fn due_columns(due: Option<&str>) -> (Option<String>, Option<String>) {
    match due.map(parse_due) {
        Some(Ok((date, time))) => (
            Some(date.format("%Y-%m-%d").to_string()),
            time.map(|t| t.format("%H:%M").to_string()),
        ),
        _ => (None, None),
    }
}

async fn on_created(
    p: &TaskCreatedPayload,
    event: &Event,
    db: &Database,
) -> Result<(), EventError> {
    let (due_date, due_time) = due_columns(p.due.as_deref());
    // UPSERT so a replay or a create pulled from another device converges on the row;
    // status and the timer totals keep any value an earlier-applied event set.
    db.query(
        "UPSERT type::record('tasks', $id) SET
            task_id = $id,
            title = $title,
            project_id = $project_id,
            estimate_min = $estimate_min,
            due_date = $due_date,
            due_time = $due_time,
            scheduled = $scheduled,
            repeat_rule = $repeat,
            series_id = $series_id,
            hub_id = $hub_id,
            deadline_uid = $deadline_uid,
            note = $note,
            status = status ?? 'todo',
            timer_active_secs = timer_active_secs ?? 0,
            active_secs_total = active_secs_total ?? 0,
            elapsed_secs_total = elapsed_secs_total ?? 0,
            created_at = created_at ?? type::datetime($ts),
            updated_at = type::datetime($ts)",
    )
    .bind(("id", p.task_id.clone()))
    .bind(("title", p.title.clone()))
    .bind(("project_id", p.project_id.clone()))
    .bind(("estimate_min", p.estimate_min.map(i64::from)))
    .bind(("due_date", due_date))
    .bind(("due_time", due_time))
    .bind((
        "scheduled",
        p.scheduled.map(|d| d.format("%Y-%m-%d").to_string()),
    ))
    .bind(("repeat", p.repeat.clone()))
    .bind(("series_id", p.series_id.clone()))
    .bind(("hub_id", p.hub_id.clone()))
    .bind(("deadline_uid", p.deadline_uid.clone()))
    .bind(("note", p.note.clone()))
    .bind(("ts", event.timestamp.to_rfc3339()))
    .await?
    .check()?;
    Ok(())
}

async fn on_modified(
    p: &TaskModifiedPayload,
    event: &Event,
    db: &Database,
) -> Result<(), EventError> {
    // Only the columns named change; each is one fixed clause, never interpolated text.
    let mut sets = vec!["updated_at = type::datetime($ts)"];
    if p.title.is_some() {
        sets.push("title = $title");
    }
    if p.project_id.is_some() || p.clear.contains(&TaskField::Project) {
        sets.push("project_id = $project_id");
    }
    if p.estimate_min.is_some() || p.clear.contains(&TaskField::Estimate) {
        sets.push("estimate_min = $estimate_min");
    }
    if p.due.is_some() || p.clear.contains(&TaskField::Due) {
        sets.push("due_date = $due_date");
        sets.push("due_time = $due_time");
    }
    if p.scheduled.is_some() || p.clear.contains(&TaskField::Scheduled) {
        sets.push("scheduled = $scheduled");
    }
    if p.repeat.is_some() || p.clear.contains(&TaskField::Repeat) {
        sets.push("repeat_rule = $repeat");
    }
    if p.note.is_some() || p.clear.contains(&TaskField::Note) {
        sets.push("note = $note");
    }
    let (due_date, due_time) = due_columns(p.due.as_deref());
    // UPDATE, not UPSERT: a change to a task this device never saw created has nothing to
    // change, and an UPSERT would make a row with no title.
    let sql = format!("UPDATE type::record('tasks', $id) SET {}", sets.join(", "));
    db.query(sql)
        .bind(("id", p.task_id.clone()))
        .bind(("title", p.title.clone()))
        .bind(("project_id", p.project_id.clone()))
        .bind(("estimate_min", p.estimate_min.map(i64::from)))
        .bind(("due_date", due_date))
        .bind(("due_time", due_time))
        .bind((
            "scheduled",
            p.scheduled.map(|d| d.format("%Y-%m-%d").to_string()),
        ))
        .bind(("repeat", p.repeat.clone()))
        .bind(("note", p.note.clone()))
        .bind(("ts", event.timestamp.to_rfc3339()))
        .await?
        .check()?;
    Ok(())
}

async fn on_status(
    p: &TaskStatusSetPayload,
    event: &Event,
    db: &Database,
) -> Result<(), EventError> {
    let closed = p.status != TaskStatus::Todo;
    db.query(
        "UPDATE type::record('tasks', $id) SET
            status = $status,
            actual_min = $actual_min,
            closed_at = $closed_at,
            updated_at = type::datetime($ts)",
    )
    .bind(("id", p.task_id.clone()))
    .bind(("status", p.status.as_str().to_string()))
    .bind(("actual_min", p.actual_min.filter(|_| closed).map(i64::from)))
    .bind(("closed_at", closed.then(|| event.timestamp.to_rfc3339())))
    .bind(("ts", event.timestamp.to_rfc3339()))
    .await?
    .check()?;
    Ok(())
}

/// The timer as stored on a task row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimerState {
    pub running: bool,
    pub session_start: Option<DateTime<Utc>>,
    pub running_since: Option<DateTime<Utc>>,
    pub active_secs: i64,
    pub active_total: i64,
    pub elapsed_total: i64,
}

/// One timer control, for a task's timer or a plan block's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerStep {
    Start,
    Pause,
    Stop,
}

impl TimerStep {
    /// The step a timer event names, task or block; `None` for any other event.
    pub fn of(event_type: &str) -> Option<TimerStep> {
        match event_type {
            "task_timer_started" | "block_timer_started" => Some(TimerStep::Start),
            "task_timer_paused" | "block_timer_paused" => Some(TimerStep::Pause),
            "task_timer_stopped" | "block_timer_stopped" => Some(TimerStep::Stop),
            _ => None,
        }
    }
}

impl TimerState {
    /// Apply a start, pause or stop at `at`. A repeat of the current state is a no-op, so a
    /// double tap or a replay changes nothing.
    pub fn step(&mut self, step: TimerStep, at: DateTime<Utc>) {
        match step {
            TimerStep::Start if !self.running => {
                self.session_start.get_or_insert(at);
                self.running_since = Some(at);
                self.running = true;
            }
            TimerStep::Pause if self.running => {
                self.close_span(at);
            }
            TimerStep::Stop => {
                if self.running {
                    self.close_span(at);
                }
                if let Some(start) = self.session_start.take() {
                    self.active_total += self.active_secs;
                    self.elapsed_total += (at - start).num_seconds().max(0);
                }
                self.active_secs = 0;
            }
            _ => {}
        }
    }

    fn close_span(&mut self, at: DateTime<Utc>) {
        if let Some(since) = self.running_since.take() {
            self.active_secs += (at - since).num_seconds().max(0);
        }
        self.running = false;
    }

    /// "running", "paused", or `None` with no session open.
    pub fn label(&self) -> Option<&'static str> {
        match (self.running, self.session_start) {
            (true, _) => Some("running"),
            (false, Some(_)) => Some("paused"),
            (false, None) => None,
        }
    }
}

#[derive(Debug, SurrealValue)]
struct TimerRow {
    timer_state: Option<String>,
    timer_session_start: Option<String>,
    timer_running_since: Option<String>,
    timer_active_secs: Option<i64>,
    active_secs_total: Option<i64>,
    elapsed_secs_total: Option<i64>,
}

fn parse_ts(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&Utc))
}

async fn on_timer(
    kind: &str,
    task_id: &str,
    at: DateTime<Utc>,
    db: &Database,
) -> Result<(), EventError> {
    let mut resp = db
        .query(
            "SELECT timer_state, timer_session_start, timer_running_since,
                    timer_active_secs, active_secs_total, elapsed_secs_total
             FROM type::record('tasks', $id)",
        )
        .bind(("id", task_id.to_string()))
        .await?
        .check()?;
    let rows: Vec<TimerRow> = resp.take(0)?;
    let Some(row) = rows.into_iter().next() else {
        tracing::warn!(task = %task_id, "timer event for a task this device has not seen");
        return Ok(());
    };
    let mut state = TimerState {
        running: row.timer_state.as_deref() == Some("running"),
        session_start: parse_ts(row.timer_session_start),
        running_since: parse_ts(row.timer_running_since),
        active_secs: row.timer_active_secs.unwrap_or(0),
        active_total: row.active_secs_total.unwrap_or(0),
        elapsed_total: row.elapsed_secs_total.unwrap_or(0),
    };
    if let Some(step) = TimerStep::of(kind) {
        state.step(step, at);
    }
    db.query(
        "UPDATE type::record('tasks', $id) SET
            timer_state = $state,
            timer_session_start = $session_start,
            timer_running_since = $running_since,
            timer_active_secs = $active,
            active_secs_total = $active_total,
            elapsed_secs_total = $elapsed_total,
            updated_at = type::datetime($ts)",
    )
    .bind(("id", task_id.to_string()))
    .bind(("state", state.label().map(str::to_string)))
    .bind(("session_start", state.session_start.map(|t| t.to_rfc3339())))
    .bind(("running_since", state.running_since.map(|t| t.to_rfc3339())))
    .bind(("active", state.active_secs))
    .bind(("active_total", state.active_total))
    .bind(("elapsed_total", state.elapsed_total))
    .bind(("ts", at.to_rfc3339()))
    .await?
    .check()?;
    Ok(())
}

async fn on_project_created(
    p: &TaskProjectCreatedPayload,
    event: &Event,
    db: &Database,
) -> Result<(), EventError> {
    db.query(
        "UPSERT type::record('task_projects', $id) SET
            project_id = $id,
            name = $name,
            course = $course,
            term = $term,
            ends_on = $ends_on,
            closed = closed ?? false,
            created_at = created_at ?? type::datetime($ts)",
    )
    .bind(("id", p.project_id.clone()))
    .bind(("name", p.name.clone()))
    .bind(("course", p.course.clone()))
    .bind(("term", p.term.clone()))
    .bind((
        "ends_on",
        p.ends_on.map(|d| d.format("%Y-%m-%d").to_string()),
    ))
    .bind(("ts", event.timestamp.to_rfc3339()))
    .await?
    .check()?;
    Ok(())
}

/// Replace the project's hub rows with the pushed index. Delete then upsert is idempotent, so a
/// failure part-way converges when the event is applied again.
async fn on_hub_index(
    p: &HubIndexReceivedPayload,
    event: &Event,
    db: &Database,
) -> Result<(), EventError> {
    db.query(
        "UPSERT type::record('hub_indexes', $project) SET
            project = $project,
            content_hash = $hash,
            updated = $updated,
            cycle = $cycle,
            received_at = type::datetime($ts),
            problems = $problems,
            task_count = $count;
         DELETE hub_tasks WHERE project = $project;",
    )
    .bind(("project", p.project.clone()))
    .bind(("hash", p.content_hash.clone()))
    .bind((
        "updated",
        p.updated.map(|d| d.format("%Y-%m-%d").to_string()),
    ))
    .bind(("cycle", p.cycle))
    .bind(("ts", event.timestamp.to_rfc3339()))
    .bind(("problems", p.problems.clone()))
    .bind(("count", p.tasks.len() as i64))
    .await?
    .check()?;
    for t in &p.tasks {
        db.query(
            "UPSERT type::record('hub_tasks', $id) SET
                hub_id = $id,
                project = $project,
                title = $title,
                status = $status,
                size = $size,
                due = $due,
                note = $note,
                source_ref = $reference,
                origin = $origin,
                tags = $tags,
                blocked_by = $blocked_by,
                needs_you = $needs_you,
                is_open = $open",
        )
        .bind(("id", t.id.clone()))
        .bind(("project", p.project.clone()))
        .bind(("title", t.title.clone()))
        .bind(("status", t.status.clone()))
        .bind(("size", t.size.clone()))
        .bind(("due", t.due.map(|d| d.format("%Y-%m-%d").to_string())))
        .bind(("note", t.note.clone()))
        .bind(("reference", t.reference.clone()))
        .bind(("origin", t.origin.clone()))
        .bind(("tags", t.tags.clone()))
        .bind(("blocked_by", t.blocked_by.clone()))
        .bind(("needs_you", t.needs_you()))
        .bind(("open", t.is_open()))
        .await?
        .check()?;
    }
    Ok(())
}

/// Replace the source's deadline rows with this read.
async fn on_deadline_feed(
    p: &DeadlineFeedReceivedPayload,
    event: &Event,
    db: &Database,
) -> Result<(), EventError> {
    db.query(
        "UPSERT type::record('deadline_feeds', $source) SET
            source = $source,
            content_hash = $hash,
            received_at = type::datetime($ts),
            problems = $problems;
         DELETE feed_deadlines WHERE source = $source;",
    )
    .bind(("source", p.source.clone()))
    .bind(("hash", p.content_hash.clone()))
    .bind(("ts", event.timestamp.to_rfc3339()))
    .bind(("problems", p.problems.clone()))
    .await?
    .check()?;
    for d in &p.deadlines {
        let (due_date, due_time) = due_columns(Some(&d.due));
        db.query(
            "UPSERT type::record('feed_deadlines', $uid) SET
                uid = $uid,
                source = $source,
                course = $course,
                term = $term,
                title = $title,
                due_date = $due_date,
                due_time = $due_time",
        )
        .bind(("uid", d.uid.clone()))
        .bind(("source", p.source.clone()))
        .bind(("course", d.course.clone()))
        .bind(("term", d.term.clone()))
        .bind(("title", d.title.clone()))
        .bind(("due_date", due_date.unwrap_or_default()))
        .bind(("due_time", due_time))
        .await?
        .check()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventType, NewEvent};

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    /// A session of 09:00-09:50 with a 10-minute pause: 40 active, 50 elapsed.
    #[test]
    fn a_paused_session_counts_active_and_elapsed_apart() {
        let mut t = TimerState::default();
        t.step(TimerStep::Start, at("2026-11-21T09:00:00Z"));
        t.step(TimerStep::Start, at("2026-11-21T09:05:00Z"));
        t.step(TimerStep::Pause, at("2026-11-21T09:20:00Z"));
        assert_eq!(t.label(), Some("paused"));
        t.step(TimerStep::Pause, at("2026-11-21T09:25:00Z"));
        t.step(TimerStep::Start, at("2026-11-21T09:30:00Z"));
        t.step(TimerStep::Stop, at("2026-11-21T09:50:00Z"));
        assert_eq!(t.label(), None);
        assert_eq!(t.active_total, 40 * 60);
        assert_eq!(t.elapsed_total, 50 * 60);
        t.step(TimerStep::Stop, at("2026-11-21T10:00:00Z"));
        assert_eq!(t.elapsed_total, 50 * 60, "a second stop adds nothing");
    }

    fn envelope(new: NewEvent, ts: &str) -> Event {
        Event {
            id: ulid::Ulid::new().to_string(),
            event_type: new.event_type,
            aggregate_id: new.aggregate_id,
            timestamp: at(ts),
            device_id: new.device_id,
            payload: new.payload,
            received_at: None,
        }
    }

    fn event(kind: EventType, aggregate: &str, payload: serde_json::Value, ts: &str) -> Event {
        envelope(
            NewEvent {
                id: None,
                event_type: kind.to_string(),
                aggregate_id: aggregate.into(),
                timestamp: at(ts),
                device_id: "phone".into(),
                payload,
            },
            ts,
        )
    }

    #[derive(Debug, SurrealValue)]
    struct Row {
        title: String,
        status: String,
        due_date: Option<String>,
        due_time: Option<String>,
        scheduled: Option<String>,
        note: Option<String>,
        actual_min: Option<i64>,
        active_secs_total: i64,
        elapsed_secs_total: i64,
    }

    async fn row(db: &Database, id: &str) -> Row {
        let mut resp = db
            .query(
                "SELECT title, status, due_date, due_time, scheduled, note, actual_min,
                        active_secs_total, elapsed_secs_total
                 FROM type::record('tasks', $id)",
            )
            .bind(("id", id.to_string()))
            .await
            .unwrap();
        let rows: Vec<Row> = resp.take(0).unwrap();
        rows.into_iter().next().expect("task row")
    }

    #[tokio::test]
    async fn a_task_is_created_changed_timed_and_closed() {
        let db = crate::db::test_db().await;
        TasksProjection.init_schema(&db).await.unwrap();
        let apply = |e: Event| {
            let db = db.clone();
            async move { TasksProjection.apply(&e, &db).await.unwrap() }
        };
        let id = "01TASK";
        apply(event(
            EventType::TaskCreated,
            id,
            serde_json::json!({"task_id": id, "title": "ME 641 take-home",
                               "due": "2026-12-04 19:00", "note": "the day's main focus"}),
            "2026-11-21T10:00:00Z",
        ))
        .await;
        let r = row(&db, id).await;
        assert_eq!(r.status, "todo");
        assert_eq!(r.due_date.as_deref(), Some("2026-12-04"));
        assert_eq!(r.due_time.as_deref(), Some("19:00"));

        apply(event(
            EventType::TaskModified,
            id,
            serde_json::json!({"task_id": id, "scheduled": "2026-12-03", "clear": ["note"]}),
            "2026-11-21T11:00:00Z",
        ))
        .await;
        let r = row(&db, id).await;
        assert_eq!(r.scheduled.as_deref(), Some("2026-12-03"));
        assert_eq!(r.note, None);
        assert_eq!(
            r.title, "ME 641 take-home",
            "an unnamed field is left alone"
        );
        assert_eq!(r.due_date.as_deref(), Some("2026-12-04"));

        let timer = |kind: EventType, ts: &'static str| {
            event(kind, id, serde_json::json!({"task_id": id}), ts)
        };
        apply(timer(EventType::TaskTimerStarted, "2026-12-03T14:00:00Z")).await;
        apply(timer(EventType::TaskTimerPaused, "2026-12-03T15:00:00Z")).await;
        apply(timer(EventType::TaskTimerStarted, "2026-12-03T15:30:00Z")).await;
        apply(timer(EventType::TaskTimerStopped, "2026-12-03T16:30:00Z")).await;
        apply(event(
            EventType::TaskStatusSet,
            id,
            serde_json::json!({"task_id": id, "status": "done", "actual_min": 150}),
            "2026-12-03T16:31:00Z",
        ))
        .await;
        let r = row(&db, id).await;
        assert_eq!(r.status, "done");
        assert_eq!(r.actual_min, Some(150));
        assert_eq!(r.active_secs_total, 2 * 3600);
        assert_eq!(r.elapsed_secs_total, 150 * 60);
    }

    #[tokio::test]
    async fn a_pushed_index_replaces_its_project_and_no_other() {
        let db = crate::db::test_db().await;
        TasksProjection.init_schema(&db).await.unwrap();
        let push = |project: &str, ids: &[&str], ts: &str| {
            let tasks: Vec<serde_json::Value> = ids
                .iter()
                .map(|id| serde_json::json!({"id": id, "title": id, "status": "todo"}))
                .collect();
            event(
                EventType::HubIndexReceived,
                project,
                serde_json::json!({"project": project, "tasks": tasks,
                                   "content_hash": format!("h-{ts}")}),
                ts,
            )
        };
        for e in [
            push("a", &["a-0001", "a-0002"], "2026-11-21T10:00:00Z"),
            push("b", &["b-0001"], "2026-11-21T10:00:00Z"),
            push("a", &["a-0002", "a-0003"], "2026-11-21T11:00:00Z"),
        ] {
            TasksProjection.apply(&e, &db).await.unwrap();
        }
        let mut resp = db
            .query("SELECT VALUE hub_id FROM hub_tasks ORDER BY hub_id")
            .await
            .unwrap();
        let ids: Vec<String> = resp.take(0).unwrap();
        assert_eq!(ids, ["a-0002", "a-0003", "b-0001"]);
    }
}
