//! The morning plan ritual (ruling 98). The agent drafts it at his first activity, or at the
//! fallback hour, as a scheduled question answered by the planning specialist (112). The server
//! sends its one silent push, never in quiet hours, and closes a plan once it has lapsed or been
//! replaced. Rationale: `docs/src/planning.md` § The morning ritual.

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::day::DayContext;
use super::plan::{self, PlanStatus};
use crate::assistant::{inbox, scheduled};
use crate::config::{ConfigKey, Feature, ResolvedConfig};
use crate::db::Database;
use crate::events::{EventWriter, ProposalDecision};
use crate::jobs::alerts::quiet_until;
use crate::notify::{Notification, Notifier, Priority};

/// The scheduled prompt's name. Never rename it: its history is how a day's run is found.
pub const MORNING_PLAN: &str = "morning_plan";
pub const TITLE: &str = "Today's plan";
/// The question the agent raises. The planning brief carries the how.
pub const PROMPT: &str = "Draft my plan for today.";

/// How often the server checks for a plan to push or close.
pub const TICK: std::time::Duration = std::time::Duration::from_secs(60);

fn quiet(config: &ResolvedConfig, now: DateTime<Utc>) -> bool {
    quiet_until(
        now,
        config.user_zone(),
        config.int_of(ConfigKey::NotifyQuietStart) as u32,
        config.int_of(ConfigKey::NotifyQuietEnd) as u32,
    )
    .is_some()
}

/// The day the morning plan should be drafted for now, or `None`. Due once a day, at his first
/// activity or the fallback hour, outside quiet hours, while the day has time left, and only if
/// no plan has been proposed for it yet: asking for one first is the ritual done.
pub async fn draft_due(
    db: &Database,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> Result<Option<NaiveDate>, String> {
    if !config.enabled(Feature::Planning) || !config.enabled(Feature::Llm) || quiet(config, now) {
        return Ok(None);
    }
    let tz = config.user_zone();
    let today = now.with_timezone(&tz).date_naive();
    let last = scheduled::last_run(db, MORNING_PLAN)
        .await
        .map_err(|e| e.to_string())?;
    if last.is_some_and(|t| t.with_timezone(&tz).date_naive() >= today) {
        return Ok(None);
    }
    // Ruling 99 (RULINGS.md): five untouched mornings pause it until he answers one.
    if super::rituals::paused(db, MORNING_PLAN).await? {
        return Ok(None);
    }
    // A question he typed and is waiting on may be asking for a plan: answer it first, or the
    // ritual's draft would supersede his.
    let waiting = crate::assistant::conversation::pending_questions(db)
        .await
        .map_err(|e| e.to_string())?;
    if waiting.iter().any(|q| !q.question.scheduled) {
        return Ok(None);
    }
    if !plan::proposals_for(db, today).await?.is_empty() {
        return Ok(None);
    }
    let ctx = DayContext::load(db, config, today).await?;
    if ctx.first_activity.is_none() && now < ctx.shape.fallback(today) {
        return Ok(None);
    }
    Ok(ctx.window(now).map(|_| today))
}

/// [`draft_due`] against the current clock, so the agent takes no `chrono` dependency.
pub async fn draft_due_now(
    db: &Database,
    config: &ResolvedConfig,
) -> Result<Option<NaiveDate>, String> {
    draft_due(db, config, Utc::now()).await
}

/// The morning plan as a scheduled prompt, for the agent's question and thread housekeeping.
/// Its due-check is [`draft_due`], not the prompt's hour.
pub fn as_prompt(config: &ResolvedConfig) -> scheduled::ScheduledPrompt {
    scheduled::ScheduledPrompt {
        name: MORNING_PLAN,
        title: TITLE,
        text: PROMPT.to_string(),
        enabled: config.enabled(Feature::Planning),
        cadence: scheduled::Cadence::Daily,
        hour: 0,
        timezone: config.user_zone(),
        tier: crate::jobs::Tier::Ritual,
        keep_days: scheduled::KEEP_CHECK_INS_DAYS,
    }
}

/// What the server's tick did, for its log line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TickReport {
    pub closed: usize,
    pub pushed: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    /// The last day a morning plan was pushed. One push a day.
    last_pushed: Option<NaiveDate>,
}

/// The server half: closes plans that lapsed or were replaced, and pushes a fresh morning plan.
pub struct RitualJob {
    db: Database,
    writer: Arc<EventWriter>,
    notifier: Option<Arc<dyn Notifier>>,
}

impl RitualJob {
    pub fn new(
        db: Database,
        writer: Arc<EventWriter>,
        notifier: Option<Arc<dyn Notifier>>,
    ) -> Self {
        Self {
            db,
            writer,
            notifier,
        }
    }

    pub async fn init_schema(&self) -> Result<(), String> {
        self.db
            .query(
                "DEFINE TABLE IF NOT EXISTS job_ritual_state SCHEMAFULL;
                 DEFINE FIELD IF NOT EXISTS state ON job_ritual_state TYPE string;",
            )
            .await
            .map_err(|e| format!("init job_ritual_state: {e}"))?
            .check()
            .map_err(|e| format!("init job_ritual_state: {e}"))?;
        Ok(())
    }

    async fn load(&self) -> State {
        let read = async {
            let mut resp = self
                .db
                .query("SELECT VALUE state FROM type::record('job_ritual_state', 'last')")
                .await
                .map_err(|e| e.to_string())?;
            let rows: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
            Ok::<_, String>(rows.into_iter().next())
        };
        match read.await {
            Ok(Some(s)) => serde_json::from_str(&s).unwrap_or_default(),
            Ok(None) => State::default(),
            Err(e) => {
                tracing::warn!(error = %e, "ritual: could not read its state; treating as first run");
                State::default()
            }
        }
    }

    async fn save(&self, state: &State) -> Result<(), String> {
        let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
        self.db
            .query("UPSERT type::record('job_ritual_state', 'last') CONTENT { state: $state }")
            .bind(("state", json))
            .await
            .map_err(|e| e.to_string())?
            .check()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn tick(&self, now: DateTime<Utc>) -> Result<TickReport, String> {
        let config = crate::events::load_shared(&self.db)
            .await
            .map_err(|e| e.to_string())?;
        let mut report = TickReport::default();
        if !config.enabled(Feature::Planning) || !config.enabled(Feature::Llm) {
            return Ok(report);
        }
        let mut fresh: Vec<plan::PlanProposal> = Vec::new();
        for p in plan::pending(&self.db).await? {
            let reason = match plan::current_status(&self.db, &config, &p, now).await {
                Ok(PlanStatus::Lapsed) => plan::REASON_LAPSED,
                Ok(PlanStatus::Superseded) => plan::REASON_SUPERSEDED,
                Ok(PlanStatus::Proposed) => {
                    fresh.push(p);
                    continue;
                }
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(error = %e, proposal = %p.proposal_id, "ritual: could not place a plan");
                    continue;
                }
            };
            match inbox::decide(
                &self.db,
                &config,
                &self.writer,
                &p.proposal_id,
                ProposalDecision::Rejected,
                Some(reason.to_string()),
            )
            .await
            {
                Ok(_) => report.closed += 1,
                Err(e) => {
                    tracing::warn!(error = %e, proposal = %p.proposal_id, "ritual: could not close a plan")
                }
            }
        }
        report.pushed = self.maybe_push(&config, &fresh, now).await?;
        Ok(report)
    }

    /// One silent push for today's morning plan, outside quiet hours. A plan he asked for in
    /// chat is not pushed: he is already looking at it.
    async fn maybe_push(
        &self,
        config: &ResolvedConfig,
        fresh: &[plan::PlanProposal],
        now: DateTime<Utc>,
    ) -> Result<bool, String> {
        let Some(notifier) = &self.notifier else {
            return Ok(false);
        };
        if quiet(config, now) {
            return Ok(false);
        }
        let today = now.with_timezone(&config.user_zone()).date_naive();
        let mut state = self.load().await;
        if state.last_pushed.is_some_and(|d| d >= today) {
            return Ok(false);
        }
        let threads = morning_threads(&self.db, today, config).await?;
        let Some(p) = fresh
            .iter()
            .find(|p| p.date == today && threads.contains(&p.thread_id))
        else {
            return Ok(false);
        };
        let n = Notification {
            title: TITLE.to_string(),
            message: match p.blocks.len() {
                0 => "Your plan for today is ready.".to_string(),
                1 => "Your plan for today is ready: 1 block.".to_string(),
                n => format!("Your plan for today is ready: {n} blocks."),
            },
            priority: Priority::Low,
            tags: Vec::new(),
            click: Some(crate::notify::app_link("assistant")),
        };
        notifier.send(&n).await?;
        state.last_pushed = Some(today);
        if let Err(e) = self.save(&state).await {
            tracing::warn!(error = %e, "ritual: pushed, but the state was not saved; it may repeat");
        }
        Ok(true)
    }

    pub fn spawn(self: Arc<Self>) {
        tokio::spawn(async move {
            if let Err(e) = self.init_schema().await {
                tracing::error!(error = %e, "ritual: no state table; job off");
                return;
            }
            let mut tick = tokio::time::interval(TICK);
            loop {
                tick.tick().await;
                match self.tick(Utc::now()).await {
                    Ok(r) if r == TickReport::default() => {}
                    Ok(r) => tracing::info!(closed = r.closed, pushed = r.pushed, "ritual: tick"),
                    Err(e) => tracing::warn!(error = %e, "ritual: tick failed"),
                }
            }
        });
    }
}

/// Threads the morning plan prompt raised on `today`.
async fn morning_threads(
    db: &Database,
    today: NaiveDate,
    config: &ResolvedConfig,
) -> Result<Vec<String>, String> {
    let from = crate::calendar::day_start(today, config.user_zone());
    let mut resp = db
        .query(
            "SELECT VALUE thread_id FROM assistant_messages
             WHERE prompt_name = $prompt AND created_at >= type::datetime($from)",
        )
        .bind(("prompt", MORNING_PLAN.to_string()))
        .bind(("from", from.to_rfc3339()))
        .await
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    let rows: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows)
}
