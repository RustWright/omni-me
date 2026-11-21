//! Routine items ticked from journal lines, and what those ticks teach the planner.
//!
//! Ruling 127 (RULINGS.md): a line saying an item is done ticks it at that line's time, marked
//! inferred and undoable; the journal is never written. Ruling 114: the morning routine's real
//! length is learned from those ticks. The model is the structurer seat (role D), which already
//! reads the journal; on a zero-retention provider (ruling 107).

use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use crate::db::Database;
use crate::events::{EventType, EventWriter, NewEvent, RoutineItemCompletedPayload};
use crate::llm::LlmClient;

/// One stamped journal line.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalLine {
    /// Local wall-clock time, as the editor stamped it.
    pub at: NaiveDateTime,
    pub text: String,
}

/// The stamped lines of a journal body, in order. Unstamped lines are not dated, so they cannot
/// tick anything and are left out. The stamp is `⟦YYYY-MM-DD HH:MM[ TZ]⟧`, written by the
/// editor (`TS_TOKEN_RE` in `assets/js/editor.js`).
pub fn stamped_lines(body: &str) -> Vec<JournalLine> {
    body.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix('⟦')?;
            let (inner, text) = rest.split_once('⟧')?;
            let at = NaiveDateTime::parse_from_str(inner.get(..16)?, "%Y-%m-%d %H:%M").ok()?;
            let text = text.trim();
            (!text.is_empty()).then(|| JournalLine {
                at,
                text: text.to_string(),
            })
        })
        .collect()
}

/// A routine item the model may tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub item_id: String,
    pub group_id: String,
    pub name: String,
    pub group_name: String,
}

fn prompt(items: &[Item], lines: &[JournalLine]) -> String {
    let mut p = String::from(
        "Below are the user's routine items and some lines from their journal, both numbered. \
         Say which lines report that the user did an item: done, in the past tense. A plan, a \
         wish, or something still to do ticks nothing. One line may report several items. \
         Return only pairs you are sure of; return an empty list when there are none.\n\nItems:\n",
    );
    for (i, item) in items.iter().enumerate() {
        p.push_str(&format!("{}. {} ({})\n", i + 1, item.name, item.group_name));
    }
    p.push_str("\nJournal lines:\n");
    for (i, line) in lines.iter().enumerate() {
        p.push_str(&format!(
            "{}. {} {}\n",
            i + 1,
            line.at.format("%H:%M"),
            line.text
        ));
    }
    p
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ticks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "item": { "type": "integer" },
                        "line": { "type": "integer" }
                    },
                    "required": ["item", "line"]
                }
            }
        },
        "required": ["ticks"]
    })
}

/// The model's pairs as zero-based (item, line), dropping any number out of range and keeping
/// the first line per item.
fn read_pairs(answer: &Value, items: usize, lines: usize) -> Vec<(usize, usize)> {
    let mut seen = BTreeSet::new();
    answer["ticks"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|t| {
            let item = usize::try_from(t["item"].as_u64()?).ok()?.checked_sub(1)?;
            let line = usize::try_from(t["line"].as_u64()?).ok()?.checked_sub(1)?;
            (item < items && line < lines && seen.insert(item)).then_some((item, line))
        })
        .collect()
}

#[derive(SurrealValue)]
struct State {
    last_line: Option<String>,
}

/// Create the job's own bookkeeping table. Not synced: it only stops a line being read twice.
pub async fn init_schema(db: &Database) -> Result<(), String> {
    db.query(
        "DEFINE TABLE IF NOT EXISTS job_journal_ticks SCHEMAFULL;
         DEFINE FIELD IF NOT EXISTS last_line ON job_journal_ticks TYPE option<string>;",
    )
    .await
    .and_then(|r| r.check())
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// What one run did, for the log line.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub lines_read: usize,
    pub ticked: usize,
}

/// Read the stamped lines of `dates` not read before, and tick what they report.
///
/// A line is read once: the watermark moves past it whether or not it ticked anything, so an
/// inferred tick he undoes is never ticked again from the same line. A model failure leaves the
/// watermark where it was, so the lines are read on the next run.
pub async fn run(
    db: &Database,
    writer: &EventWriter,
    llm: &dyn LlmClient,
    dates: &[NaiveDate],
    tz: Tz,
) -> Result<Report, String> {
    let items = items(db).await?;
    let mut report = Report::default();
    if items.is_empty() {
        return Ok(report);
    }
    for date in dates {
        let key = date.format("%Y-%m-%d").to_string();
        let mut resp = db
            .query(
                "SELECT VALUE raw_text FROM journal_entries WHERE date = $d;
                 SELECT last_line FROM type::record('job_journal_ticks', $d);
                 SELECT VALUE item_id FROM routine_completions WHERE date = $d;",
            )
            .bind(("d", key.clone()))
            .await
            .map_err(|e| e.to_string())?;
        let bodies: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
        let state: Vec<State> = resp.take(1).map_err(|e| e.to_string())?;
        let done: Vec<String> = resp.take(2).map_err(|e| e.to_string())?;
        let watermark = state
            .into_iter()
            .next()
            .and_then(|s| s.last_line)
            .and_then(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M").ok());

        let lines: Vec<JournalLine> = bodies
            .iter()
            .flat_map(|b| stamped_lines(b.as_str()))
            .filter(|l| watermark.is_none_or(|w| l.at > w))
            .collect();
        let Some(newest) = lines.iter().map(|l| l.at).max() else {
            continue;
        };
        let open: Vec<&Item> = items
            .iter()
            .filter(|i| !done.contains(&i.item_id))
            .collect();
        if !open.is_empty() {
            let open_owned: Vec<Item> = open.iter().map(|i| (*i).clone()).collect();
            let answer = llm
                .complete_json(&prompt(&open_owned, &lines), &schema())
                .await
                .map_err(|e| e.to_string())?;
            for (i, l) in read_pairs(&answer, open_owned.len(), lines.len()) {
                let (item, line) = (&open_owned[i], &lines[l]);
                tick(writer, item, *date, line, tz).await?;
                report.ticked += 1;
            }
        }
        report.lines_read += lines.len();
        db.query("UPSERT type::record('job_journal_ticks', $d) SET last_line = $at")
            .bind(("d", key))
            .bind(("at", newest.format("%Y-%m-%d %H:%M").to_string()))
            .await
            .and_then(|r| r.check())
            .map_err(|e| e.to_string())?;
    }
    Ok(report)
}

#[derive(SurrealValue)]
struct ItemRow {
    item_id: String,
    group_id: String,
    name: String,
    order_num: i64,
}

#[derive(SurrealValue)]
struct GroupRow {
    group_id: String,
    name: String,
}

/// Every live item of every live routine, in routine then item order.
async fn items(db: &Database) -> Result<Vec<Item>, String> {
    let mut resp = db
        .query(
            "SELECT meta::id(id) AS item_id, group_id, name, order_num FROM routine_items
             WHERE removed != true;
             SELECT meta::id(id) AS group_id, name FROM routine_groups WHERE removed != true;",
        )
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<ItemRow> = resp.take(0).map_err(|e| e.to_string())?;
    let groups: Vec<GroupRow> = resp.take(1).map_err(|e| e.to_string())?;
    let mut out: Vec<(String, i64, Item)> = rows
        .into_iter()
        .filter_map(|r| {
            let group = groups.iter().find(|g| g.group_id == r.group_id)?;
            Some((
                group.name.clone(),
                r.order_num,
                Item {
                    item_id: r.item_id,
                    group_id: r.group_id,
                    name: r.name,
                    group_name: group.name.clone(),
                },
            ))
        })
        .collect();
    out.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    Ok(out.into_iter().map(|(_, _, item)| item).collect())
}

async fn tick(
    writer: &EventWriter,
    item: &Item,
    date: NaiveDate,
    line: &JournalLine,
    tz: Tz,
) -> Result<(), String> {
    let completed_at = line
        .at
        .and_local_timezone(tz)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| DateTime::<Utc>::from_naive_utc_and_offset(line.at, Utc));
    let payload = RoutineItemCompletedPayload {
        item_id: item.item_id.clone(),
        group_id: item.group_id.clone(),
        date,
        completed_at,
        inferred: true,
        source_line: Some(format!("{} {}", line.at.format("%H:%M"), line.text)),
    };
    writer
        .append_new(NewEvent {
            id: None,
            event_type: EventType::RoutineItemCompleted.to_string(),
            aggregate_id: item.item_id.clone(),
            timestamp: Utc::now(),
            device_id: writer.device_id().to_string(),
            payload: serde_json::to_value(&payload).map_err(|e| e.to_string())?,
        })
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// One day's evidence for the morning's length: when he started and when the routine ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MorningSample {
    pub first_activity: NaiveDateTime,
    pub last_tick: NaiveDateTime,
}

/// Fewer days than this and the estimate falls back to his item estimates.
pub const MIN_SAMPLES: usize = 3;

/// The morning routine's learned length in minutes: the median gap from first activity to the
/// last morning tick, over the samples given. A median, since one slow morning should not move
/// every plan; `None` with too few days. Ruling 114 (RULINGS.md).
pub fn morning_minutes(samples: &[MorningSample]) -> Option<u32> {
    let mut gaps: Vec<i64> = samples
        .iter()
        .map(|s| (s.last_tick - s.first_activity).num_minutes())
        .filter(|m| *m > 0)
        .collect();
    if gaps.len() < MIN_SAMPLES {
        return None;
    }
    gaps.sort_unstable();
    let mid = gaps.len() / 2;
    let median = if gaps.len() % 2 == 0 {
        (gaps[mid - 1] + gaps[mid]) / 2
    } else {
        gaps[mid]
    };
    u32::try_from(median).ok()
}

/// The samples for one routine over `dates`: days with a journal line and a tick of its items.
pub async fn morning_samples(
    db: &Database,
    group_id: &str,
    dates: &[NaiveDate],
    tz: Tz,
) -> Result<Vec<MorningSample>, String> {
    let mut out = Vec::new();
    for date in dates {
        let key = date.format("%Y-%m-%d").to_string();
        let mut resp = db
            .query(
                "SELECT VALUE raw_text FROM journal_entries WHERE date = $d;
                 SELECT VALUE <string> completed_at FROM routine_completions
                 WHERE date = $d AND group_id = $g AND skipped != true;",
            )
            .bind(("d", key))
            .bind(("g", group_id.to_string()))
            .await
            .map_err(|e| e.to_string())?;
        let bodies: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
        let ticks: Vec<String> = resp.take(1).map_err(|e| e.to_string())?;
        let first = bodies
            .iter()
            .flat_map(|b| stamped_lines(b.as_str()))
            .map(|l| l.at)
            .min();
        let last = ticks
            .iter()
            .filter_map(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&tz).naive_local())
            .max();
        if let (Some(first_activity), Some(last_tick)) = (first, last) {
            out.push(MorningSample {
                first_activity,
                last_tick,
            });
        }
    }
    Ok(out)
}

/// Items of daily routines with no tick or skip on `date`: what the evening shutdown asks one line
/// about, so an unjournaled day is not read as skipped. Ruling 127 (RULINGS.md).
pub async fn unevidenced(db: &Database, date: NaiveDate) -> Result<Vec<Item>, String> {
    let all = items(db).await?;
    let mut resp = db
        .query(
            "SELECT VALUE item_id FROM routine_completions WHERE date = $d;
             SELECT VALUE meta::id(id) FROM routine_groups WHERE frequency = 'daily';",
        )
        .bind(("d", date.format("%Y-%m-%d").to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let evidenced: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
    let daily: Vec<String> = resp.take(1).map_err(|e| e.to_string())?;
    Ok(all
        .into_iter()
        .filter(|i| daily.contains(&i.group_id) && !evidenced.contains(&i.item_id))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn only_stamped_lines_are_read() {
        let body = "## What happened today?\n\n\
                    ⟦2026-11-06 08:03 EST⟧I'm awake now\n\n\
                    an unstamped line\n\
                    ⟦2026-11-06 10:40 EST⟧I completed my brilliant session, then meditated\n\
                    ⟦not a stamp⟧prose\n\
                    ⟦2026-11-06 10:41 EST⟧   \n";
        let lines = stamped_lines(body);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].at, at("2026-11-06 08:03"));
        assert_eq!(
            lines[1].text,
            "I completed my brilliant session, then meditated"
        );
    }

    #[test]
    fn the_models_pairs_are_bounded_and_one_per_item() {
        let answer = json!({"ticks": [
            {"item": 1, "line": 2},
            {"item": 1, "line": 3},
            {"item": 2, "line": 2},
            {"item": 9, "line": 1},
            {"item": 0, "line": 1},
            {"item": 3, "line": 7}
        ]});
        assert_eq!(read_pairs(&answer, 3, 3), vec![(0, 1), (1, 1)]);
        assert!(read_pairs(&json!({"nope": 1}), 3, 3).is_empty());
    }

    fn sample(first: &str, last: &str) -> MorningSample {
        MorningSample {
            first_activity: at(first),
            last_tick: at(last),
        }
    }

    /// His mornings run about two hours against item estimates of about 95 minutes.
    #[test]
    fn the_morning_is_the_median_gap_and_needs_three_days() {
        let s = [
            sample("2026-11-10 08:10", "2026-11-10 10:10"),
            sample("2026-11-11 11:08", "2026-11-11 12:38"),
        ];
        assert_eq!(morning_minutes(&s), None);
        let s = [
            s[0],
            s[1],
            sample("2026-11-12 09:00", "2026-11-12 11:55"),
            sample("2026-11-13 09:00", "2026-11-13 08:00"),
        ];
        assert_eq!(
            morning_minutes(&s),
            Some(120),
            "the negative gap is ignored"
        );
    }

    struct Scripted(Value);

    #[async_trait::async_trait]
    impl LlmClient for Scripted {
        fn model_name(&self) -> &str {
            "scripted"
        }
        async fn complete(&self, _p: &str) -> Result<String, crate::llm::LlmError> {
            unimplemented!()
        }
        async fn complete_json(&self, _p: &str, _s: &Value) -> Result<Value, crate::llm::LlmError> {
            Ok(self.0.clone())
        }
        async fn complete_with_tools(
            &self,
            _p: &str,
            _t: &[crate::llm::ToolDef],
        ) -> Result<crate::llm::LlmResponse, crate::llm::LlmError> {
            unimplemented!()
        }
        async fn chat(
            &self,
            _r: &crate::llm::ChatRequest,
        ) -> Result<crate::llm::ChatResponse, crate::llm::LlmError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn a_line_ticks_at_its_time_once_and_never_again() {
        use crate::config::ALL_FEATURES;
        use crate::events::{
            NotesProjection, ProjectionRunner, RoutinesProjection, SurrealEventStore,
        };
        use std::sync::Arc;

        let db = crate::db::test_db().await;
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(NotesProjection), Box::new(RoutinesProjection)],
        );
        runner.init_all().await.unwrap();
        init_schema(&db).await.unwrap();
        let writer = EventWriter::new(
            Arc::new(SurrealEventStore::new(db.clone())),
            runner,
            ALL_FEATURES.iter().copied().collect(),
            "server",
        );
        db.query(
            "CREATE routine_groups:g SET name = 'Morning', frequency = 'daily', order_num = 0,
                 removed = false, created_at = time::now(), updated_at = time::now();
             CREATE routine_items:a SET group_id = 'g', name = 'Brilliant session',
                 estimated_duration_min = 10, order_num = 0, removed = false;
             CREATE routine_items:b SET group_id = 'g', name = 'Meditation',
                 estimated_duration_min = 15, order_num = 1, removed = false;
             CREATE journal_entries:d SET journal_id = 'j', date = '2026-11-06',
                 raw_text = '⟦2026-11-06 08:03 EST⟧I am awake\n⟦2026-11-06 10:40 EST⟧Did my brilliant session',
                 tags = [], closed = false, complete = false,
                 created_at = time::now(), updated_at = time::now();",
        )
        .await
        .unwrap()
        .check()
        .unwrap();
        let llm = Scripted(json!({"ticks": [{"item": 1, "line": 2}]}));
        let day = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let tz = chrono_tz::America::Toronto;

        let r = run(&db, &writer, &llm, &[day], tz).await.unwrap();
        assert_eq!(
            r,
            Report {
                lines_read: 2,
                ticked: 1
            }
        );
        #[derive(SurrealValue)]
        struct Tick {
            item_id: String,
            completed_at: String,
            inferred: Option<bool>,
            source_line: Option<String>,
        }
        let mut resp = db
            .query(
                "SELECT item_id, <string> completed_at AS completed_at, inferred, source_line
                 FROM routine_completions",
            )
            .await
            .unwrap();
        let rows: Vec<Tick> = resp.take(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].item_id, "a");
        assert!(rows[0].completed_at.starts_with("2026-11-06T15:40:00"));
        assert_eq!(rows[0].inferred, Some(true));
        assert_eq!(
            rows[0].source_line.as_deref(),
            Some("10:40 Did my brilliant session")
        );

        // Undone by him; the same lines are not read again, so it stays undone.
        db.query("DELETE routine_completions").await.unwrap();
        let again = run(&db, &writer, &llm, &[day], tz).await.unwrap();
        assert_eq!(again, Report::default());

        let missing = unevidenced(&db, day).await.unwrap();
        assert_eq!(missing.len(), 2);
    }
}
