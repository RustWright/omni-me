//! One day's inputs, gathered for [`super::fit`]: the shape he set in config, his routine
//! lengths, his first activity, the calendar, his answers and his places (rulings 98, 114).
//! Rationale: `docs/src/planning.md` § The day's inputs.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

use super::fit::{self, Block, DayWindow, FreeTime, Refusal};
use super::places::{self, Places};
use crate::calendar::day::{Answers, Day, read_day};
use crate::calendar::{CalendarEvent, day_start, sync};
use crate::config::{ConfigKey, ResolvedConfig, clock_time};
use crate::db::Database;

/// The day's shape as he set it.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub tz: Tz,
    pub bedtime: NaiveTime,
    pub morning_min: u32,
    pub evening_min: u32,
    /// The hour the day is taken to start when nothing he did says otherwise.
    pub fallback_hour: u32,
}

impl Shape {
    pub async fn load(db: &Database, config: &ResolvedConfig) -> Self {
        let bedtime = clock_time(&config.text_of(ConfigKey::PlanningBedtime))
            .or_else(|| NaiveTime::from_hms_opt(23, 30, 0))
            .unwrap_or(NaiveTime::MIN);
        Shape {
            tz: config.user_zone(),
            bedtime,
            morning_min: routine_minutes(db, &config.text_of(ConfigKey::PlanningMorningRoutine))
                .await,
            evening_min: routine_minutes(db, &config.text_of(ConfigKey::PlanningEveningRoutine))
                .await,
            fallback_hour: config.int_of(ConfigKey::PlanningFallbackHour) as u32,
        }
    }

    /// The fallback hour on `day`, as an instant.
    pub fn fallback(&self, day: NaiveDate) -> DateTime<Utc> {
        let at =
            day.and_time(NaiveTime::from_hms_opt(self.fallback_hour, 0, 0).unwrap_or_default());
        self.tz
            .from_local_datetime(&at)
            .earliest()
            .map_or_else(|| day_start(day, self.tz), |t| t.with_timezone(&Utc))
    }
}

/// The summed item estimates of the routine group named `name`, matched without case. Empty,
/// unknown or unreadable is zero, so a typo in settings plans a longer day rather than none.
async fn routine_minutes(db: &Database, name: &str) -> u32 {
    let name = name.trim();
    if name.is_empty() {
        return 0;
    }
    let groups = match crate::db::queries::list_routine_groups(db).await {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!(error = %e, "could not read routine groups; no routine minutes");
            return 0;
        }
    };
    let Some(group) = groups
        .iter()
        .find(|g| g.name.trim().eq_ignore_ascii_case(name))
    else {
        tracing::warn!(
            routine = name,
            "no routine by that name; no routine minutes"
        );
        return 0;
    };
    match crate::db::queries::list_routine_items(db, &group.id).await {
        Ok(items) => items
            .iter()
            .map(|i| u32::try_from(i.estimated_duration_min).unwrap_or(0))
            .sum(),
        Err(e) => {
            tracing::warn!(error = %e, "could not read routine items; no routine minutes");
            0
        }
    }
}

/// The event types that count as him starting the day (ruling 114). A question counts only
/// when he typed it: a scheduled one is the assistant's.
const ACTIVITY_KINDS: &[&str] = &["journal_entry_created", "routine_item_completed"];

/// Activity before this local hour belongs to the night before: a 01:00 journal line is a late
/// night, not an early start.
pub(crate) const NIGHT_ENDS_HOUR: i64 = 4;

/// His first activity on local `day`, read from the log, which every host holds.
pub async fn first_activity(
    db: &Database,
    day: NaiveDate,
    tz: Tz,
) -> Result<Option<DateTime<Utc>>, String> {
    let mut resp = db
        .query(
            "SELECT <string> timestamp AS timestamp FROM events
             WHERE timestamp >= type::datetime($from) AND timestamp < type::datetime($to)
               AND (event_type IN $kinds
                    OR (event_type = 'assistant_question_asked' AND payload.scheduled != true))
             ORDER BY timestamp ASC LIMIT 1",
        )
        .bind((
            "from",
            (day_start(day, tz) + Duration::hours(NIGHT_ENDS_HOUR)).to_rfc3339(),
        ))
        .bind((
            "to",
            (day_start(day + Duration::days(1), tz) + Duration::hours(NIGHT_ENDS_HOUR))
                .to_rfc3339(),
        ))
        .bind((
            "kinds",
            ACTIVITY_KINDS
                .iter()
                .map(|k| k.to_string())
                .collect::<Vec<_>>(),
        ))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let rows: Vec<String> = resp.take("timestamp").unwrap_or_default();
    Ok(rows
        .first()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc)))
}

/// Everything a plan for one date is checked against.
#[derive(Debug, Clone)]
pub struct DayContext {
    pub date: NaiveDate,
    pub shape: Shape,
    pub first_activity: Option<DateTime<Utc>>,
    pub events: Vec<CalendarEvent>,
    pub answers: Answers,
    pub places: Places,
}

impl DayContext {
    pub async fn load(
        db: &Database,
        config: &ResolvedConfig,
        date: NaiveDate,
    ) -> Result<Self, String> {
        let shape = Shape::load(db, config).await;
        Ok(DayContext {
            first_activity: first_activity(db, date, shape.tz).await?,
            events: sync::recorded(db, None).await?,
            answers: sync::answers(db).await?,
            places: places::load(db).await?,
            date,
            shape,
        })
    }

    pub fn day(&self) -> Day<'_> {
        read_day(&self.events, &self.answers, self.date, self.shape.tz)
    }

    /// When the day opened: his first activity, else the fallback hour. A plan drafted before
    /// either has happened assumes the fallback.
    pub fn opened(&self) -> DateTime<Utc> {
        self.first_activity
            .unwrap_or_else(|| self.shape.fallback(self.date))
    }

    /// The whole day's window from [`fit::day_window`], wherever now is.
    pub fn bounds(&self) -> Option<DayWindow> {
        fit::day_window(
            self.opened(),
            self.shape.morning_min,
            self.shape.bedtime,
            self.shape.evening_min,
            self.shape.tz,
        )
    }

    /// [`DayContext::bounds`], with its start moved up to `now` once the day is under way: time
    /// already gone is not free.
    pub fn window(&self, now: DateTime<Utc>) -> Option<DayWindow> {
        let mut w = self.bounds()?;
        if now > w.start {
            w.start = now;
        }
        (w.end > w.start).then_some(w)
    }

    pub fn free_time(&self, now: DateTime<Utc>) -> Option<FreeTime> {
        let window = self.window(now)?;
        Some(fit::free_time(
            &self.day(),
            window,
            &self.places,
            &self.answers,
            0,
            self.shape.tz,
        ))
    }

    /// Every refusal for `blocks` against this day (ruling 112). An ended day refuses them all
    /// as outside the window.
    pub fn validate(&self, blocks: &[Block], now: DateTime<Utc>) -> Vec<(usize, Refusal)> {
        match self.free_time(now) {
            Some(free) => fit::validate(blocks, &self.day(), &free, now, self.shape.tz),
            None => (0..blocks.len())
                .map(|i| (i, Refusal::OutsideWindow))
                .collect(),
        }
    }

    /// The day as the planning brief states it: plain lines of local times, every sum done here
    /// so the model never adds minutes itself.
    pub fn describe(&self, now: DateTime<Utc>) -> String {
        let tz = self.shape.tz;
        let hm = |t: DateTime<Utc>| t.with_timezone(&tz).format("%H:%M").to_string();
        let mut out = format!("\n\n### {}", self.date.format("%A %Y-%m-%d"));
        let Some(free) = self.free_time(now) else {
            out.push_str("\nNothing is left of this day to plan.");
            return out;
        };
        let opened = match self.first_activity {
            Some(t) => format!("up at {}", hm(t)),
            None => format!("assumed up at {}", hm(self.shape.fallback(self.date))),
        };
        out.push_str(&format!(
            "\nWindow: {}-{} ({opened}, morning routine {} min; bedtime {} less evening \
             routine {} min).",
            hm(free.window.start),
            hm(free.window.end),
            self.shape.morning_min,
            self.shape.bedtime.format("%H:%M"),
            self.shape.evening_min,
        ));
        let day = self.day();
        if !day.takes_day.is_empty() {
            let titles: Vec<&str> = day.takes_day.iter().map(|e| e.title.as_str()).collect();
            out.push_str(&format!(
                "\nThe whole day is taken by {}. Plan nothing timed.",
                titles.join(", ")
            ));
        }
        if !day.labels.is_empty() {
            let titles: Vec<&str> = day.labels.iter().map(|e| e.title.as_str()).collect();
            out.push_str(&format!(
                "\nAll-day labels (take no time): {}.",
                titles.join(", ")
            ));
        }
        if !day.timed.is_empty() {
            out.push_str("\nCalendar events (fixed; a block may never overlap one):");
            for e in &day.timed {
                let (s, end) = e.span(tz);
                let place = self
                    .places
                    .place_of(e, &self.answers)
                    .map(|p| format!(" ({p})"))
                    .unwrap_or_default();
                out.push_str(&format!("\n- {}-{} {}{place}", hm(s), hm(end), e.title));
            }
        }
        for clash in &day.clashes {
            if clash.going_to.is_none() {
                out.push_str(&format!(
                    "\nClash: \"{}\" and \"{}\" overlap and he has not said which he is going \
                     to. Ask him in one line; travel assumes the earlier.",
                    clash.earlier.title, clash.later.title
                ));
            }
        }
        if !free.travel.is_empty() {
            out.push_str("\nTravel (busy):");
            for leg in &free.travel {
                out.push_str(&format!(
                    "\n- {}-{} {} to {}",
                    hm(leg.start),
                    hm(leg.end),
                    leg.from,
                    leg.to
                ));
            }
        }
        if free.free.is_empty() {
            out.push_str("\nNo free time is left in the window.");
        } else {
            out.push_str("\nFree time:");
            for (s, e) in &free.free {
                out.push_str(&format!(
                    "\n- {}-{} ({} min)",
                    hm(*s),
                    hm(*e),
                    (*e - *s).num_minutes()
                ));
            }
            out.push_str(&format!("\nFree in all: {} min.", free.free_min()));
        }
        let asks = day.asks.len();
        if asks > 0 {
            out.push_str(&format!(
                "\n{asks} calendar question(s) are still open; travel and the day's shape \
                 assume the cautious answer until he gives one."
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::test_support::timed;

    const TZ: Tz = chrono_tz::America::Toronto;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn ctx(first: Option<&str>) -> DayContext {
        DayContext {
            date: "2026-11-23".parse().unwrap(),
            shape: Shape {
                tz: TZ,
                bedtime: NaiveTime::from_hms_opt(23, 30, 0).unwrap(),
                morning_min: 60,
                evening_min: 30,
                fallback_hour: 11,
            },
            first_activity: first.map(at),
            events: vec![timed(
                "l",
                "Lecture",
                "2026-11-23T19:00:00Z",
                "2026-11-23T20:00:00Z",
            )],
            answers: Answers::default(),
            places: Places::default(),
        }
    }

    #[test]
    fn the_day_opens_at_his_first_activity_else_the_fallback() {
        // Up at 09:00 EST: the window opens an hour later and ends at 23:00.
        let c = ctx(Some("2026-11-23T14:00:00Z"));
        let w = c.window(at("2026-11-23T13:00:00Z")).unwrap();
        assert_eq!(w.start, at("2026-11-23T15:00:00Z"));
        assert_eq!(w.end, at("2026-11-24T04:00:00Z"));
        // Nothing yet: 11:00 plus the morning.
        let c = ctx(None);
        let w = c.window(at("2026-11-23T13:00:00Z")).unwrap();
        assert_eq!(w.start, at("2026-11-23T17:00:00Z"));
    }

    #[test]
    fn time_already_gone_is_not_free() {
        let c = ctx(Some("2026-11-23T14:00:00Z"));
        let w = c.window(at("2026-11-23T21:00:00Z")).unwrap();
        assert_eq!(w.start, at("2026-11-23T21:00:00Z"));
        assert!(c.window(at("2026-11-24T05:00:00Z")).is_none());
    }

    #[test]
    fn the_brief_states_events_and_free_minutes_in_local_time() {
        let c = ctx(Some("2026-11-23T14:00:00Z"));
        let text = c.describe(at("2026-11-23T13:00:00Z"));
        assert!(text.contains("Window: 10:00-23:00"), "{text}");
        assert!(text.contains("- 14:00-15:00 Lecture"), "{text}");
        // 10:00-14:00 and 15:00-23:00.
        assert!(text.contains("Free in all: 720 min."), "{text}");
    }

    #[test]
    fn an_ended_day_refuses_every_block() {
        let c = ctx(Some("2026-11-23T14:00:00Z"));
        let block = Block {
            start: at("2026-11-24T05:10:00Z"),
            end: at("2026-11-24T05:40:00Z"),
            title: "Late".into(),
        };
        let refusals = c.validate(&[block], at("2026-11-24T05:00:00Z"));
        assert_eq!(refusals, vec![(0, Refusal::OutsideWindow)]);
    }
}
