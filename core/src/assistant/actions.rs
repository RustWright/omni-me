//! The action catalog: what the assistant is allowed to propose.
//!
//! The write-side twin of [`super::catalog`], and deliberately a *separate*
//! table. Being readable and being writable are different permissions over the
//! same collection — the journal is the proof: it is fully readable and
//! permanently unwritable — so one table carrying both would have to encode the
//! difference as a flag, and a flag is something a later entry can get wrong by
//! omission.
//!
//! ## Code rather than events, same as the read catalog
//!
//! Every event an action can author is constructed in this build. An
//! event-declared action naming an event type this binary cannot build would be
//! genericity in name only, so the declaration lives where the constructor does.
//!
//! ## ⛔ The journal is never proposable
//!
//! The user is its sole author. That is a product invariant, not a phase
//! boundary: it does not lapse when autonomy is granted per action type, and it
//! does not lapse because the machinery would support it. [`NEVER_PROPOSABLE`]
//! and the test over it are what make the decision visible — an absent entry is
//! not a decision anyone can review.
//!
//! Reading the journal stays fully available through [`super::catalog`]. What is
//! forbidden is only the reverse direction.

use serde_json::{Value, json};

use crate::config::{Feature, ResolvedConfig};
use crate::events::{EventType, NewEvent};

/// How much an action may do without being asked about.
///
/// One variant, and that is the point: every route to a write in this build ends
/// at a human. Promotion — approval-rate evidence turning into a standing grant —
/// adds the second variant and the grant events behind it, and
/// [`every_action_always_asks`] is what stops autonomy arriving as a side effect
/// of someone adding an action type in the meantime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autonomy {
    /// Propose, and change nothing until a person accepts.
    AlwaysAsk,
}

/// Which action this is, for the one place that branches on it.
///
/// Per-kind branching exists only in [`build_events`], mirroring how the
/// auto-import sources converge on a single `build_one`. Everything else about an
/// action is data on [`ActionType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    NoteCreate,
    BeliefRecord,
    BeliefSupersede,
    RoutineCreate,
    RoutineAddItem,
    RoutineModifyItem,
}

/// Where an argument's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamSource {
    /// The model supplies it in the `propose` call.
    Model,
    /// **The system fills it from the run's trace** — the records the assistant
    /// actually opened while reaching this conclusion.
    ///
    /// ⚠️ Deliberately not something the model can write. Letting it list its own
    /// evidence reopens exactly the hallucination surface `records_read` closes:
    /// a citation is only worth anything if it names a record that was
    /// demonstrably in hand. The model is never shown this parameter and cannot
    /// pass it.
    EvidenceFromTrace,
}

/// A domain parser an argument must satisfy, returning its own error text.
///
/// Named because it appears on the field, on the builder and in every entry that
/// uses one — and because the signature is the contract: **take the raw string,
/// return the domain's message**, never a rewritten one.
pub type ParamValidator = fn(&str) -> Result<(), String>;

/// One argument an action takes.
///
/// Declared as data rather than as a raw JSON schema because two readers need it
/// in different shapes: `describe_type` renders it as prose for the model, and
/// [`validate_args`] checks a proposal against it. A hand-written schema plus a
/// hand-written validator is two places to disagree.
#[derive(Debug, Clone, Copy)]
pub struct ActionParam {
    pub key: &'static str,
    pub required: bool,
    pub description: &'static str,
    /// Hard ceiling on a string argument. Not politeness: an argument is model
    /// output that may carry text lifted out of a record, it is stored in an
    /// event, and every event syncs to the phone.
    pub max_chars: usize,
    /// When set, the value must be one of these exactly.
    ///
    /// Used where a free string would invite false precision — a model asked for
    /// a numeric confidence will happily produce `0.87` with no calibration
    /// behind it, while a three-way choice is both what it can actually
    /// distinguish and what a person can act on.
    pub allowed: Option<&'static [&'static str]>,
    /// A domain parser the value must satisfy, returning its own error message.
    ///
    /// ⚠️ Reached for **instead of** [`ActionParam::allowed`] wherever the domain
    /// already owns a parser. A routine's frequency is the case that forced it:
    /// `daily`/`weekly`/`biweekly`/`monthly` would fit a list, but `custom:N`
    /// carries an integer with bounds, and re-listing what is legal here would
    /// be a second definition of the same rule — free to drift, and drifting
    /// silently, since an unparseable frequency reaches the projection rather
    /// than the user.
    pub validator: Option<ParamValidator>,
    pub source: ParamSource,
}

impl ActionParam {
    /// A plain text argument the model supplies.
    pub const fn text(key: &'static str, description: &'static str, max_chars: usize) -> Self {
        ActionParam {
            key,
            required: true,
            description,
            max_chars,
            allowed: None,
            validator: None,
            source: ParamSource::Model,
        }
    }

    pub const fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub const fn one_of(mut self, allowed: &'static [&'static str]) -> Self {
        self.allowed = Some(allowed);
        self
    }

    /// Validate with the domain's own parser. See [`ActionParam::validator`].
    pub const fn checked_by(mut self, validator: ParamValidator) -> Self {
        self.validator = Some(validator);
        self
    }

    /// The records this run opened, filled in by the system. See
    /// [`ParamSource::EvidenceFromTrace`].
    pub const fn evidence(key: &'static str, description: &'static str) -> Self {
        ActionParam {
            key,
            required: false,
            description,
            max_chars: 0,
            allowed: None,
            validator: None,
            source: ParamSource::EvidenceFromTrace,
        }
    }

    pub fn is_model_supplied(&self) -> bool {
        self.source == ParamSource::Model
    }
}

/// One thing the assistant may offer to do.
#[derive(Debug, Clone, Copy)]
pub struct ActionType {
    /// What the model calls it. `{type}.{operation}`, so the list stays readable
    /// as it grows and an action's target is obvious without a lookup.
    pub name: &'static str,
    pub kind: ActionKind,
    /// Off means the action is absent from the proposable list entirely, rather
    /// than present and refusing — a disabled feature should not look like a
    /// broken one. Same rule as [`super::catalog::CatalogEntry::feature`].
    pub feature: Feature,
    /// What it does, as the model reads it.
    pub description: &'static str,
    pub params: &'static [ActionParam],
    /// The event types approving this action authors.
    ///
    /// Declared as well as built so the ⛔ journal guard can be checked against a
    /// list rather than by running every action, and
    /// [`declared_events_match_what_is_built`] then holds the declaration to what
    /// [`build_events`] actually does. Either check alone is weak: the first
    /// trusts a comment, the second needs a fixture per action.
    pub produces: &'static [EventType],
    /// Whether the effect can be undone from the log alone. Declared from the day
    /// the action is defined, per `docs/src/assistant.md` § "Reversibility,
    /// precisely" — the predicate is the destination of the whole permission
    /// model, so it cannot be retrofitted onto a hundred action types later.
    pub reversible: bool,
    /// Why it got that answer, in one sentence. Recorded because the three rules
    /// are easy to agree with and hard to apply, and a future entry copying a
    /// neighbour's `true` without re-running them is the failure this prevents.
    pub reversibility: &'static str,
    pub autonomy: Autonomy,
    /// When the assistant may raise this at all.
    pub trigger: Trigger,
}

/// What makes an action eligible to be proposed.
///
/// ⚠️ **[`Trigger::OnRequest`] is prompt-enforced, not code-enforced**, and saying
/// so matters: there is no reliable way to classify "the user asked for a
/// conclusion", and pretending otherwise would put a guarantee behind a
/// heuristic. What is structural is the approval gate — an over-eager proposal
/// still records nothing until a person accepts it, so the trigger governs how
/// many cards appear, never what enters the log.
///
/// One variant today by the user's decision (2026-09-10): beliefs are proposed
/// only when asked for. The two wider policies considered — propose whenever a
/// durable pattern is noticed, and propose only on repeated independent evidence
/// — are **deferred to during or after Phase F, not dropped**, which is why this
/// is an enum rather than a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Raise it only when the conversation is explicitly asking for it.
    OnRequest,
}

/// Event types no action may author, whatever else changes.
///
/// See this module's ⛔ note. The four journal events are listed individually
/// rather than matched by prefix so that a fifth one added later fails
/// [`no_action_touches_the_journal`] instead of being caught by a pattern nobody
/// re-reads.
pub const NEVER_PROPOSABLE: &[EventType] = &[
    EventType::JournalEntryCreated,
    EventType::JournalEntryUpdated,
    EventType::JournalEntryClosed,
    EventType::JournalEntryReopened,
];

const NOTE_CREATE: ActionType = ActionType {
    name: "note.create",
    kind: ActionKind::NoteCreate,
    feature: Feature::Notes,
    description: "Create a new note. Use this to capture something the user asked to be \
                  written down, or a summary they asked you to keep.",
    params: &[
        ActionParam::text(
            "title",
            "Short title for the note, as it will appear in their list.",
            200,
        ),
        ActionParam::text(
            "body",
            "The note's content, in markdown. Write it as the user would want to read it \
             later, not as a reply to them.",
            20_000,
        ),
    ],
    produces: &[EventType::GenericNoteCreated],
    // Rule 1: the log holds the create, so deleting the note restores the prior
    // state. Rule 2: nothing leaves the log. Rule 3: a note the user has not
    // opened has not been observed by anyone.
    reversible: true,
    reversibility: "Creating a note is undone by deleting it; nothing leaves the event log.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

/// The three confidence levels, as the model must spell them.
///
/// ⚠️ Three words rather than a number, and that is the decision. A model asked
/// for a numeric confidence returns something like `0.87` with no calibration
/// behind it — precision the value does not have, which then reads as rigour to
/// whoever sees it. Three levels are what a model can actually distinguish and
/// what a person can act on.
pub const CONFIDENCE_LEVELS: &[&str] = &["low", "medium", "high"];

const BELIEF_RECORD: ActionType = ActionType {
    name: "belief.record",
    kind: ActionKind::BeliefRecord,
    feature: Feature::Llm,
    description: "Record a lasting conclusion about the user — a pattern that holds over time, \
                  such as how they tend to work or where they consistently misjudge something. \
                  ⚠️ Only when they have asked you to draw a conclusion. Never volunteer one. \
                  A fact belongs in a note; this is for patterns you could defend from the \
                  records you have read.",
    params: &[
        ActionParam::text(
            "statement",
            "The conclusion, in one sentence, written about the user in the second person. \
             State what holds, not the reasoning that got you there.",
            500,
        ),
        ActionParam::text(
            "confidence",
            "How well the records you read support this: low, medium, or high.",
            10,
        )
        .one_of(CONFIDENCE_LEVELS),
        ActionParam::text(
            "review_after",
            "When this should be re-examined, as YYYY-MM-DD. Choose by how fast the subject \
             changes, not by a default.",
            10,
        )
        .optional(),
        ActionParam::evidence(
            "evidence",
            "Filled in automatically from the records you opened. You cannot set it.",
        ),
    ],
    produces: &[EventType::BeliefRecorded],
    // Rule 1: the log holds the record, so superseding restores the prior state.
    // Rule 2: nothing leaves the log. Rule 3: a belief the user has not read has
    // not been observed — and they see it at the approval gate regardless.
    reversible: true,
    reversibility: "A belief is undone by superseding it; the log keeps both, and nothing \
                    leaves the system.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

const BELIEF_SUPERSEDE: ActionType = ActionType {
    name: "belief.supersede",
    kind: ActionKind::BeliefSupersede,
    feature: Feature::Llm,
    description: "Retire a belief that no longer holds. Use when what you have read contradicts \
                  something previously recorded. The old belief stays in the history with the \
                  reason it was retired.",
    params: &[
        ActionParam::text(
            "belief_id",
            "Identity of the belief to retire, from a search or list of beliefs.",
            64,
        ),
        ActionParam::text(
            "reason",
            "One sentence on what changed, for the user reading this later.",
            500,
        ),
    ],
    produces: &[EventType::BeliefSuperseded],
    reversible: true,
    reversibility: "Superseding only marks the belief retired; the statement and its evidence \
                    stay in the log.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

/// Where a new group or item is placed in its list.
///
/// ⚠️ **The model is not asked for a position, and that is deliberate.** `order`
/// is meaningful only relative to a list the model cannot see, and a number
/// guessed against an unknown list is worse than a predictable one. This sorts
/// last; the user reorders on a screen built for it.
const APPEND_ORDER: u32 = 10_000;

/// Ceiling on a routine item's estimated duration, in minutes.
///
/// A day, because a routine item longer than that is not a habit — the same
/// reasoning that caps `Frequency::Custom` at monthly.
const MAX_ITEM_MINUTES: u32 = 1_440;

fn valid_frequency(value: &str) -> Result<(), String> {
    value
        .parse::<crate::routines::Frequency>()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn valid_minutes(value: &str) -> Result<(), String> {
    match value.parse::<u32>() {
        Ok(n) if n <= MAX_ITEM_MINUTES => Ok(()),
        Ok(_) => Err(format!("must be at most {MAX_ITEM_MINUTES} minutes")),
        Err(_) => Err("must be a whole number of minutes".to_string()),
    }
}

const ROUTINE_CREATE: ActionType = ActionType {
    name: "routine.create",
    kind: ActionKind::RoutineCreate,
    feature: Feature::Routines,
    description: "Create a new routine — a group of things the user does together on a \
                  repeating schedule. Creates the group only; add its items separately.",
    params: &[
        ActionParam::text(
            "name",
            "What the user would call this routine, e.g. \"Morning\" or \"Weekly reset\".",
            120,
        ),
        ActionParam::text(
            "frequency",
            "How often it repeats: daily, weekly, biweekly, monthly, or `custom:N` for \
             every N days (N between 2 and 31).",
            32,
        )
        .checked_by(valid_frequency),
    ],
    produces: &[EventType::RoutineGroupCreated],
    // Rule 1: the log holds the create, and removal is a soft delete. Rule 2:
    // nothing leaves the log. Rule 3: an unopened routine has been observed by
    // nobody.
    reversible: true,
    reversibility: "A routine is undone by removing it, which is a soft delete the log keeps.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

const ROUTINE_ADD_ITEM: ActionType = ActionType {
    name: "routine.add_item",
    kind: ActionKind::RoutineAddItem,
    feature: Feature::Routines,
    description: "Add one thing to an existing routine. Get the routine's id from a search \
                  or list first — you cannot invent it.",
    params: &[
        ActionParam::text(
            "group_id",
            "Identity of the routine to add to, from a search or list of routines.",
            64,
        ),
        ActionParam::text(
            "name",
            "The thing to be done, phrased as the user would.",
            200,
        ),
        ActionParam::text(
            "estimated_duration_min",
            "Roughly how many whole minutes it takes.",
            8,
        )
        .checked_by(valid_minutes),
    ],
    produces: &[EventType::RoutineItemAdded],
    reversible: true,
    reversibility: "An added item is undone by removing it, which is a soft delete.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

const ROUTINE_MODIFY_ITEM: ActionType = ActionType {
    name: "routine.modify_item",
    kind: ActionKind::RoutineModifyItem,
    feature: Feature::Routines,
    description: "Rename one item in a routine, change how long it is expected to take, or \
                  both. Give only what should change.",
    params: &[
        ActionParam::text(
            "item_id",
            "Identity of the item to change, from reading the routine it belongs to.",
            64,
        ),
        ActionParam::text(
            "name",
            "The item's new wording. Omit to leave it alone.",
            200,
        )
        .optional(),
        ActionParam::text(
            "estimated_duration_min",
            "The new estimate in whole minutes. Omit to leave it alone.",
            8,
        )
        .optional()
        .checked_by(valid_minutes),
    ],
    produces: &[EventType::RoutineItemModified],
    reversible: true,
    reversibility: "The log holds the prior wording and estimate, so a change can be reversed \
                    by applying the old values.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

/// Every action this build knows, enabled or not.
pub const ALL_ACTIONS: &[ActionType] = &[
    NOTE_CREATE,
    BELIEF_RECORD,
    BELIEF_SUPERSEDE,
    ROUTINE_CREATE,
    ROUTINE_ADD_ITEM,
    ROUTINE_MODIFY_ITEM,
];

/// The actions whose feature is on, which is the set the model is told about.
pub fn available(config: &ResolvedConfig) -> Vec<&'static ActionType> {
    ALL_ACTIONS
        .iter()
        .filter(|a| config.enabled(a.feature))
        .collect()
}

/// Look one up by name, honouring the feature switch.
///
/// A disabled action is `None` rather than a distinct error: to a caller it does
/// not exist, and saying "exists but is off" would leak the shape of a feature
/// the user turned off into the model's view of the app.
pub fn lookup(config: &ResolvedConfig, name: &str) -> Option<&'static ActionType> {
    ALL_ACTIONS
        .iter()
        .find(|a| a.name == name && config.enabled(a.feature))
}

/// The actions that operate on one record type, for `describe_type`.
///
/// Matched on the `{type}.` prefix of the action name. That is a convention this
/// module owns on both sides — the names are written here — so it is a cheap
/// mapping rather than a guess about someone else's data.
pub fn for_type(config: &ResolvedConfig, type_name: &str) -> Vec<&'static ActionType> {
    let prefix = format!("{type_name}.");
    available(config)
        .into_iter()
        .filter(|a| a.name.starts_with(&prefix))
        .collect()
}

/// What went wrong with a proposal's arguments.
///
/// A string rather than a typed error set, because every consumer renders it: the
/// model sees it as the verb's result and recovers on the next turn, and the
/// approving client shows it when a stored proposal no longer validates.
pub type ArgError = String;

/// Check a proposal's arguments against its declaration, returning the normalized
/// object to store.
///
/// ⚠️ **Unknown keys are rejected, not dropped.** Arguments are model output and
/// may carry text lifted out of a record, so the object has to be closed: a key
/// that means nothing today is a key that means something after the next action
/// is added, and a silently-dropped one leaves a proposal whose stored form does
/// not match what was proposed.
pub fn validate_args(action: &ActionType, args: &Value) -> Result<Value, ArgError> {
    let Some(obj) = args.as_object() else {
        return Err(format!("`args` for {} must be an object", action.name));
    };

    for key in obj.keys() {
        if !action.params.iter().any(|p| p.key == key) {
            let known: Vec<&str> = action
                .params
                .iter()
                .filter(|p| p.is_model_supplied())
                .map(|p| p.key)
                .collect();
            return Err(format!(
                "{} has no argument `{key}`; it takes {}",
                action.name,
                known.join(", ")
            ));
        }
    }

    let mut out = serde_json::Map::new();
    for param in action.params {
        // Evidence is filled from the trace after the run, so it is absent when
        // the model's call is checked and present when the stored proposal is
        // re-checked at approval. Both are valid; what is never valid is the
        // model having written it.
        if param.source == ParamSource::EvidenceFromTrace {
            if let Some(value) = obj.get(param.key) {
                let Some(items) = value.as_array() else {
                    return Err(format!("{}'s `{}` must be a list", action.name, param.key));
                };
                for item in items {
                    let has_identity = item.get("kind").and_then(|v| v.as_str()).is_some()
                        && item.get("id").and_then(|v| v.as_str()).is_some();
                    if !has_identity {
                        return Err(format!(
                            "{}'s `{}` entries each need a `kind` and an `id`",
                            action.name, param.key
                        ));
                    }
                }
                out.insert(param.key.to_string(), value.clone());
            }
            continue;
        }

        match obj.get(param.key) {
            None => {
                if param.required {
                    return Err(format!("{} needs `{}`", action.name, param.key));
                }
            }
            Some(value) => {
                let Some(text) = value.as_str() else {
                    return Err(format!("{}'s `{}` must be text", action.name, param.key));
                };
                let text = text.trim();
                if text.is_empty() {
                    if param.required {
                        return Err(format!("{}'s `{}` is empty", action.name, param.key));
                    }
                    continue;
                }
                if text.chars().count() > param.max_chars {
                    return Err(format!(
                        "{}'s `{}` is longer than {} characters",
                        action.name, param.key, param.max_chars
                    ));
                }
                // Compared case-insensitively and stored lowercased: a model
                // writing "High" means the same thing, and refusing it would
                // spend a turn on nothing.
                if let Some(allowed) = param.allowed {
                    let lowered = text.to_lowercase();
                    if !allowed.contains(&lowered.as_str()) {
                        return Err(format!(
                            "{}'s `{}` must be one of {}",
                            action.name,
                            param.key,
                            allowed.join(", ")
                        ));
                    }
                    out.insert(param.key.to_string(), json!(lowered));
                    continue;
                }
                // The domain's own error text is passed through rather than
                // rewritten: it already names the bounds, and the model reads
                // this on the turn it called, while it can still fix the value.
                if let Some(check) = param.validator
                    && let Err(why) = check(text)
                {
                    return Err(format!("{}'s `{}`: {why}", action.name, param.key));
                }
                out.insert(param.key.to_string(), json!(text));
            }
        }
    }

    Ok(Value::Object(out))
}

/// Whether an action takes system-filled evidence, and under which key.
pub fn evidence_key(action: &ActionType) -> Option<&'static str> {
    action
        .params
        .iter()
        .find(|p| p.source == ParamSource::EvidenceFromTrace)
        .map(|p| p.key)
}

/// Turn approved arguments into the events that carry them out.
///
/// ⚠️ **The only place per-kind branching lives.** Adding an action means one arm
/// here and one entry above; anything else that grows a match on [`ActionKind`]
/// has put a second copy of this decision somewhere it will drift.
///
/// Identity is minted *here*, at approval, not at proposal: a proposal is an
/// intention and carries none. That is what makes approving the same proposal
/// twice a question the caller must answer (it checks the status) rather than a
/// silent duplicate with two different ids.
pub fn build_events(
    action: &ActionType,
    args: &Value,
    device_id: &str,
) -> Result<Vec<NewEvent>, ArgError> {
    // Re-validate rather than trusting the stored object. A proposal can be
    // approved long after it was made, by a build whose declaration has moved.
    let args = validate_args(action, args)?;

    match action.kind {
        ActionKind::NoteCreate => {
            let title = args["title"].as_str().unwrap_or_default();
            let body = args["body"].as_str().unwrap_or_default();
            let note_id = ulid::Ulid::new().to_string();
            Ok(vec![NewEvent::generic_note_created(
                device_id, &note_id, title, body, None,
            )])
        }
        ActionKind::BeliefRecord => {
            let evidence: Vec<crate::events::RecordRef> = args
                .get("evidence")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let payload = crate::events::BeliefRecordedPayload {
                belief_id: ulid::Ulid::new().to_string(),
                statement: args["statement"].as_str().unwrap_or_default().to_string(),
                confidence: args["confidence"].as_str().unwrap_or("low").to_string(),
                review_after: args
                    .get("review_after")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                evidence,
            };
            NewEvent::belief_recorded(device_id, &payload)
                .map(|e| vec![e])
                .map_err(|e| e.to_string())
        }
        ActionKind::BeliefSupersede => {
            let payload = crate::events::BeliefSupersededPayload {
                belief_id: args["belief_id"].as_str().unwrap_or_default().to_string(),
                reason: args["reason"].as_str().map(str::to_string),
                superseded_by: None,
            };
            NewEvent::belief_superseded(device_id, &payload)
                .map(|e| vec![e])
                .map_err(|e| e.to_string())
        }
        ActionKind::RoutineCreate => {
            let group_id = ulid::Ulid::new().to_string();
            Ok(vec![routine_event(
                EventType::RoutineGroupCreated,
                group_id,
                device_id,
                json!({
                    "name": args["name"].as_str().unwrap_or_default(),
                    "frequency": args["frequency"].as_str().unwrap_or("daily"),
                    "order": APPEND_ORDER,
                }),
            )])
        }
        ActionKind::RoutineAddItem => {
            let item_id = ulid::Ulid::new().to_string();
            Ok(vec![routine_event(
                EventType::RoutineItemAdded,
                item_id,
                device_id,
                json!({
                    "group_id": args["group_id"].as_str().unwrap_or_default(),
                    "name": args["name"].as_str().unwrap_or_default(),
                    "estimated_duration_min": minutes(&args, "estimated_duration_min").unwrap_or(0),
                    "order": APPEND_ORDER,
                }),
            )])
        }
        ActionKind::RoutineModifyItem => {
            // ⚠️ The `changes` object is built here, key by key, rather than
            // taken from the model. `RoutineItemModifiedPayload.changes` is
            // free-form JSON, so passing a model-authored blob through would
            // hand it a write surface wider than the action declares — the one
            // place in this module where the payload's own flexibility is the
            // hazard.
            let mut changes = serde_json::Map::new();
            if let Some(name) = args["name"].as_str() {
                changes.insert("name".into(), json!(name));
            }
            if let Some(minutes) = minutes(&args, "estimated_duration_min") {
                changes.insert("estimated_duration_min".into(), json!(minutes));
            }
            if changes.is_empty() {
                return Err(format!(
                    "{} needs `name`, `estimated_duration_min`, or both",
                    action.name
                ));
            }
            let item_id = args["item_id"].as_str().unwrap_or_default().to_string();
            Ok(vec![routine_event(
                EventType::RoutineItemModified,
                item_id.clone(),
                device_id,
                json!({ "item_id": item_id, "changes": Value::Object(changes) }),
            )])
        }
    }
}

/// Envelope for a routine event.
///
/// Routines have no `NewEvent::*` factory — the client's commands build these
/// inline — so the aggregate discipline is applied here instead: a create keys
/// on the identity it just minted, a modify keys on the row it changes.
fn routine_event(
    event_type: EventType,
    aggregate_id: String,
    device_id: &str,
    payload: Value,
) -> NewEvent {
    NewEvent {
        id: None,
        event_type: event_type.to_string(),
        aggregate_id,
        timestamp: chrono::Utc::now(),
        device_id: device_id.to_string(),
        payload,
    }
}

/// A validated minute count, as the number the payload wants.
///
/// The argument arrives as text — every model-supplied argument does, so that
/// one validator shape covers them all — and `valid_minutes` has already proved
/// it parses.
fn minutes(args: &Value, key: &str) -> Option<u32> {
    args[key].as_str().and_then(|s| s.parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ResolvedConfig;

    fn all_on() -> ResolvedConfig {
        // Every feature resolves to its default of `on` when nothing is set.
        ResolvedConfig::new(crate::config::ConfigMap::new(), Default::default())
    }

    fn with_feature_off(feature: Feature) -> ResolvedConfig {
        let mut global = crate::config::ConfigMap::new();
        global.insert(feature.key(), crate::config::ConfigValue::Bool(false));
        ResolvedConfig::new(global, Default::default())
    }

    /// ⛔ The journal invariant. See this module's header — the user is the sole
    /// author of the journal, permanently.
    #[test]
    fn no_action_touches_the_journal() {
        for action in ALL_ACTIONS {
            for produced in action.produces {
                assert!(
                    !NEVER_PROPOSABLE.contains(produced),
                    "{} would author {produced:?}, which the assistant may never propose",
                    action.name
                );
            }
        }
    }

    /// The declaration is only worth checking the guard against if it matches
    /// what the builder does.
    #[test]
    fn declared_events_match_what_is_built() {
        for action in ALL_ACTIONS {
            let built = build_events(action, &sample_args(action), "test-device")
                .expect("sample args should validate");
            let types: Vec<String> = built.iter().map(|e| e.event_type.clone()).collect();
            let declared: Vec<String> = action.produces.iter().map(|t| t.to_string()).collect();
            assert_eq!(types, declared, "{} declares the wrong events", action.name);
        }
    }

    /// Autonomy is Phase G's to grant, and only with the grant events behind it.
    #[test]
    fn every_action_always_asks() {
        for action in ALL_ACTIONS {
            assert_eq!(action.autonomy, Autonomy::AlwaysAsk, "{}", action.name);
        }
    }

    #[test]
    fn names_are_unique_and_prefixed_by_their_type() {
        let mut seen = Vec::new();
        for action in ALL_ACTIONS {
            assert!(!seen.contains(&action.name), "duplicate {}", action.name);
            seen.push(action.name);
            assert!(
                action.name.contains('.'),
                "{} should be `{{type}}.{{operation}}`",
                action.name
            );
        }
    }

    #[test]
    fn a_disabled_feature_hides_its_actions() {
        let config = with_feature_off(Feature::Notes);
        assert!(lookup(&config, "note.create").is_none());
        assert!(available(&config).iter().all(|a| a.name != "note.create"));
    }

    #[test]
    fn unknown_arguments_are_refused_rather_than_dropped() {
        let err = validate_args(
            &NOTE_CREATE,
            &json!({ "title": "t", "body": "b", "tags": "x" }),
        )
        .expect_err("an unknown key must not be silently dropped");
        assert!(err.contains("tags"), "{err}");
    }

    #[test]
    fn a_missing_required_argument_names_itself() {
        let err = validate_args(&NOTE_CREATE, &json!({ "title": "t" })).expect_err("needs body");
        assert!(err.contains("body"), "{err}");
    }

    #[test]
    fn an_over_long_argument_is_refused() {
        let long = "x".repeat(201);
        let err = validate_args(&NOTE_CREATE, &json!({ "title": long, "body": "b" }))
            .expect_err("over the title ceiling");
        assert!(err.contains("title"), "{err}");
    }

    /// The ceiling counts characters, not bytes — a multi-byte title must not be
    /// refused at a quarter of its declared length.
    #[test]
    fn the_length_ceiling_counts_characters() {
        let title = "é".repeat(200);
        validate_args(&NOTE_CREATE, &json!({ "title": title, "body": "b" }))
            .expect("200 characters is 200 characters");
    }

    #[test]
    fn for_type_finds_a_types_actions() {
        let config = all_on();
        let notes = for_type(&config, "note");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].name, "note.create");
        assert!(for_type(&config, "journal").is_empty());
    }

    fn sample_args(action: &ActionType) -> Value {
        match action.kind {
            ActionKind::NoteCreate => json!({ "title": "Renew passport", "body": "Expires May." }),
            ActionKind::BeliefRecord => json!({
                "statement": "You underestimate how long admin tasks take.",
                "confidence": "medium",
                "evidence": [{ "kind": "journal", "id": "2026-08-14" }],
            }),
            ActionKind::BeliefSupersede => json!({
                "belief_id": "01BELIEF0000000000000001",
                "reason": "Their last four estimates were accurate.",
            }),
            ActionKind::RoutineCreate => json!({ "name": "Morning", "frequency": "daily" }),
            ActionKind::RoutineAddItem => json!({
                "group_id": "01GROUP00000000000000001",
                "name": "Stretch",
                "estimated_duration_min": "10",
            }),
            ActionKind::RoutineModifyItem => json!({
                "item_id": "01ITEM000000000000000001",
                "estimated_duration_min": "15",
            }),
        }
    }

    // Routines

    /// ⚠️ Validated by the domain's own parser, not a list re-stated here. The
    /// custom form carries bounds a list cannot express, and a second copy of
    /// those bounds would drift silently.
    #[test]
    fn frequency_is_checked_by_the_real_parser_including_the_custom_bounds() {
        for good in [
            "daily",
            "weekly",
            "biweekly",
            "monthly",
            "custom:2",
            "custom:31",
        ] {
            validate_args(&ROUTINE_CREATE, &json!({ "name": "n", "frequency": good }))
                .unwrap_or_else(|e| panic!("{good} should parse: {e}"));
        }
        for bad in [
            "fortnightly",
            "custom:1",
            "custom:32",
            "custom:x",
            "every day",
        ] {
            let err = validate_args(&ROUTINE_CREATE, &json!({ "name": "n", "frequency": bad }))
                .expect_err("{bad} must be refused");
            assert!(err.contains("frequency"), "{bad}: {err}");
        }
    }

    /// The domain's message reaches the model, so it can fix the value on the
    /// turn it called rather than guessing at a generic refusal.
    #[test]
    fn a_bad_frequency_reports_the_real_bounds() {
        let err = validate_args(
            &ROUTINE_CREATE,
            &json!({ "name": "n", "frequency": "custom:99" }),
        )
        .expect_err("out of bounds");
        assert!(err.contains('2') && err.contains("31"), "{err}");
    }

    #[test]
    fn a_duration_must_be_whole_minutes_within_a_day() {
        let base = json!({ "group_id": "g", "name": "n", "estimated_duration_min": "10" });
        validate_args(&ROUTINE_ADD_ITEM, &base).expect("ten minutes is fine");

        for bad in ["-5", "1441", "ten", "10.5"] {
            let mut args = base.clone();
            args["estimated_duration_min"] = json!(bad);
            assert!(
                validate_args(&ROUTINE_ADD_ITEM, &args).is_err(),
                "{bad} should be refused"
            );
        }
    }

    /// ⚠️ `changes` is free-form JSON on the payload, so it is assembled key by
    /// key here. A model-authored blob would be a write surface wider than the
    /// action declares.
    #[test]
    fn modify_builds_its_changes_from_declared_keys_only() {
        let events = build_events(
            &ROUTINE_MODIFY_ITEM,
            &json!({ "item_id": "i1", "name": "Stretch longer" }),
            "test-device",
        )
        .expect("a rename alone is valid");
        let changes = &events[0].payload["changes"];
        assert_eq!(changes["name"], json!("Stretch longer"));
        assert!(
            changes.get("estimated_duration_min").is_none(),
            "an omitted field must not be written: {changes}"
        );
        assert_eq!(
            events[0].aggregate_id, "i1",
            "modify keys on the row it changes"
        );
    }

    /// A modify with nothing to modify is a no-op event that would still appear
    /// in the user's inbox as a decision to take.
    #[test]
    fn modify_with_no_changes_is_refused() {
        let err = build_events(&ROUTINE_MODIFY_ITEM, &json!({ "item_id": "i1" }), "d")
            .expect_err("nothing to change");
        assert!(err.contains("name"), "{err}");
    }

    /// ⚠️ The model is never asked where a new thing goes: `order` is relative to
    /// a list it cannot see, and a guess against an unknown list is worse than a
    /// predictable position the user can correct.
    #[test]
    fn placement_is_not_something_the_model_supplies() {
        for action in [&ROUTINE_CREATE, &ROUTINE_ADD_ITEM] {
            assert!(
                !action.params.iter().any(|p| p.key == "order"),
                "{} must not ask the model for a position",
                action.name
            );
        }
        let events = build_events(&ROUTINE_CREATE, &sample_args(&ROUTINE_CREATE), "d").unwrap();
        assert_eq!(events[0].payload["order"], json!(APPEND_ORDER));
    }

    // Belief-specific rules

    #[test]
    fn a_confidence_outside_the_three_levels_is_refused() {
        let err = validate_args(
            &BELIEF_RECORD,
            &json!({ "statement": "s", "confidence": "0.87" }),
        )
        .expect_err("a number is not one of the three levels");
        assert!(err.contains("low, medium, high"), "{err}");
    }

    #[test]
    fn confidence_is_accepted_case_insensitively_and_stored_lowercased() {
        let out = validate_args(
            &BELIEF_RECORD,
            &json!({ "statement": "s", "confidence": "High" }),
        )
        .expect("`High` means high");
        assert_eq!(out["confidence"], json!("high"));
    }

    /// ⚠️ Evidence is system-filled. The model must not be able to name its own
    /// sources — that is the whole reason the parameter has a separate source.
    #[test]
    fn the_model_is_not_offered_evidence_as_an_argument() {
        let model_keys: Vec<&str> = BELIEF_RECORD
            .params
            .iter()
            .filter(|p| p.is_model_supplied())
            .map(|p| p.key)
            .collect();
        assert!(!model_keys.contains(&"evidence"), "{model_keys:?}");
        assert_eq!(evidence_key(&BELIEF_RECORD), Some("evidence"));
        assert_eq!(evidence_key(&NOTE_CREATE), None);
    }

    /// The stored proposal carries evidence, so re-validation at approval has to
    /// accept it — while still refusing a shape that is not a record reference.
    #[test]
    fn stored_evidence_survives_revalidation_but_a_bad_shape_does_not() {
        validate_args(
            &BELIEF_RECORD,
            &json!({
                "statement": "s", "confidence": "low",
                "evidence": [{ "kind": "note", "id": "01N" }],
            }),
        )
        .expect("a well-formed record reference");

        let err = validate_args(
            &BELIEF_RECORD,
            &json!({
                "statement": "s", "confidence": "low",
                "evidence": [{ "kind": "note" }],
            }),
        )
        .expect_err("an entry with no id is not a citation");
        assert!(err.contains("evidence"), "{err}");
    }

    #[test]
    fn a_belief_with_no_evidence_is_still_well_formed() {
        // It is the user's call at the gate, not ours: an unsupported conclusion
        // shown and declined beats one silently dropped.
        validate_args(
            &BELIEF_RECORD,
            &json!({ "statement": "s", "confidence": "low" }),
        )
        .expect("evidence is not required to be well-formed");
    }

    /// Phase G's job, and it must not arrive by accident here either.
    #[test]
    fn every_action_is_proposed_only_on_request() {
        for action in ALL_ACTIONS {
            assert_eq!(action.trigger, Trigger::OnRequest, "{}", action.name);
        }
    }
}
