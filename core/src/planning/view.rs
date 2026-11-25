//! What the plan card and the read-only view show (rulings 110, 117). Every time is formatted
//! and every sum done here, so the client does no zone arithmetic. Rationale:
//! `docs/src/planning.md` § The card and the view.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use surrealdb::types::SurrealValue;

use super::day::{DayContext, NIGHT_ENDS_HOUR};
use super::fit::{self, Block, FreeTime};
use super::goals;
use super::plan::{self, PlanProposal, PlanStatus, UntimedItem};
use crate::calendar::day_start;
use crate::config::ResolvedConfig;
use crate::db::Database;

/// How far ahead a due date puts a task on the card.
const DUE_WITHIN_DAYS: i64 = 7;
/// Most due tasks the card lists.
const MAX_DUE: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    Block,
    Event,
    Travel,
}

/// One row of a day's timeline: a block, a calendar event, or a trip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Slot {
    pub kind: SlotKind,
    /// Local `HH:MM`.
    pub from: String,
    pub to: String,
    pub title: String,
    /// The event's place, when planning knows it.
    pub place: Option<String>,
    /// A block a re-plan moved from where the plan before it had it.
    pub moved: bool,
    #[serde(skip)]
    start: DateTime<Utc>,
    #[serde(skip)]
    end: DateTime<Utc>,
}

/// The one coloured line (117): what is planned against what is free. Never blocks Accept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FitLine {
    pub text: String,
    pub over: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DueItem {
    pub title: String,
    /// "today", "tomorrow", a weekday within the week, else a date.
    pub when: String,
    pub overdue: bool,
}

/// One day's facts, shared by the card and the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DayHead {
    pub date: NaiveDate,
    /// "Thursday".
    pub weekday: String,
    /// "Thu 19 Nov".
    pub label: String,
    /// Where the window comes from: "From 11:00 (up 09:40, morning routine 1 h 20) to 22:30 …".
    pub window: Option<String>,
    /// All-day events: labels, never time (111).
    pub all_day: Vec<String>,
    /// One line per unanswered clash (116) and per all-day event that takes the day (111).
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanCard {
    pub proposal_id: String,
    pub status: PlanStatus,
    /// When it was drafted, local `HH:MM`.
    pub drafted: String,
    /// While proposed: when it lapses, at its first block (98).
    pub until: Option<String>,
    /// When superseded: when the plan that replaced it was drafted.
    pub replaced_at: Option<String>,
    pub day: DayHead,
    pub slots: Vec<Slot>,
    pub untimed: Vec<UntimedItem>,
    pub fit: Option<FitLine>,
    pub coming_due: Vec<DueItem>,
    /// An input that could not be read; the card says so rather than showing less (117).
    pub unread: Vec<String>,
}

/// What is left of today (110).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TodayView {
    pub day: DayHead,
    /// Local `HH:MM`, for the now line.
    pub now: String,
    /// Blocks, events and trips not yet over.
    pub slots: Vec<Slot>,
    /// Where the now line goes: the number of slots that started before now.
    pub now_at: usize,
    pub blocks_done: usize,
    pub untimed: Vec<UntimedItem>,
    /// Only with an accepted plan: there is nothing to fit without one.
    pub fit: Option<FitLine>,
    /// Without an accepted plan, the free time left: "3 h 10 free". Muted, unlike the fit.
    pub free: Option<String>,
    /// A plan is waiting in chat.
    pub pending: bool,
    pub due: Vec<DueItem>,
}

/// One later day of the week: planned against free (110).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WeekRow {
    pub date: NaiveDate,
    /// "Fri".
    pub day: String,
    pub free_min: i64,
    /// Only when a plan for that day was accepted.
    pub planned_min: Option<i64>,
    /// "4 h / 6 h", or "6 h free".
    pub hours: String,
    pub all_day: Vec<String>,
    pub due: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanView {
    pub today: TodayView,
    /// "Rest of the week", or "Next week" on a Sunday.
    pub week_title: String,
    pub week: Vec<WeekRow>,
    /// "November goals".
    pub goals_title: String,
    /// The month's goals as set (ruling 110); empty until the monthly review sets them.
    pub goals: Vec<String>,
}

/// `90` → "1 h 30", `45` → "45 min", `120` → "2 h".
pub fn duration(min: i64) -> String {
    let min = min.max(0);
    match (min / 60, min % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m:02}"),
    }
}

fn hm(t: DateTime<Utc>, tz: Tz) -> String {
    t.with_timezone(&tz).format("%H:%M").to_string()
}

fn head(ctx: &DayContext) -> DayHead {
    let tz = ctx.shape.tz;
    let day = ctx.day();
    let window = ctx.bounds().map(|w| {
        let up = match ctx.first_activity {
            Some(t) => format!("up {}", hm(t, tz)),
            None => format!("assumed up {}", hm(ctx.shape.fallback(ctx.date), tz)),
        };
        let morning = match ctx.shape.morning_min {
            0 => up,
            m => format!("{up}, morning routine {}", duration(i64::from(m))),
        };
        let bed = ctx.shape.bedtime.format("%H:%M");
        let evening = match ctx.shape.evening_min {
            0 => format!("bed {bed}"),
            m => format!("bed {bed}, less evening routine {}", duration(i64::from(m))),
        };
        format!(
            "From {} ({morning}) to {} ({evening})",
            hm(w.start, tz),
            hm(w.end, tz)
        )
    });
    let mut notices: Vec<String> = day
        .takes_day
        .iter()
        .map(|e| format!("{} takes the whole day.", e.title))
        .collect();
    for clash in day.clashes.iter().filter(|c| c.going_to.is_none()) {
        notices.push(format!(
            "{} and {} clash at {}. Which are you going to?",
            clash.earlier.title,
            clash.later.title,
            hm(clash.later.span(tz).0, tz)
        ));
    }
    DayHead {
        date: ctx.date,
        weekday: ctx.date.format("%A").to_string(),
        label: ctx.date.format("%a %-d %b").to_string(),
        window,
        all_day: day.labels.iter().map(|e| e.title.clone()).collect(),
        notices,
    }
}

/// Blocks, the events he is going to, and travel, in time order. `moved[i]` marks block `i`.
fn timeline(
    ctx: &DayContext,
    free: Option<&FreeTime>,
    blocks: &[Block],
    moved: &[bool],
) -> Vec<Slot> {
    let tz = ctx.shape.tz;
    let day = ctx.day();
    let (events, _) = fit::attended(&day);
    let slot = |kind, start, end, title: &str, place, moved| Slot {
        kind,
        from: hm(start, tz),
        to: hm(end, tz),
        title: title.to_string(),
        place,
        moved,
        start,
        end,
    };
    let mut out: Vec<Slot> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let m = moved.get(i).copied().unwrap_or(false);
            slot(SlotKind::Block, b.start, b.end, &b.title, None, m)
        })
        .collect();
    for e in events {
        let (start, end) = e.span(tz);
        let place = ctx.places.place_of(e, &ctx.answers);
        out.push(slot(SlotKind::Event, start, end, &e.title, place, false));
    }
    for leg in free.map(|f| f.travel.as_slice()).unwrap_or_default() {
        let title = format!("Travel to {}", leg.to);
        out.push(slot(
            SlotKind::Travel,
            leg.start,
            leg.end,
            &title,
            None,
            false,
        ));
    }
    out.sort_by_key(|s| (s.start, s.end));
    out
}

/// Which of `blocks` sit somewhere else than a same-titled block in `before` (117).
fn moved_flags(blocks: &[Block], before: &[Block]) -> Vec<bool> {
    blocks
        .iter()
        .map(|b| {
            before.iter().any(|o| {
                o.title.eq_ignore_ascii_case(&b.title) && (o.start, o.end) != (b.start, b.end)
            })
        })
        .collect()
}

/// The card says "About 5 h 50 of work in 5 h 15 free."; the view, "5 h 50 in 5 h 15 free".
fn fit_line(planned_min: i64, free_min: i64, on_card: bool) -> FitLine {
    let (planned, free) = (duration(planned_min), duration(free_min));
    FitLine {
        text: if on_card {
            format!("About {planned} of work in {free} free.")
        } else {
            format!("{planned} in {free} free")
        },
        over: planned_min > free_min,
    }
}

fn untimed_of(p: &PlanProposal) -> Vec<UntimedItem> {
    p.untimed
        .iter()
        .filter_map(|t| plan::parse_untimed(t).ok())
        .collect()
}

#[derive(SurrealValue)]
struct DueRow {
    title: String,
    due_date: String,
}

/// Open tasks due on or before `until`, overdue ones included, earliest first.
async fn due_until(db: &Database, until: NaiveDate) -> Result<Vec<(String, NaiveDate)>, String> {
    let mut resp = db
        .query(
            "SELECT title, due_date FROM tasks
             WHERE status = 'todo' AND due_date IS NOT NONE AND due_date <= $until
             ORDER BY due_date ASC",
        )
        .bind(("until", until.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<DueRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|r| Some((r.title, r.due_date.get(..10)?.parse().ok()?)))
        .collect())
}

fn due_item(title: String, due: NaiveDate, from: NaiveDate) -> DueItem {
    let when = match (due - from).num_days() {
        d if d < 0 => format!("was due {}", due.format("%a %-d %b")),
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        2..=6 => due.format("%a").to_string(),
        _ => due.format("%a %-d %b").to_string(),
    };
    DueItem {
        title,
        when,
        overdue: due < from,
    }
}

/// The card for one `plan.set` proposal; `None` when the id is not a plan.
pub async fn card(
    db: &Database,
    config: &ResolvedConfig,
    proposal_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<PlanCard>, String> {
    let Some(p) = plan::by_id(db, proposal_id).await? else {
        return Ok(None);
    };
    let same_day = plan::proposals_for(db, p.date).await?;
    let ctx = DayContext::load(db, config, p.date).await?;
    let tz = ctx.shape.tz;
    let bounds = ctx.bounds();
    let blocks = plan::resolve_blocks(p.date, tz, bounds, &p.blocks)?;
    let first = blocks.iter().map(|b| b.start).min();
    let day_end = bounds.map_or_else(|| day_start(p.date + Duration::days(1), tz), |w| w.end);
    let status = plan::status(&p, &same_day, first, day_end, now);

    let previous = same_day
        .iter()
        .filter(|o| p.is_newer_than(o))
        .max_by_key(|o| (o.created_at, o.proposal_id.clone()));
    let before = previous
        .and_then(|o| plan::resolve_blocks(p.date, tz, bounds, &o.blocks).ok())
        .unwrap_or_default();
    let replaced_at = same_day
        .iter()
        .filter(|o| o.is_newer_than(&p))
        .map(|o| o.created_at)
        .min()
        .map(|t| hm(t, tz));

    // Against the day as it stood when drafted, as `propose` checked it.
    let free = ctx.free_time(p.created_at);
    let untimed = untimed_of(&p);
    let untimed_min: u32 = untimed.iter().map(|u| u.minutes).sum();
    let fit = free.as_ref().map(|f| {
        let fit = fit::fit(&blocks, untimed_min, f);
        fit_line(fit.planned_min, fit.free_min, true)
    });
    let mut slots = timeline(&ctx, free.as_ref(), &blocks, &moved_flags(&blocks, &before));
    if let Some(f) = &free {
        slots.retain(|s| s.end > f.window.start && s.start < f.window.end);
    }

    let mut unread = Vec::new();
    let coming_due = match due_until(db, p.date + Duration::days(DUE_WITHIN_DAYS)).await {
        Ok(rows) => rows
            .into_iter()
            .take(MAX_DUE)
            .map(|(t, d)| due_item(t, d, p.date))
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "could not read due tasks for the plan card");
            unread.push("Tasks: couldn't read what is due.".to_string());
            Vec::new()
        }
    };

    Ok(Some(PlanCard {
        proposal_id: p.proposal_id.clone(),
        status,
        drafted: hm(p.created_at, tz),
        until: (status == PlanStatus::Proposed)
            .then(|| first.map(|t| hm(t, tz)))
            .flatten(),
        replaced_at: (status == PlanStatus::Superseded)
            .then_some(replaced_at)
            .flatten(),
        day: head(&ctx),
        slots,
        untimed,
        fit,
        coming_due,
        unread,
    }))
}

/// The planning day `now` falls in: before 04:00 it is still the night before.
fn planning_today(now: DateTime<Utc>, tz: Tz) -> NaiveDate {
    let local = now.with_timezone(&tz);
    if i64::from(local.hour()) < NIGHT_ENDS_HOUR {
        local.date_naive() - Duration::days(1)
    } else {
        local.date_naive()
    }
}

/// The days after `today` up to Sunday; on a Sunday, the week after.
fn rest_of_week(today: NaiveDate) -> (&'static str, Vec<NaiveDate>) {
    let left = 6 - i64::from(today.weekday().num_days_from_monday());
    if left == 0 {
        (
            "Next week",
            (1..=7).map(|d| today + Duration::days(d)).collect(),
        )
    } else {
        (
            "Rest of the week",
            (1..=left).map(|d| today + Duration::days(d)).collect(),
        )
    }
}

/// The read-only view: what is left of today, then the rest of the week (110). The month's
/// goals join it when the monthly review produces them.
pub async fn view(
    db: &Database,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> Result<PlanView, String> {
    let tz = config.user_zone();
    let today = planning_today(now, tz);
    let ctx = DayContext::load(db, config, today).await?;
    let accepted = plan::accepted(db, today).await?;
    let pending = plan::pending(db).await?.iter().any(|p| p.date == today);

    let blocks = accepted
        .as_ref()
        .map(|a| a.blocks.clone())
        .unwrap_or_default();
    let untimed = accepted
        .as_ref()
        .map(|a| a.untimed.clone())
        .unwrap_or_default();
    let free_now = ctx.free_time(now);
    let fit = match (&accepted, ctx.bounds()) {
        (Some(_), Some(w)) => ctx.free_time(w.start).map(|whole| {
            let untimed_min: u32 = untimed.iter().map(|u| u.minutes).sum();
            let f = fit::fit(&blocks, untimed_min, &whole);
            fit_line(f.planned_min, f.free_min, false)
        }),
        _ => None,
    };
    let free = match accepted {
        Some(_) => None,
        None => free_now
            .as_ref()
            .map(|f| format!("{} free", duration(f.free_min()))),
    };
    let mut slots = timeline(&ctx, free_now.as_ref(), &blocks, &[]);
    slots.retain(|s| s.end > now);
    let now_at = slots.iter().take_while(|s| s.start <= now).count();
    let blocks_done = blocks.iter().filter(|b| b.end <= now).count();

    let (week_title, days) = rest_of_week(today);
    let last = days.last().copied().unwrap_or(today);
    let due_rows = match due_until(db, last).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "could not read due tasks for the plan view");
            Vec::new()
        }
    };
    let due = due_rows
        .iter()
        .filter(|(_, d)| *d <= today)
        .map(|(t, d)| due_item(t.clone(), *d, today))
        .collect();

    let mut week = Vec::with_capacity(days.len());
    for date in days {
        let day_ctx = DayContext {
            date,
            first_activity: None,
            ..ctx.clone()
        };
        let free_min = day_ctx.free_time(now).map_or(0, |f| f.free_min());
        let planned_min = plan::accepted(db, date).await?.map(|a| {
            let blocked: i64 = a
                .blocks
                .iter()
                .map(|b| (b.end - b.start).num_minutes().max(0))
                .sum();
            blocked + a.untimed.iter().map(|u| i64::from(u.minutes)).sum::<i64>()
        });
        let hours = match planned_min {
            Some(p) => format!("{} / {}", duration(p), duration(free_min)),
            None => format!("{} free", duration(free_min)),
        };
        week.push(WeekRow {
            date,
            day: date.format("%a").to_string(),
            free_min,
            planned_min,
            hours,
            all_day: day_ctx
                .day()
                .labels
                .iter()
                .map(|e| e.title.clone())
                .collect(),
            due: due_rows
                .iter()
                .filter(|(_, d)| *d == date)
                .map(|(t, _)| t.clone())
                .collect(),
        });
    }

    Ok(PlanView {
        today: TodayView {
            day: head(&ctx),
            now: hm(now, tz),
            slots,
            now_at,
            blocks_done,
            untimed,
            fit,
            free,
            pending,
            due,
        },
        week_title: week_title.to_string(),
        week,
        goals_title: format!("{} goals", today.format("%B")),
        goals: goals::for_month(db, &goals::month_of(today))
            .await?
            .map(|g| g.goals)
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::NaiveTime;

    use super::super::day::Shape;
    use super::super::places::{Place, Places, pair};
    use super::*;
    use crate::calendar::day::Answers;
    use crate::calendar::test_support::{all_day, timed};

    const TZ: Tz = chrono_tz::America::Toronto;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn ctx(events: Vec<crate::calendar::CalendarEvent>) -> DayContext {
        let mut travel = HashMap::new();
        travel.insert(pair("Home", "Campus"), 45);
        DayContext {
            date: "2026-11-19".parse().unwrap(),
            shape: Shape {
                tz: TZ,
                bedtime: NaiveTime::from_hms_opt(23, 30, 0).unwrap(),
                morning_min: 80,
                evening_min: 60,
                fallback_hour: 9,
            },
            // 09:40 EST.
            first_activity: Some(at("2026-11-19T14:40:00Z")),
            events,
            answers: Answers::default(),
            places: Places {
                places: vec![
                    Place {
                        name: "Home".into(),
                        matches: vec![],
                        home: true,
                    },
                    Place {
                        name: "Campus".into(),
                        matches: vec!["B2".into()],
                        home: false,
                    },
                ],
                travel,
            },
        }
    }

    fn block(start: &str, end: &str, title: &str) -> Block {
        Block {
            start: at(start),
            end: at(end),
            title: title.into(),
        }
    }

    #[test]
    fn durations_read_as_hours_and_minutes() {
        assert_eq!(duration(45), "45 min");
        assert_eq!(duration(120), "2 h");
        assert_eq!(duration(350), "5 h 50");
        assert_eq!(duration(65), "1 h 05");
        assert_eq!(duration(-5), "0 min");
    }

    #[test]
    fn the_head_says_where_the_window_comes_from() {
        let e = all_day("b", "Mum's birthday", "2026-11-19", "2026-11-20");
        let h = head(&ctx(vec![e]));
        assert_eq!(h.weekday, "Thursday");
        assert_eq!(h.label, "Thu 19 Nov");
        assert_eq!(
            h.window.as_deref(),
            Some(
                "From 11:00 (up 09:40, morning routine 1 h 20) to 22:30 \
                 (bed 23:30, less evening routine 1 h)"
            )
        );
        assert_eq!(h.all_day, vec!["Mum's birthday".to_string()]);
    }

    #[test]
    fn the_timeline_interleaves_blocks_events_and_travel() {
        // Lab meeting 13:00–14:00 EST on campus, with a 45-minute trip before it.
        let mut lab = timed(
            "lab",
            "Lab meeting",
            "2026-11-19T18:00:00Z",
            "2026-11-19T19:00:00Z",
        );
        lab.location = Some("B2 3342".into());
        let c = ctx(vec![lab]);
        let free = c.free_time(at("2026-11-19T14:40:00Z"));
        let blocks = vec![block(
            "2026-11-19T16:00:00Z",
            "2026-11-19T17:10:00Z",
            "Thesis",
        )];
        let slots = timeline(&c, free.as_ref(), &blocks, &[true]);
        let rows: Vec<(SlotKind, &str, &str)> = slots
            .iter()
            .map(|s| (s.kind, s.from.as_str(), s.title.as_str()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (SlotKind::Block, "11:00", "Thesis"),
                (SlotKind::Travel, "12:15", "Travel to Campus"),
                (SlotKind::Event, "13:00", "Lab meeting"),
                (SlotKind::Travel, "21:45", "Travel to Home"),
            ]
        );
        assert!(slots[0].moved);
        assert_eq!(slots[2].place.as_deref(), Some("Campus"));
    }

    #[test]
    fn a_block_is_moved_only_when_its_title_sat_elsewhere_before() {
        let before = vec![
            block("2026-11-19T16:00:00Z", "2026-11-19T17:00:00Z", "Thesis"),
            block("2026-11-19T19:00:00Z", "2026-11-19T20:00:00Z", "Reading"),
        ];
        let now = vec![
            block("2026-11-19T19:00:00Z", "2026-11-19T20:00:00Z", "thesis"),
            block("2026-11-19T19:00:00Z", "2026-11-19T20:00:00Z", "Reading"),
            block("2026-11-19T21:00:00Z", "2026-11-19T22:00:00Z", "New"),
        ];
        assert_eq!(moved_flags(&now, &before), vec![true, false, false]);
    }

    #[test]
    fn due_dates_read_relative_to_the_day() {
        let day: NaiveDate = "2026-11-19".parse().unwrap();
        let when = |d: &str| due_item("x".into(), d.parse().unwrap(), day);
        assert_eq!(when("2026-11-19").when, "today");
        assert_eq!(when("2026-11-20").when, "tomorrow");
        assert_eq!(when("2026-11-23").when, "Mon");
        assert_eq!(when("2026-11-30").when, "Mon 30 Nov");
        let late = when("2026-11-17");
        assert!(late.overdue);
        assert_eq!(late.when, "was due Tue 17 Nov");
    }

    #[test]
    fn the_week_runs_to_sunday_and_a_sunday_shows_the_next() {
        let (title, days) = rest_of_week("2026-11-19".parse().unwrap());
        assert_eq!(title, "Rest of the week");
        assert_eq!(days.len(), 3);
        let (title, days) = rest_of_week("2026-11-22".parse().unwrap());
        assert_eq!(title, "Next week");
        assert_eq!(days.first().unwrap().to_string(), "2026-11-23");
        assert_eq!(days.len(), 7);
    }

    #[test]
    fn the_small_hours_belong_to_the_night_before() {
        // 01:30 EST on the 20th is still the 19th's planning day.
        assert_eq!(
            planning_today(at("2026-11-20T06:30:00Z"), TZ).to_string(),
            "2026-11-19"
        );
        assert_eq!(
            planning_today(at("2026-11-20T09:30:00Z"), TZ).to_string(),
            "2026-11-20"
        );
    }
}
