//! Day plans: the `plan.set` proposal, what its times resolve to, and where each plan stands
//! (rulings 97, 98, 100, 117). Rationale: `docs/src/planning.md` § Plans.
//!
//! A plan is a proposal like any other, so it inherits the approval gate, the inbox and sync.
//! Proposed, skipped, lapsed and superseded are read off the proposal and its decision; only an
//! accepted plan writes a record of its own, `day_plan_accepted`, which reminders and the view read.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use super::day::DayContext;
use super::fit::{Block, DayWindow};
use crate::calendar::day_start;
use crate::config::ResolvedConfig;
use crate::db::Database;

/// The action's name, matched where a plan is checked: `verbs::propose` and `inbox::decide`.
pub const PLAN_SET: &str = "plan.set";

/// Decision reasons a plan's rejection carries, so the record says why it closed.
pub const REASON_SKIPPED: &str = "skip today";
pub const REASON_LAPSED: &str = "lapsed";
pub const REASON_SUPERSEDED: &str = "superseded";

/// Most blocks one plan may hold: a card he cannot check at a glance is not an approval.
pub const MAX_BLOCKS: usize = 12;
/// Most untimed items one plan may hold.
pub const MAX_UNTIMED: usize = 15;
/// Longest one untimed estimate may be. Longer work belongs in a block.
pub const MAX_UNTIMED_MIN: u32 = 120;

fn parse_hm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

/// A block as the model writes it: `HH:MM-HH:MM Title`, in his local time.
pub fn parse_block(text: &str) -> Result<(NaiveTime, NaiveTime, String), String> {
    let text = text.trim();
    let (times, title) = text
        .split_once(' ')
        .ok_or_else(|| "must read HH:MM-HH:MM followed by a title".to_string())?;
    let (start, end) = times
        .split_once(['-', '\u{2013}'])
        .ok_or_else(|| "must read HH:MM-HH:MM followed by a title".to_string())?;
    let (Some(start), Some(end)) = (parse_hm(start), parse_hm(end)) else {
        return Err("must start with two 24-hour times, like 14:00-15:30".to_string());
    };
    let title = title.trim();
    if title.is_empty() {
        return Err("needs a title after its times".to_string());
    }
    Ok((start, end, title.to_string()))
}

pub fn valid_block(text: &str) -> Result<(), String> {
    parse_block(text).map(|_| ())
}

/// An untimed item as the model writes it: `Title (N min)`.
pub fn parse_untimed(text: &str) -> Result<UntimedItem, String> {
    let text = text.trim();
    let shape = || "must read: title (N min), like \"Email the TA (10 min)\"".to_string();
    let inner = text.strip_suffix("min)").ok_or_else(shape)?;
    let open = inner.rfind('(').ok_or_else(shape)?;
    let minutes: u32 = inner[open + 1..].trim().parse().map_err(|_| shape())?;
    if minutes == 0 || minutes > MAX_UNTIMED_MIN {
        return Err(format!(
            "estimate must be 1 to {MAX_UNTIMED_MIN} minutes; longer work belongs in a block"
        ));
    }
    let title = inner[..open].trim();
    if title.is_empty() {
        return Err(shape());
    }
    Ok(UntimedItem {
        title: title.to_string(),
        minutes,
    })
}

pub fn valid_untimed(text: &str) -> Result<(), String> {
    parse_untimed(text).map(|_| ())
}

/// One small thing on the untimed list (ruling 97).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UntimedItem {
    pub title: String,
    pub minutes: u32,
}

/// An accepted plan, as `day_plan_accepted` carries it. Blocks are instants, resolved at
/// approval, so a later change of zone or bedtime cannot move them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceptedPlan {
    pub date: NaiveDate,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub untimed: Vec<UntimedItem>,
}

fn local(date: NaiveDate, t: NaiveTime, tz: Tz) -> DateTime<Utc> {
    tz.from_local_datetime(&date.and_time(t))
        .earliest()
        .map_or_else(|| date.and_time(t).and_utc(), |x| x.with_timezone(&Utc))
}

/// Local block texts on `date` as instants. A time before the day's window that fits after
/// midnight is read as after midnight, so a bedtime past 00:00 can be planned; an end at or
/// before its start ends the next day.
pub fn resolve_blocks(
    date: NaiveDate,
    tz: Tz,
    bounds: Option<DayWindow>,
    texts: &[String],
) -> Result<Vec<Block>, String> {
    let mut out = Vec::with_capacity(texts.len());
    for text in texts {
        let (s, e, title) = parse_block(text).map_err(|why| format!("\"{text}\" {why}"))?;
        let mut start = local(date, s, tz);
        if let Some(w) = bounds
            && start < w.start
            && start + Duration::days(1) <= w.end
        {
            start += Duration::days(1);
        }
        let mut end = local(start.with_timezone(&tz).date_naive(), e, tz);
        if end <= start {
            end += Duration::days(1);
        }
        out.push(Block { start, end, title });
    }
    Ok(out)
}

/// Where a plan stands (rulings 98, 117).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Waiting on him.
    Proposed,
    /// In force for its day.
    Accepted,
    /// A later plan for the same day replaced it; its approval is refused.
    Superseded,
    /// He chose to run the day without one.
    Skipped,
    /// Unanswered when its first block would have started.
    Lapsed,
}

/// One `plan.set` proposal, as status needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanProposal {
    pub proposal_id: String,
    /// The conversation it came out of: the morning ritual's push finds its own plan by thread.
    pub thread_id: String,
    pub date: NaiveDate,
    pub created_at: DateTime<Utc>,
    /// Local block texts, as proposed.
    pub blocks: Vec<String>,
    pub decision: Option<String>,
    pub decided_reason: Option<String>,
}

impl PlanProposal {
    fn is_newer_than(&self, other: &PlanProposal) -> bool {
        (self.created_at, &self.proposal_id) > (other.created_at, &other.proposal_id)
    }
}

/// `plan` among every plan proposed for its date. `first_start` is when its first block
/// starts, `None` for a plan with no blocks, which lapses when its day ends (`day_end`).
pub fn status(
    plan: &PlanProposal,
    same_day: &[PlanProposal],
    first_start: Option<DateTime<Utc>>,
    day_end: DateTime<Utc>,
    now: DateTime<Utc>,
) -> PlanStatus {
    let newer: Vec<&PlanProposal> = same_day.iter().filter(|p| p.is_newer_than(plan)).collect();
    match plan.decision.as_deref() {
        Some("approved") => {
            if newer
                .iter()
                .any(|p| p.decision.as_deref() == Some("approved"))
            {
                PlanStatus::Superseded
            } else {
                PlanStatus::Accepted
            }
        }
        Some(_) => match plan.decided_reason.as_deref() {
            Some(REASON_LAPSED) => PlanStatus::Lapsed,
            Some(REASON_SUPERSEDED) => PlanStatus::Superseded,
            _ => PlanStatus::Skipped,
        },
        None => {
            if !newer.is_empty() {
                PlanStatus::Superseded
            } else if now >= first_start.unwrap_or(day_end) {
                PlanStatus::Lapsed
            } else {
                PlanStatus::Proposed
            }
        }
    }
}

const PLAN_COLUMNS: &str = "SELECT proposal_id, thread_id, args, \
     <string> created_at AS created_at, decision, decided_reason FROM assistant_proposals";

/// Every plan proposed for `date`, newest first.
pub async fn proposals_for(db: &Database, date: NaiveDate) -> Result<Vec<PlanProposal>, String> {
    query_plans(
        db,
        "WHERE action = $action AND args.date = $date",
        Some(date),
    )
    .await
}

/// Every plan still waiting on him, any date.
pub async fn pending(db: &Database) -> Result<Vec<PlanProposal>, String> {
    query_plans(db, "WHERE action = $action AND decision IS NONE", None).await
}

/// Read column by column, as `assistant::inbox` does, since `args` is a free-form object.
async fn query_plans(
    db: &Database,
    filter: &str,
    date: Option<NaiveDate>,
) -> Result<Vec<PlanProposal>, String> {
    let sql = format!("{PLAN_COLUMNS} {filter} ORDER BY created_at DESC");
    let query = db.query(sql).bind(("action", PLAN_SET.to_string()));
    let query = match date {
        Some(d) => query.bind(("date", d.to_string())),
        None => query,
    };
    let mut resp = query
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let ids: Vec<String> = resp.take("proposal_id").unwrap_or_default();
    let threads: Vec<Option<String>> = resp.take("thread_id").unwrap_or_default();
    let args: Vec<Option<serde_json::Value>> = resp.take("args").unwrap_or_default();
    let created: Vec<Option<String>> = resp.take("created_at").unwrap_or_default();
    let decisions: Vec<Option<String>> = resp.take("decision").unwrap_or_default();
    let reasons: Vec<Option<String>> = resp.take("decided_reason").unwrap_or_default();
    Ok(ids
        .into_iter()
        .enumerate()
        .filter_map(|(i, id)| {
            plan_proposal(
                id,
                threads.get(i).cloned().flatten(),
                args.get(i).cloned().flatten(),
                created.get(i).cloned().flatten(),
                decisions.get(i).cloned().flatten(),
                reasons.get(i).cloned().flatten(),
            )
        })
        .collect())
}

/// A row whose detail has not arrived, or whose date does not parse, is not a plan yet.
fn plan_proposal(
    proposal_id: String,
    thread_id: Option<String>,
    args: Option<serde_json::Value>,
    created_at: Option<String>,
    decision: Option<String>,
    decided_reason: Option<String>,
) -> Option<PlanProposal> {
    let args = args?;
    let date = args.get("date")?.as_str()?.parse().ok()?;
    let created_at = DateTime::parse_from_rfc3339(created_at.as_deref()?)
        .ok()?
        .with_timezone(&Utc);
    let blocks = args
        .get("blocks")
        .and_then(|b| b.as_array())
        .map(|b| {
            b.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(PlanProposal {
        proposal_id,
        thread_id: thread_id?,
        date,
        created_at,
        blocks,
        decision,
        decided_reason,
    })
}

/// What `propose` learns from checking a plan: how it fits, never a reason to refuse (117).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Checked {
    pub fit: super::fit::Fit,
    /// The one line shown on the card when the plan asks for more than the day has.
    pub over: Option<String>,
}

/// Check a `plan.set` proposal against its day on the turn it is made (ruling 112): each
/// refusal comes back as a sentence the model can act on, naming the block.
pub async fn check_proposal(
    db: &Database,
    config: &ResolvedConfig,
    args: &serde_json::Value,
    now: DateTime<Utc>,
) -> Result<Checked, String> {
    let date: NaiveDate = args
        .get("date")
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse().ok())
        .ok_or_else(|| "the plan names no date".to_string())?;
    let tz = config.user_zone();
    if date < now.with_timezone(&tz).date_naive() {
        return Err(format!(
            "{date} has already passed; plan today or a later day"
        ));
    }
    let texts = |key: &str| -> Vec<String> {
        args.get(key)
            .and_then(|b| b.as_array())
            .map(|b| {
                b.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let (block_texts, untimed_texts) = (texts("blocks"), texts("untimed"));
    if block_texts.is_empty() && untimed_texts.is_empty() {
        return Err("a plan needs at least one block or one untimed item".to_string());
    }
    let ctx = DayContext::load(db, config, date).await?;
    let blocks = resolve_blocks(date, ctx.shape.tz, ctx.bounds(), &block_texts)?;
    let refusals = ctx.validate(&blocks, now);
    if !refusals.is_empty() {
        let lines: Vec<String> = refusals
            .iter()
            .map(|(i, why)| format!("\"{}\" {why}", block_texts[*i]))
            .collect();
        return Err(format!(
            "Nothing was proposed. Fix these blocks and propose the whole plan again: {}.",
            lines.join("; ")
        ));
    }
    let untimed_min: u32 = untimed_texts
        .iter()
        .filter_map(|t| parse_untimed(t).ok())
        .map(|u| u.minutes)
        .sum();
    let fit = match ctx.free_time(now) {
        Some(free) => super::fit::fit(&blocks, untimed_min, &free),
        None => super::fit::Fit {
            planned_min: i64::from(untimed_min),
            free_min: 0,
        },
    };
    let over = (fit.over_min() > 0).then(|| {
        format!(
            "Asks for {} min more than the {} min free.",
            fit.over_min(),
            fit.free_min
        )
    });
    Ok(Checked { fit, over })
}

/// Where `plan` stands now, against every plan for its date and its day's window.
pub async fn current_status(
    db: &Database,
    config: &ResolvedConfig,
    plan: &PlanProposal,
    now: DateTime<Utc>,
) -> Result<PlanStatus, String> {
    let same_day = proposals_for(db, plan.date).await?;
    let ctx = DayContext::load(db, config, plan.date).await?;
    let bounds = ctx.bounds();
    let first = resolve_blocks(plan.date, ctx.shape.tz, bounds, &plan.blocks)?
        .iter()
        .map(|b| b.start)
        .min();
    let day_end = bounds.map_or_else(
        || day_start(plan.date + Duration::days(1), ctx.shape.tz),
        |w| w.end,
    );
    Ok(status(plan, &same_day, first, day_end, now))
}

/// The proposal's blocks as instants, as the `resolved` argument carries them into the event.
/// Resolved at approval, against the day as it stands then.
pub async fn resolve_for_approval(
    db: &Database,
    config: &ResolvedConfig,
    args: &serde_json::Value,
) -> Result<String, String> {
    let date: NaiveDate = args
        .get("date")
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse().ok())
        .ok_or_else(|| "the plan names no date".to_string())?;
    let texts: Vec<String> = args
        .get("blocks")
        .and_then(|b| b.as_array())
        .map(|b| {
            b.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let ctx = DayContext::load(db, config, date).await?;
    let blocks = resolve_blocks(date, ctx.shape.tz, ctx.bounds(), &texts)?;
    serde_json::to_string(&blocks).map_err(|e| e.to_string())
}

#[derive(SurrealValue)]
struct PlanRow {
    body: String,
}

/// The plan in force for `date`: the last one accepted.
pub async fn accepted(db: &Database, date: NaiveDate) -> Result<Option<AcceptedPlan>, String> {
    let mut resp = db
        .query("SELECT body FROM type::record('day_plans', $date)")
        .bind(("date", date.to_string()))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let rows: Vec<PlanRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|r| serde_json::from_str(&r.body).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TZ: Tz = chrono_tz::America::Toronto;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn blocks_and_untimed_items_parse_or_say_why_not() {
        let (s, e, t) = parse_block("14:00-15:30 Essay draft").unwrap();
        assert_eq!(
            (s.to_string(), e.to_string(), t.as_str()),
            ("14:00:00".into(), "15:30:00".into(), "Essay draft")
        );
        assert!(parse_block("14:00\u{2013}15:30 En dash").is_ok());
        assert!(parse_block("2pm-3pm Essay").is_err());
        assert!(parse_block("14:00-15:30").is_err());

        let item = parse_untimed("Email the TA (10 min)").unwrap();
        assert_eq!(
            item,
            UntimedItem {
                title: "Email the TA".into(),
                minutes: 10
            }
        );
        assert!(parse_untimed("Email the TA").is_err());
        assert!(parse_untimed("Deep work (240 min)").is_err());
        assert!(parse_untimed("(5 min)").is_err());
    }

    #[test]
    fn a_time_before_the_window_that_fits_after_midnight_is_read_so() {
        // Window 10:00 to 01:00 the next morning, EST.
        let bounds = DayWindow {
            start: at("2026-11-23T15:00:00Z"),
            end: at("2026-11-24T06:00:00Z"),
        };
        let date = "2026-11-23".parse().unwrap();
        let blocks = resolve_blocks(
            date,
            TZ,
            Some(bounds),
            &[
                "23:30-00:30 Reading".into(),
                "00:15-00:45 Late".into(),
                "09:00-09:30 Early".into(),
            ],
        )
        .unwrap();
        assert_eq!(blocks[0].start, at("2026-11-24T04:30:00Z"));
        assert_eq!(blocks[0].end, at("2026-11-24T05:30:00Z"));
        assert_eq!(blocks[1].start, at("2026-11-24T05:15:00Z"));
        // 09:00 the next day is past the window's end, so it stays on the date and is refused
        // later as outside the window.
        assert_eq!(blocks[2].start, at("2026-11-23T14:00:00Z"));
    }

    /// Ruling 112 (RULINGS.md): a block the day cannot hold comes back named, before anything is
    /// proposed; a plan that only runs over is proposed with its fit.
    #[tokio::test]
    async fn a_plan_is_checked_against_its_day_on_the_turn_it_is_made() {
        use crate::config::ConfigMap;
        use crate::events::{PlanningProjection, ProjectionRunner};
        let db = crate::db::test_db().await;
        ProjectionRunner::new(db.clone(), vec![Box::new(PlanningProjection)])
            .init_all()
            .await
            .unwrap();
        // UTC by default: nothing done yet, so the day opens at the 11:00 fallback and ends at
        // the 23:30 bedtime.
        let config = ResolvedConfig::new(ConfigMap::new(), ConfigMap::new());
        let now = at("2026-11-23T09:00:00Z");
        let args = |blocks: &[&str], untimed: &[&str]| serde_json::json!({ "date": "2026-11-23", "blocks": blocks, "untimed": untimed });

        let err = check_proposal(
            &db,
            &config,
            &args(&["12:00-13:00 Essay", "10:00-10:30 Early"], &[]),
            now,
        )
        .await
        .unwrap_err();
        assert!(err.contains("\"10:00-10:30 Early\" falls outside"), "{err}");
        assert!(!err.contains("Essay"), "{err}");

        let ok = check_proposal(
            &db,
            &config,
            &args(&["12:00-13:00 Essay"], &["Email the TA (10 min)"]),
            now,
        )
        .await
        .unwrap();
        assert_eq!(ok.fit.planned_min, 70);
        assert_eq!(ok.fit.free_min, 750);
        assert_eq!(ok.over, None);

        let over = check_proposal(
            &db,
            &config,
            &args(&["11:00-23:30 Everything"], &["Email (10 min)"]),
            now,
        )
        .await
        .unwrap();
        assert!(over.over.is_some());

        assert!(
            check_proposal(&db, &config, &args(&[], &[]), now)
                .await
                .is_err()
        );
        let past = serde_json::json!({ "date": "2026-11-22", "untimed": ["Email (5 min)"] });
        assert!(check_proposal(&db, &config, &past, now).await.is_err());
    }

    fn proposal(
        id: &str,
        created: &str,
        decision: Option<&str>,
        reason: Option<&str>,
    ) -> PlanProposal {
        PlanProposal {
            proposal_id: id.into(),
            thread_id: "t".into(),
            date: "2026-11-23".parse().unwrap(),
            created_at: at(created),
            blocks: vec![],
            decision: decision.map(str::to_string),
            decided_reason: reason.map(str::to_string),
        }
    }

    #[test]
    fn a_plan_stands_as_its_decision_and_its_successors_say() {
        let first_start = Some(at("2026-11-23T17:00:00Z"));
        let end = at("2026-11-24T04:00:00Z");
        let early = at("2026-11-23T15:00:00Z");
        let a = proposal("a", "2026-11-23T14:00:00Z", None, None);
        let b = proposal("b", "2026-11-23T14:30:00Z", None, None);
        let all = vec![a.clone(), b.clone()];
        assert_eq!(
            status(&a, &all, first_start, end, early),
            PlanStatus::Superseded
        );
        assert_eq!(
            status(&b, &all, first_start, end, early),
            PlanStatus::Proposed
        );
        assert_eq!(
            status(&b, &all, first_start, end, at("2026-11-23T17:00:00Z")),
            PlanStatus::Lapsed
        );

        let a = proposal("a", "2026-11-23T14:00:00Z", Some("approved"), None);
        let all = vec![a.clone(), b.clone()];
        // A re-plan still waiting leaves the accepted plan in force.
        assert_eq!(
            status(&a, &all, first_start, end, early),
            PlanStatus::Accepted
        );
        let b = proposal("b", "2026-11-23T14:30:00Z", Some("approved"), None);
        let all = vec![a.clone(), b.clone()];
        assert_eq!(
            status(&a, &all, first_start, end, early),
            PlanStatus::Superseded
        );
        assert_eq!(
            status(&b, &all, first_start, end, early),
            PlanStatus::Accepted
        );

        let skipped = proposal("c", "2026-11-23T14:00:00Z", Some("rejected"), None);
        assert_eq!(
            status(&skipped, &[], first_start, end, early),
            PlanStatus::Skipped
        );
        let lapsed = proposal(
            "d",
            "2026-11-23T14:00:00Z",
            Some("rejected"),
            Some(REASON_LAPSED),
        );
        assert_eq!(
            status(&lapsed, &[], first_start, end, early),
            PlanStatus::Lapsed
        );
    }
}
