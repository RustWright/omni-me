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
    NoteAppend,
    NoteRevise,
    NoteRename,
    BeliefRecord,
    BeliefSupersede,
    RoutineCreate,
    RoutineAddItem,
    RoutineModifyItem,
    RoutineComplete,
    RoutineSkip,
}

/// Where an argument's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamSource {
    /// The model supplies it in the `propose` call.
    Model,
    /// **The system fills it from the run's trace** — the records the assistant
    /// actually opened while reaching this conclusion.
    ///
    /// ⚠️ Deliberately not something the model can author. Letting it list its
    /// own evidence reopens exactly the hallucination surface `records_read`
    /// closes: a citation is only worth anything if it names a record that was
    /// demonstrably in hand.
    ///
    /// ⚠️ **The guarantee is an overwrite, not a refusal — do not mistake one for
    /// the other.** `describe_type` renders this parameter like any other (its
    /// description is what tells the model it cannot set it), and
    /// [`validate_args`] *accepts* a well-shaped value, because it cannot tell a
    /// model's fresh call from a stored proposal being re-checked at approval,
    /// where the evidence is legitimately present. What makes model-authored
    /// evidence impossible is `answer::proposals`, which inserts the
    /// trace-derived list over whatever is there. ⛔ Removing that insert on the
    /// belief that validation already guards this would silently hand the model
    /// its own citations.
    EvidenceFromTrace,
    /// **The system fills it at approval, from the record being changed.**
    ///
    /// The one argument that exists because of *when* it can be known rather than
    /// who knows it: `note.revise` sends an anchor and a replacement, and the
    /// finished body only exists once the anchor has been resolved against the
    /// note as it stands at the moment of approval. Resolving it when the
    /// proposal was made would bake in a body that may be stale by the time
    /// anyone reads the card.
    ///
    /// ⚠️ Same guarantee and same non-guarantee as
    /// [`ParamSource::EvidenceFromTrace`]: `validate_args` *accepts* a
    /// well-shaped value, because a stored proposal being re-checked at approval
    /// legitimately carries one. What makes a model-authored value impossible to
    /// apply is `inbox::resolve_args` writing over it.
    ///
    /// ⛔ Never `required`. It is absent on the model's call and present on the
    /// re-check, and a declaration that demanded it would refuse every proposal
    /// at the moment it was made.
    ResolvedAtApproval,
}

/// What an argument's JSON value looks like.
///
/// Separate from [`ParamSource`] because the two answer different questions —
/// source is *who writes it*, shape is *what it looks like* — and an argument
/// varies on both independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamShape {
    /// One string. Every model-supplied argument was this until batching.
    Text,
    /// A list of strings, each held to the same `max_chars` and the same
    /// validator a single [`ParamShape::Text`] argument would be.
    ///
    /// ⚠️ **The count bound is part of the shape, not decoration.** One approval
    /// authors one event per element, and a card the user cannot check against
    /// reality is not an approval — it is a rubber stamp with a number on it.
    TextList { max_items: usize },
    /// A list of `{kind, id}` objects. Only [`ParamSource::EvidenceFromTrace`]
    /// carries this, and the system writes it.
    Records,
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
    /// Keep the value's leading and trailing whitespace instead of trimming it.
    ///
    /// ⚠️ **Trimming is right for a name and wrong for an anchor.** Every
    /// argument here was a title, an id or a whole added block until
    /// `note.revise`, and for those a stray newline is noise. For an anchored
    /// splice the whitespace *is* the edit: markdown indentation nests a list
    /// item, and a trimmed replacement can come out byte-identical to the text it
    /// replaces — which applies cleanly, changes nothing, and reports success.
    /// That is the exact failure this feature was required not to have.
    pub verbatim: bool,
    pub source: ParamSource,
    /// What the value looks like. ⚠️ Rendered to the model by `describe_type`,
    /// which is the only way it can learn that an argument takes a list —
    /// nothing else in the registry is typed, so leaving this out of the
    /// rendering costs a turn per wrong guess.
    pub shape: ParamShape,
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
            verbatim: false,
            source: ParamSource::Model,
            shape: ParamShape::Text,
        }
    }

    /// A list of plain text arguments, each checked exactly as a
    /// [`ActionParam::text`] of the same `max_chars` would be.
    ///
    /// ⚠️ **A batch is one decision, so it has to stay one claim.** What varies
    /// across the list is the identity; everything else about the proposal — the
    /// day, the routine, the evidence — is shared, and that is what lets the card
    /// state the batch in a sentence a person can accept or refuse as a whole.
    /// ⛔ Do not reach for this to let one proposal span two days or two
    /// routines: there is no partial approval, so a card the user half-agrees
    /// with has no outcome they can choose.
    pub const fn list(
        key: &'static str,
        description: &'static str,
        max_chars: usize,
        max_items: usize,
    ) -> Self {
        ActionParam {
            key,
            required: true,
            description,
            max_chars,
            allowed: None,
            validator: None,
            verbatim: false,
            source: ParamSource::Model,
            shape: ParamShape::TextList { max_items },
        }
    }

    /// A value the system resolves at approval. See
    /// [`ParamSource::ResolvedAtApproval`].
    ///
    /// `max_chars` bounds it like any other argument. The bound is not the
    /// interesting part — the *applied* event carries the same body whatever the
    /// bound is — but leaving it unbounded would mean one declaration in this
    /// file with no ceiling at all, and the reason every other one has a ceiling
    /// (an argument is stored in an event, and every event syncs to the phone)
    /// applies here too.
    ///
    /// ⚠️ `verbatim` is set but inert today: [`validate_args`] gives this source
    /// its own branch and never reaches `checked_text`. It is declared anyway so
    /// that removing that branch degrades to trimming-off rather than to silently
    /// trimming a note's trailing newline off every body it writes.
    pub const fn resolved(key: &'static str, description: &'static str, max_chars: usize) -> Self {
        ActionParam {
            key,
            required: false,
            description,
            max_chars,
            allowed: None,
            validator: None,
            verbatim: true,
            source: ParamSource::ResolvedAtApproval,
            shape: ParamShape::Text,
        }
    }

    pub const fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Keep the value's surrounding whitespace. See [`ActionParam::verbatim`].
    pub const fn verbatim(mut self) -> Self {
        self.verbatim = true;
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
            verbatim: false,
            source: ParamSource::EvidenceFromTrace,
            shape: ParamShape::Records,
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

/// Adding to the end of a note the user already has.
///
/// ⚠️ **Appending rather than rewriting is the whole point of the action.** The
/// alternative considered and rejected was a `note.update` carrying the finished
/// body: it costs tokens in proportion to the note's length, and — the real
/// objection — the model has to reproduce the parts it was not asked to change,
/// so a quietly reworded paragraph is indistinguishable from the edit that was
/// asked for. This action cannot drop what it did not send.
///
/// ⛔ **The journal is not reachable from here**, and not because notes and
/// journals happen to use different events. The user is the journal's sole
/// author; see this module's ⛔ note and [`NEVER_PROPOSABLE`].
const NOTE_APPEND: ActionType = ActionType {
    name: "note.append",
    kind: ActionKind::NoteAppend,
    feature: Feature::Notes,
    description: "Add text to the end of a note that already exists. Use this to add to \
                  something rather than replace it — what is already in the note stays, and \
                  you do not need to repeat any of it. Send only the new text.",
    params: &[
        ActionParam::text(
            "note_id",
            "Identity of the note to add to, from having read it.",
            64,
        ),
        ActionParam::text(
            "added_text",
            "Only the new text, in markdown. It is placed after what is already there, \
             separated by a blank line. Do not repeat the existing note.",
            20_000,
        ),
    ],
    produces: &[EventType::GenericNoteAppended],
    // Rule 1: the added text is derivable from the log, so removing it restores
    // the prior body. Rule 2: nothing leaves the log. Rule 3: text added to a
    // note the user has not reopened has not been observed.
    reversible: true,
    reversibility: "The added text is in the log, so the note's previous body can be restored.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

/// Changing text in the middle of a note the user already has.
///
/// ## Why this is not the `note.update` that was rejected
///
/// [`NOTE_APPEND`]'s note records the objection: a model handed the finished body
/// has to reproduce the parts it was not asked to change, so a quietly reworded
/// paragraph is indistinguishable from the edit that was asked for. That
/// objection is about **the model authoring the body**, not about the event that
/// overwrites one — `GenericNoteUpdated` is what every autosave from the notes
/// editor already writes. So the model sends only the region it touched, and the
/// **system** splices it into the live body at approval. Nothing that can reword a
/// paragraph ever sees the paragraphs it was not asked about.
///
/// ## ⛔ Why the anchor is text and the splice happens at approval
///
/// A byte offset into a body another device has already rewritten points at the
/// wrong place; `GenericNoteAppendedPayload` refuses to carry one for that reason.
/// Text carries its own identity instead.
///
/// The anchor is resolved **once, on the approving device**, and the event that
/// reaches the log is the finished body. ⛔ Do not move the resolution into the
/// projection, however naturally an anchored `GenericNoteRevised` event seems to
/// fit there: projections consume events in `received_at` order, which is local
/// arrival time, so two devices legitimately fold the same events in different
/// orders. An anchor evaluated per device can match on one and miss on another,
/// and nothing afterwards reconciles them — a note that silently differs between
/// the phone and the desktop, permanently. Logging the outcome instead of the
/// intent is what makes every device agree.
///
/// ⚠️ What this does *not* fix is last-write-wins itself (`architecture.md`): a
/// device holding an older copy of the note writes an overwrite computed from it,
/// exactly as that device's own editor would. This action adds no new exposure to
/// that; it inherits the note editor's.
const NOTE_REVISE: ActionType = ActionType {
    name: "note.revise",
    kind: ActionKind::NoteRevise,
    feature: Feature::Notes,
    description: "Change text inside a note that already exists, rather than adding to the end. \
                  Give the exact text to find and what to put in its place. The text you give \
                  must appear once and only once in the note, or the change will not be applied.",
    params: &[
        ActionParam::text(
            "note_id",
            "Identity of the note to change, from having read it.",
            64,
        ),
        // ⚠️ The description carries the whole accuracy burden of this action.
        // The match is exact and unique or it is refused, so a paraphrase of what
        // the model *remembers* reading is the failure mode to expect, and the
        // only place to head it off is here.
        ActionParam::text(
            "find",
            "The exact text to replace, copied character for character from the note you read, \
             including punctuation, capitalisation and line breaks. Give enough of it to appear \
             only once in the note. Do not paraphrase it and do not tidy it up: if it does not \
             match exactly, nothing is changed.",
            20_000,
        )
        .verbatim(),
        ActionParam::text(
            "replace",
            "What to put in its place, in markdown. Leave it out to delete the text instead. \
             Indentation matters: markdown uses it for nesting.",
            20_000,
        )
        .optional()
        .verbatim(),
        // Bounded well above any real note rather than at a note's own limits:
        // a note has no aggregate size cap (the editor imposes none and appends
        // accumulate), so a ceiling anywhere near one would refuse the approval
        // of a long note that was never anyone's mistake.
        ActionParam::resolved(
            "raw_text",
            "Filled in automatically when the user approves, by finding your text in the note as \
             it stands then. You cannot set it.",
            200_000,
        ),
    ],
    produces: &[EventType::GenericNoteUpdated],
    // Rule 1: the replaced text is in the log — the proposal carries it — and the
    // body before the change replays from the note's own earlier events, so the
    // prior state is recoverable. Rule 2: nothing leaves the log. Rule 3: a change
    // to a note the user has not reopened has not been observed.
    reversible: true,
    reversibility: "The replaced text is in the log, so the note's previous body can be restored.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

/// Retitling a note. Independent of [`NOTE_APPEND`] — the event already existed.
const NOTE_RENAME: ActionType = ActionType {
    name: "note.rename",
    kind: ActionKind::NoteRename,
    feature: Feature::Notes,
    description: "Change a note's title. The body is untouched. Use this when the title no \
                  longer describes what the note has become.",
    params: &[
        ActionParam::text(
            "note_id",
            "Identity of the note to retitle, from having read it.",
            64,
        ),
        ActionParam::text(
            "title",
            "The new title, as it will appear in their list.",
            200,
        ),
    ],
    produces: &[EventType::GenericNoteRenamed],
    reversible: true,
    reversibility: "The previous title is in the log, so a rename can be reversed.",
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
        .optional()
        // The sibling of the completion dates, and checked the same way. Here an
        // odd spelling would not corrupt anything — `is_due_for_review` parses
        // the loose form too — but one spelling across the app is what stops the
        // next date field deciding for itself.
        .checked_by(valid_future_date),
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

/// A day in exactly the app's spelling, or the reason it is not one.
///
/// ⚠️ **The round-trip check is the point, not pedantry.** `chrono` accepts
/// `2026-8-14` for `%Y-%m-%d`, and a routine completion's row id is
/// `{item_id}-{date}-done` built by concatenation — so an unpadded day keys a
/// *different* row than the same tick made in the app, and the app's undo would
/// then leave the assistant's copy behind. Refusing is the loud failure the
/// model fixes on its next turn; accepting is a duplicate nobody sees.
fn canonical_date(value: &str) -> Result<chrono::NaiveDate, String> {
    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "must be a day written as YYYY-MM-DD".to_string())?;
    if date.format("%Y-%m-%d").to_string() != value {
        return Err("must be a day written as YYYY-MM-DD, with the zeroes".to_string());
    }
    Ok(date)
}

/// A day something is claimed to have happened on: canonical, and not ahead of
/// today.
///
/// ⚠️ Time-dependent, which a validator otherwise is not, and the direction is
/// what makes it safe. A stored proposal is re-validated at approval, so a
/// clock-sensitive rule risks refusing something the user was already shown.
/// Here the clock can only move a date the permissive way — future becomes past
/// — so waiting can legalise a refused proposal and can never invalidate a live
/// one.
fn valid_completion_date(value: &str) -> Result<(), String> {
    if canonical_date(value)? > super::memory::today() {
        return Err("cannot be a day that has not happened yet".to_string());
    }
    Ok(())
}

/// A day that is allowed to be in the future, for scheduling something.
fn valid_future_date(value: &str) -> Result<(), String> {
    canonical_date(value).map(|_| ())
}

/// How many routine items one proposal may name.
///
/// ⚠️ **Sized by what a person can check, not by what a routine can hold.** The
/// card states the batch as a count and the rationale names the items; past a
/// dozen or so the user is agreeing to a number rather than to a claim, and the
/// approval stops being one. A longer routine is not refused — it is two
/// proposals, which is also two chances to disagree.
const BATCH_LIMIT: usize = 12;

/// Marking routine items done on a given day.
///
/// ⚠️ **This is the one action whose argument is a claim about the world**, and
/// it is why `date` and `evidence` are both required in spirit. Everything else
/// here proposes a change the user can evaluate on its face — a note's wording
/// is right there. A completion asserts that something happened, and the user
/// reads their own history back as fact. So the proposal has to carry what it
/// read: the card says *you wrote about it on the 14th*, not *done*.
///
/// It is nonetheless **grantable like any other reversible action**, and that is
/// deliberate rather than an oversight of the three rules. Undo is derivable,
/// nothing leaves the log, and nothing outside observed it. The user's stated
/// reason for making the journal readable was precisely this — they write about
/// what they did, and having to re-tick it by hand is the friction that left
/// routines unused. Making completion permanently un-grantable would foreclose
/// the behaviour the readability was for.
///
/// ⚠️ **One proposal, many items — and no partial approval.** A morning written
/// up in one journal entry is one reading and one decision, so it is one card
/// rather than five stacked ones the user clears by reflex. The cost is real and
/// chosen: there is no way to accept four of five, and a user who disagrees with
/// one item must refuse the batch and let the assistant re-propose. That is the
/// right way round — refusing is the cheap outcome, and an inbox that trains
/// reflex approval is the expensive one. ⛔ Do not "fix" this by splitting a
/// batch at approval; the proposal the user read is the thing being approved.
const ROUTINE_COMPLETE: ActionType = ActionType {
    name: "routine.complete",
    kind: ActionKind::RoutineComplete,
    feature: Feature::Routines,
    description: "Mark routine items done on a particular day. Use this when what you have \
                  read says they did them — a journal entry describing the morning, for \
                  example. Name every item from the same routine and the same day in one \
                  call; use another call for another day or another routine. The day is the \
                  day they did it, not today. Do not mark something done because it was \
                  scheduled.",
    params: &[
        ActionParam::list(
            "item_ids",
            "Identities of the items they did, from reading the routine they belong to. A \
             list even when it names only one.",
            64,
            BATCH_LIMIT,
        ),
        ActionParam::text(
            "group_id",
            "Identity of the routine that item belongs to, from the same read.",
            64,
        ),
        ActionParam::text(
            "date",
            "The day it was done, as YYYY-MM-DD. Take it from what you read, not from \
             today's date.",
            10,
        )
        .checked_by(valid_completion_date),
        ActionParam::evidence(
            "evidence",
            "Filled in automatically from the records you opened. You cannot set it.",
        ),
    ],
    produces: &[EventType::RoutineItemCompleted],
    // Rule 1: `routine_item_completion_undone` deletes the row outright, and the
    // completion event stays in the log. Rule 2: nothing leaves the log. Rule 3:
    // a tick is not shown to anyone at the moment it lands.
    reversible: true,
    reversibility: "A completion is undone by the undo event, which deletes the row; the log \
                    keeps both.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

const ROUTINE_SKIP: ActionType = ActionType {
    name: "routine.skip",
    kind: ActionKind::RoutineSkip,
    feature: Feature::Routines,
    description: "Mark routine items deliberately skipped on a particular day, with the \
                  reason if they gave one. A skip is a decision they made, not an item left \
                  undone — leave an unmentioned item alone. Name every item skipped for the \
                  same reason on the same day in one call.",
    params: &[
        ActionParam::list(
            "item_ids",
            "Identities of the items they skipped, from reading the routine they belong to. \
             A list even when it names only one.",
            64,
            BATCH_LIMIT,
        ),
        ActionParam::text(
            "group_id",
            "Identity of the routine that item belongs to, from the same read.",
            64,
        ),
        ActionParam::text("date", "The day it was skipped, as YYYY-MM-DD.", 10)
            .checked_by(valid_completion_date),
        ActionParam::text(
            "reason",
            "Why, in their words if they gave one — one reason covering everything in this \
             call. Omit rather than inventing one.",
            300,
        )
        .optional(),
        ActionParam::evidence(
            "evidence",
            "Filled in automatically from the records you opened. You cannot set it.",
        ),
    ],
    produces: &[EventType::RoutineItemSkipped],
    reversible: true,
    reversibility: "A skip is undone by the skip-undo event, which deletes the row; the log \
                    keeps both.",
    autonomy: Autonomy::AlwaysAsk,
    trigger: Trigger::OnRequest,
};

/// Every action this build knows, enabled or not.
pub const ALL_ACTIONS: &[ActionType] = &[
    NOTE_CREATE,
    NOTE_APPEND,
    NOTE_REVISE,
    NOTE_RENAME,
    BELIEF_RECORD,
    BELIEF_SUPERSEDE,
    ROUTINE_CREATE,
    ROUTINE_ADD_ITEM,
    ROUTINE_MODIFY_ITEM,
    ROUTINE_COMPLETE,
    ROUTINE_SKIP,
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

        // ⚠️ Its own branch rather than the optional-text path below, for one
        // reason that path cannot give it: **an empty value has to survive.**
        // Blank-and-optional is dropped down there, which is right for an
        // argument the model declined to fill and wrong here — a splice that
        // deletes a note's last line resolves to an empty body, and dropping it
        // would reach `build_events` looking exactly like a resolution that never
        // ran. Those two must stay distinguishable: one is a legal edit, the
        // other is a bug that would overwrite a note with nothing.
        if param.source == ParamSource::ResolvedAtApproval {
            if let Some(value) = obj.get(param.key) {
                let Some(text) = value.as_str() else {
                    return Err(format!("{}'s `{}` must be text", action.name, param.key));
                };
                if text.chars().count() > param.max_chars {
                    return Err(format!(
                        "{}'s `{}` is longer than {} characters",
                        action.name, param.key, param.max_chars
                    ));
                }
                out.insert(param.key.to_string(), json!(text));
            }
            continue;
        }

        match obj.get(param.key) {
            None => {
                if param.required {
                    return Err(format!("{} needs `{}`", action.name, param.key));
                }
            }
            Some(value) => match param.shape {
                ParamShape::Text => match checked_text(action, param, param.key, value)? {
                    Some(checked) => {
                        out.insert(param.key.to_string(), checked);
                    }
                    None if param.required => {
                        return Err(format!("{}'s `{}` is empty", action.name, param.key));
                    }
                    // Blank and optional: dropped rather than stored, so an
                    // argument the model declined to fill is absent from the
                    // payload instead of present and meaningless.
                    None => {}
                },
                ParamShape::TextList { max_items } => {
                    let Some(items) = value.as_array() else {
                        return Err(format!(
                            "{}'s `{}` must be a list, even when it names only one",
                            action.name, param.key
                        ));
                    };
                    if items.is_empty() {
                        return Err(format!("{}'s `{}` is empty", action.name, param.key));
                    }
                    if items.len() > max_items {
                        return Err(format!(
                            "{}'s `{}` takes at most {max_items} at a time; propose the rest \
                             separately",
                            action.name, param.key
                        ));
                    }
                    let mut checked = Vec::with_capacity(items.len());
                    for (index, item) in items.iter().enumerate() {
                        let label = format!("{}[{index}]", param.key);
                        match checked_text(action, param, &label, item)? {
                            // Refused rather than quietly deduplicated. The same
                            // identity twice means the model has lost track of
                            // what it is acting on, and downstream the UPSERT
                            // would make the second write a silent no-op — the
                            // confusion would land in the log and nowhere else.
                            Some(value) if checked.contains(&value) => {
                                return Err(format!(
                                    "{}'s `{}` names the same one twice",
                                    action.name, param.key
                                ));
                            }
                            Some(value) => checked.push(value),
                            None => {
                                return Err(format!("{}'s `{label}` is empty", action.name));
                            }
                        }
                    }
                    out.insert(param.key.to_string(), Value::Array(checked));
                }
                // Unreachable by construction — only `ActionParam::evidence`
                // builds this shape and it sets the source the branch above
                // catches — but said as a message rather than a panic, because
                // this function's whole job is to be the thing model output
                // cannot crash.
                ParamShape::Records => {
                    return Err(format!(
                        "{}'s `{}` is filled in by the system",
                        action.name, param.key
                    ));
                }
            },
        }
    }

    Ok(Value::Object(out))
}

/// One text value held to a parameter's rules, or the reason it is not one.
///
/// Shared by [`ParamShape::Text`] and by every element of a
/// [`ParamShape::TextList`], so that what counts as a valid text argument has
/// exactly one definition. `label` is what the message names — the key itself
/// for a lone value, `key[3]` for an element — because a batch refused without
/// saying *which* element is a batch the model can only fix by guessing.
///
/// `Ok(None)` means blank. The caller decides what that is worth: dropped for a
/// lone optional argument, refused inside a list, where it is a hole that would
/// otherwise silently shorten the batch.
fn checked_text(
    action: &ActionType,
    param: &ActionParam,
    label: &str,
    value: &Value,
) -> Result<Option<Value>, ArgError> {
    let Some(text) = value.as_str() else {
        return Err(format!("{}'s `{label}` must be text", action.name));
    };
    // ⚠️ Trimmed unless the parameter asked not to be. See
    // [`ActionParam::verbatim`] for why an anchored splice is the exception: a
    // trimmed replacement can come back byte-identical to what it replaces.
    // Blankness is still judged on the trimmed form either way, so a verbatim
    // argument holding only spaces is blank rather than a value made of nothing.
    let text = if param.verbatim { text } else { text.trim() };
    if text.trim().is_empty() {
        return Ok(None);
    }
    if text.chars().count() > param.max_chars {
        return Err(format!(
            "{}'s `{label}` is longer than {} characters",
            action.name, param.max_chars
        ));
    }
    // Compared case-insensitively and stored lowercased: a model writing "High"
    // means the same thing, and refusing it would spend a turn on nothing.
    if let Some(allowed) = param.allowed {
        let lowered = text.to_lowercase();
        if !allowed.contains(&lowered.as_str()) {
            return Err(format!(
                "{}'s `{label}` must be one of {}",
                action.name,
                allowed.join(", ")
            ));
        }
        return Ok(Some(json!(lowered)));
    }
    // The domain's own error text is passed through rather than rewritten: it
    // already names the bounds, and the model reads this on the turn it called,
    // while it can still fix the value.
    if let Some(check) = param.validator
        && let Err(why) = check(text)
    {
        return Err(format!("{}'s `{label}`: {why}", action.name));
    }
    Ok(Some(json!(text)))
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
        // ⚠️ Both key on the note they change, not on a fresh identity — the
        // aggregate is the note, and `NotesProjection` addresses the row by
        // `aggregate_id`. A minted id here would write to a note nobody has.
        ActionKind::NoteAppend => {
            let note_id = args["note_id"].as_str().unwrap_or_default().to_string();
            Ok(vec![note_event(
                EventType::GenericNoteAppended,
                note_id.clone(),
                device_id,
                json!({
                    "note_id": note_id,
                    "added_text": args["added_text"].as_str().unwrap_or_default(),
                }),
            )])
        }
        // ⛔ Reads the body it writes, never builds one. The splice lives in
        // `revise::splice` and runs in `inbox::resolve_args`, which is the only
        // place that can see the note as it stands at approval. An absent
        // `raw_text` means that step did not run, and the only safe answer is to
        // refuse: the alternatives are writing an empty body over the note, or
        // writing the anchor as if it were the body.
        ActionKind::NoteRevise => {
            let note_id = args["note_id"].as_str().unwrap_or_default().to_string();
            let Some(raw_text) = args.get("raw_text").and_then(|v| v.as_str()) else {
                return Err(format!(
                    "{} cannot be built until its `raw_text` is resolved against the note",
                    action.name
                ));
            };
            Ok(vec![note_event(
                EventType::GenericNoteUpdated,
                note_id.clone(),
                device_id,
                json!({ "note_id": note_id, "raw_text": raw_text }),
            )])
        }
        ActionKind::NoteRename => {
            let note_id = args["note_id"].as_str().unwrap_or_default().to_string();
            Ok(vec![note_event(
                EventType::GenericNoteRenamed,
                note_id.clone(),
                device_id,
                json!({
                    "note_id": note_id,
                    "title": args["title"].as_str().unwrap_or_default(),
                }),
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
        // ⚠️ The batch stops here. One proposal fans out into one event per item,
        // each carrying the singular `item_id` the app's own tick writes — so the
        // projection, the sync payload and the undo path never learn that
        // batching exists. ⛔ Do not add a plural payload key to "match" the
        // argument: a second event shape for the same fact is a second thing the
        // projection has to fold, and the app would still only write the first.
        ActionKind::RoutineComplete => {
            // ⚠️ Recorded once, outside the loop, so every tick in one approval
            // shares a timestamp. Per-item clock reads would order the batch by
            // nothing more than iteration, and this column is a tiebreak
            // (`db::queries`) — a false ordering is worse than none.
            let recorded_at = chrono::Utc::now().to_rfc3339();
            Ok(item_ids(action, &args)?
                .into_iter()
                .map(|item_id| {
                    routine_event(
                        EventType::RoutineItemCompleted,
                        item_id.to_string(),
                        device_id,
                        json!({
                            "item_id": item_id,
                            "group_id": args["group_id"].as_str().unwrap_or_default(),
                            "date": args["date"].as_str().unwrap_or_default(),
                            // ⚠️ When it was *recorded*, not when the activity
                            // happened — `date` carries that, and a retroactive
                            // tick must not invent a clock time it never read.
                            // Safe because the column is only ever an ordering
                            // tiebreak, never rendered.
                            "completed_at": recorded_at,
                        }),
                    )
                })
                .collect())
        }
        ActionKind::RoutineSkip => Ok(item_ids(action, &args)?
            .into_iter()
            .map(|item_id| {
                let mut payload = json!({
                    "item_id": item_id,
                    "group_id": args["group_id"].as_str().unwrap_or_default(),
                    "date": args["date"].as_str().unwrap_or_default(),
                });
                // Absent rather than null: the payload skips serializing `None`,
                // so a reason-less skip written here has to match one written by
                // the app. One reason covers the batch — see the entry.
                if let Some(reason) = args["reason"].as_str() {
                    payload["reason"] = json!(reason);
                }
                routine_event(
                    EventType::RoutineItemSkipped,
                    item_id.to_string(),
                    device_id,
                    payload,
                )
            })
            .collect()),
    }
}

/// Envelope for a note event that changes a note the user already has.
///
/// `NewEvent::generic_note_created` exists for the create because the notes
/// command and the importer share it; these have no caller outside this module,
/// so the envelope is built here rather than adding factories nothing else uses.
fn note_event(event_type: EventType, note_id: String, device_id: &str, payload: Value) -> NewEvent {
    NewEvent {
        id: None,
        event_type: event_type.to_string(),
        aggregate_id: note_id,
        timestamp: chrono::Utc::now(),
        device_id: device_id.to_string(),
        payload,
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

/// The item identities a batched routine action names, in the order proposed.
///
/// ⚠️ **Refuses an empty list rather than yielding one**, even though
/// [`validate_args`] has already run and cannot let one through. An action that
/// quietly builds no events would mark its proposal approved, write nothing, and
/// leave the user believing the tick landed — the one failure this path must not
/// have. The redundancy costs a branch; the silence would cost a lie.
fn item_ids<'a>(action: &ActionType, args: &'a Value) -> Result<Vec<&'a str>, ArgError> {
    let ids: Vec<&str> = args["item_ids"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if ids.is_empty() {
        return Err(format!("{} names no items", action.name));
    }
    Ok(ids)
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

    /// ⛔ The journal's empty list is the assertion that matters here, and it is a
    /// product invariant rather than a gap: the user is its sole author. Notes
    /// are checked by name rather than by count so that adding a fourth note
    /// action does not silently pass a test about which ones exist.
    #[test]
    fn for_type_finds_a_types_actions() {
        let config = all_on();
        let names: Vec<&str> = for_type(&config, "note").iter().map(|a| a.name).collect();
        assert_eq!(
            names,
            ["note.create", "note.append", "note.revise", "note.rename"]
        );
        assert!(for_type(&config, "journal").is_empty());
    }

    fn sample_args(action: &ActionType) -> Value {
        match action.kind {
            ActionKind::NoteCreate => json!({ "title": "Renew passport", "body": "Expires May." }),
            ActionKind::NoteAppend => json!({
                "note_id": "01NOTE000000000000000001",
                "added_text": "Booked the appointment for the 3rd.",
            }),
            // ⚠️ Carries `raw_text`, because this is the shape at *approval* —
            // which is the only point `build_events` is reached from. A sample
            // without it would make `declared_events_match_what_is_built` assert
            // over an input the builder is designed to refuse.
            ActionKind::NoteRevise => json!({
                "note_id": "01NOTE000000000000000001",
                "find": "due in May.",
                "replace": "due in June.",
                "raw_text": "Renewal is due in June.",
            }),
            ActionKind::NoteRename => json!({
                "note_id": "01NOTE000000000000000001",
                "title": "Passport renewal",
            }),
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
            // A fixed past day rather than a computed one: the date validator
            // refuses the future, and a literal in the past stays in the past.
            // One item, deliberately: this fixture is what the well-formedness
            // tests run over, and a fixture that also exercises fan-out makes
            // their failures ambiguous. Batching has its own tests below.
            ActionKind::RoutineComplete => json!({
                "item_ids": ["01ITEM000000000000000001"],
                "group_id": "01GROUP00000000000000001",
                "date": "2026-08-14",
                "evidence": [{ "kind": "journal", "id": "2026-08-14" }],
            }),
            ActionKind::RoutineSkip => json!({
                "item_ids": ["01ITEM000000000000000001"],
                "group_id": "01GROUP00000000000000001",
                "date": "2026-08-14",
                "reason": "Travelling.",
            }),
        }
    }

    // note.revise

    /// The model's call carries no `raw_text` — it cannot know one — and must
    /// still validate, or the proposal is refused at the moment it is made.
    #[test]
    fn a_revise_proposal_validates_without_its_resolved_body() {
        let args = validate_args(
            &NOTE_REVISE,
            &json!({ "note_id": "01NOTE000000000000000001", "find": "May", "replace": "June" }),
        )
        .expect("the model's own call must validate");
        assert!(
            args.get("raw_text").is_none(),
            "nothing should invent a body here: {args}"
        );
    }

    /// ⛔ The guard that stops an unresolved proposal from authoring an event. The
    /// two things it prevents are writing an empty body over the note and writing
    /// the anchor as if it were the body.
    #[test]
    fn building_a_revise_without_a_resolved_body_is_refused() {
        let err = build_events(
            &NOTE_REVISE,
            &json!({ "note_id": "01NOTE000000000000000001", "find": "May", "replace": "June" }),
            "d",
        )
        .expect_err("an unresolved revise must not build");
        assert!(err.contains("resolved"), "{err}");
    }

    /// ⚠️ An emptied body is a legal outcome — deleting a note's last line — and
    /// must not read as a resolution that never ran. Blank-and-optional is dropped
    /// for every model-supplied argument, which is exactly why the resolved source
    /// has its own branch in `validate_args`.
    #[test]
    fn a_resolved_body_survives_being_empty() {
        let args = validate_args(
            &NOTE_REVISE,
            &json!({
                "note_id": "01NOTE000000000000000001",
                "find": "gone",
                "raw_text": "",
            }),
        )
        .expect("an empty resolved body is valid");
        assert_eq!(args.get("raw_text").and_then(|v| v.as_str()), Some(""));
        let events = build_events(&NOTE_REVISE, &args, "d").expect("and it must build");
        assert_eq!(events[0].payload["raw_text"], json!(""));
    }

    /// ⚠️ The trap [`ActionParam::verbatim`] exists for. A trimmed `replace` can
    /// come back byte-identical to the `find` it replaces — the splice applies,
    /// the note does not change, and the card says it worked.
    #[test]
    fn a_revise_keeps_the_whitespace_that_is_the_edit() {
        let args = validate_args(
            &NOTE_REVISE,
            &json!({
                "note_id": "01NOTE000000000000000001",
                "find": "- item",
                "replace": "  - item",
            }),
        )
        .unwrap();
        assert_eq!(
            args["replace"], "  - item",
            "indentation is markdown nesting, not decoration"
        );
    }

    /// A resolved body is bounded like every other argument, and the bound sits
    /// far above any note a person or an append loop produces.
    #[test]
    fn a_long_note_still_approves() {
        let body = "x".repeat(150_000);
        let args = validate_args(
            &NOTE_REVISE,
            &json!({
                "note_id": "01NOTE000000000000000001",
                "find": "x",
                "raw_text": body,
            }),
        )
        .expect("a 150k-character note is not a mistake");
        assert_eq!(args["raw_text"].as_str().map(str::len), Some(150_000));
    }

    /// ⛔ The whole feature refuses rather than guesses, so an anchor the model
    /// left blank cannot be accepted: it would match everywhere.
    #[test]
    fn a_blank_anchor_is_refused() {
        for blank in ["", "   ", "\n"] {
            validate_args(
                &NOTE_REVISE,
                &json!({ "note_id": "01NOTE000000000000000001", "find": blank }),
            )
            .expect_err("a blank anchor must be refused");
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

    /// ⚠️ A completion asserts something happened. The user reads their own
    /// history back as fact, so the proposal has to be able to show what it read
    /// — the same reason `belief.record` carries evidence, applied to a claim
    /// about the world rather than about the user.
    #[test]
    fn a_completion_carries_evidence_and_cannot_source_it_from_the_model() {
        for action in [&ROUTINE_COMPLETE, &ROUTINE_SKIP] {
            assert_eq!(
                evidence_key(action),
                Some("evidence"),
                "{} must carry what it read",
                action.name
            );
            assert!(
                !action
                    .params
                    .iter()
                    .filter(|p| p.is_model_supplied())
                    .any(|p| p.key == "evidence"),
                "{} must not let the model name its own sources",
                action.name
            );
        }
    }

    /// ⚠️ The day comes from what was read, so a model reaching for "today" by
    /// default is the error worth catching — but the hard refusal is the future,
    /// which no record can evidence.
    #[test]
    fn a_completion_day_must_be_a_real_past_date() {
        let base = json!({ "item_ids": ["i1"], "group_id": "g1", "date": "2026-08-14" });
        validate_args(&ROUTINE_COMPLETE, &base).expect("a past day is fine");

        let today = super::super::memory::today();
        let mut same_day = base.clone();
        same_day["date"] = json!(today.format("%Y-%m-%d").to_string());
        validate_args(&ROUTINE_COMPLETE, &same_day).expect("today has happened");

        let mut tomorrow = base.clone();
        tomorrow["date"] = json!(
            (today + chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string()
        );
        let err = validate_args(&ROUTINE_COMPLETE, &tomorrow).expect_err("the future");
        assert!(err.contains("date"), "{err}");

        for bad in ["14/08/2026", "yesterday", "2026-02-30"] {
            let mut args = base.clone();
            args["date"] = json!(bad);
            assert!(
                validate_args(&ROUTINE_COMPLETE, &args).is_err(),
                "{bad} should be refused"
            );
        }
    }

    /// ⚠️ `chrono` accepts `2026-8-14` for `%Y-%m-%d`, and a completion's row id
    /// is `{item_id}-{date}-done` built by concatenation — so an unpadded day
    /// keys a different row than the same tick made in the app, and the app's
    /// undo would leave this one behind. Every date field is checked the same
    /// way so the next one does not decide for itself.
    #[test]
    fn a_day_must_be_spelled_the_way_the_app_spells_it() {
        let loose = validate_args(
            &ROUTINE_COMPLETE,
            &json!({ "item_ids": ["i1"], "group_id": "g1", "date": "2026-8-14" }),
        )
        .expect_err("chrono would take this; the row id must not");
        assert!(loose.contains("date"), "{loose}");

        let belief = validate_args(
            &BELIEF_RECORD,
            &json!({ "statement": "s", "confidence": "low", "review_after": "2026-8-14" }),
        )
        .expect_err("the same rule reaches the belief's date");
        assert!(belief.contains("review_after"), "{belief}");

        validate_args(
            &BELIEF_RECORD,
            &json!({ "statement": "s", "confidence": "low", "review_after": "2027-01-05" }),
        )
        .expect("a review date is allowed to be in the future");
    }

    /// The tick is keyed on `{item_id}-{date}`, so both halves have to reach the
    /// payload intact — a completion that silently lands on the wrong day is
    /// indistinguishable from one the user did not make.
    #[test]
    fn a_completion_lands_on_the_day_it_names() {
        let events = build_events(&ROUTINE_COMPLETE, &sample_args(&ROUTINE_COMPLETE), "d").unwrap();
        assert_eq!(events[0].payload["date"], json!("2026-08-14"));
        assert_eq!(
            events[0].payload["item_id"],
            json!("01ITEM000000000000000001")
        );
        assert_eq!(
            events[0].aggregate_id, "01ITEM000000000000000001",
            "a completion keys on the item it ticks"
        );
        assert!(
            events[0].payload["completed_at"].is_string(),
            "the projection needs a recording timestamp"
        );
    }

    /// ⚠️ The batch is a proposal-layer idea and must not survive into the log:
    /// each event has to be shaped exactly like the one the app writes when the
    /// user ticks that item by hand, or the projection folds two spellings of
    /// the same fact and the app's undo only knows one of them.
    #[test]
    fn a_batch_becomes_one_ordinary_event_per_item() {
        let events = build_events(
            &ROUTINE_COMPLETE,
            &json!({
                "item_ids": ["i1", "i2", "i3"],
                "group_id": "g1",
                "date": "2026-08-14",
            }),
            "d",
        )
        .expect("three items in one proposal");

        assert_eq!(events.len(), 3, "one event per item named");
        let ids: Vec<&str> = events
            .iter()
            .map(|e| e.payload["item_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["i1", "i2", "i3"], "in the order proposed");

        for event in &events {
            assert!(
                event.payload.get("item_ids").is_none(),
                "the plural argument must not reach the payload: {}",
                event.payload
            );
            assert_eq!(event.payload["date"], json!("2026-08-14"));
            assert_eq!(event.payload["group_id"], json!("g1"));
        }
        assert_eq!(
            events[0].aggregate_id, "i1",
            "each event still keys on its own item"
        );
        assert_eq!(events[2].aggregate_id, "i3");

        // One approval, one recording time. Per-item clock reads would order the
        // batch by iteration speed, and this column is a tiebreak.
        let stamps: Vec<&Value> = events.iter().map(|e| &e.payload["completed_at"]).collect();
        assert!(
            stamps.iter().all(|s| *s == stamps[0]),
            "a batch shares one recording timestamp, got {stamps:?}"
        );
    }

    /// The three ways a list can be wrong, each refused loudly rather than
    /// quietly repaired — a batch the model did not mean is a batch the user is
    /// asked to approve.
    #[test]
    fn a_batch_is_bounded_and_refuses_a_repeat() {
        let args = |ids: Value| json!({ "item_ids": ids, "group_id": "g1", "date": "2026-08-14" });

        let bare = validate_args(&ROUTINE_COMPLETE, &args(json!("i1")))
            .expect_err("a lone string is not a list of one");
        assert!(bare.contains("list"), "{bare}");

        let empty =
            validate_args(&ROUTINE_COMPLETE, &args(json!([]))).expect_err("nothing to approve");
        assert!(empty.contains("empty"), "{empty}");

        // ⚠️ Deduplicating would hide the confusion entirely: the completion
        // UPSERT makes the second write a no-op, so a model ticking one item
        // twice looks identical to one ticking it once.
        let repeat = validate_args(&ROUTINE_COMPLETE, &args(json!(["i1", "i2", "i1"])))
            .expect_err("the same item twice");
        assert!(repeat.contains("twice"), "{repeat}");

        let over: Vec<String> = (0..=BATCH_LIMIT).map(|n| format!("i{n}")).collect();
        let too_many =
            validate_args(&ROUTINE_COMPLETE, &args(json!(over))).expect_err("past the limit");
        assert!(too_many.contains(&BATCH_LIMIT.to_string()), "{too_many}");

        let at_limit: Vec<String> = (0..BATCH_LIMIT).map(|n| format!("i{n}")).collect();
        validate_args(&ROUTINE_COMPLETE, &args(json!(at_limit))).expect("the limit is inclusive");
    }

    /// A list element is checked by the same code a lone value is, so a rule
    /// added to one cannot quietly miss the other — and the refusal names the
    /// element, because a batch refused without saying which one is a batch the
    /// model can only fix by guessing.
    #[test]
    fn a_bad_element_is_refused_by_position() {
        let err = validate_args(
            &ROUTINE_COMPLETE,
            &json!({
                "item_ids": ["i1", "x".repeat(65)],
                "group_id": "g1",
                "date": "2026-08-14",
            }),
        )
        .expect_err("the same 64-character ceiling a lone id has");
        assert!(err.contains("item_ids[1]"), "{err}");

        let blank = validate_args(
            &ROUTINE_COMPLETE,
            &json!({ "item_ids": ["i1", "  "], "group_id": "g1", "date": "2026-08-14" }),
        )
        .expect_err("a hole in the list is not a shorter list");
        assert!(blank.contains("item_ids[1]"), "{blank}");
    }

    /// `RoutineItemSkippedPayload` skips serializing `None`, so a reason-less
    /// skip built here has to be shaped like one the app writes — a `null` would
    /// be a second spelling of the same absence.
    #[test]
    fn a_skip_without_a_reason_omits_the_key_rather_than_nulling_it() {
        let events = build_events(
            &ROUTINE_SKIP,
            &json!({ "item_ids": ["i1"], "group_id": "g1", "date": "2026-08-14" }),
            "d",
        )
        .expect("a reason is optional");
        assert!(
            events[0].payload.get("reason").is_none(),
            "got {}",
            events[0].payload
        );

        let with_reason = build_events(&ROUTINE_SKIP, &sample_args(&ROUTINE_SKIP), "d").unwrap();
        assert_eq!(with_reason[0].payload["reason"], json!("Travelling."));
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
