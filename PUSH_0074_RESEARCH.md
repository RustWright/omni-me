# [0074] research — tasks and projects, planning, people

**Answered 2026-11-18/19 in the planning session: the rulings are `RULINGS.md` 90–109, and §6
maps each question to its ruling.** Where this file and a ruling differ, the ruling wins.

Pre-read for the [0074] planning session. Written 2026-11-16, alongside the Cycle 5 close, from
his 14 answers (Appendix A, at the end). **Nothing here is a ruling.**
Sections 2–3 report evidence; section 4 is my proposal, built on it; section 6 lists what he
still has to decide.

**Epistemic labels**, used on every claim that matters:
- **[P]** read in a primary source (the paper, the spec, the official docs, or our own code).
- **[S]** from a secondary summary only (abstract, press release, review blog); direction trusted,
  numbers not.
- **[I]** my inference; no source states it.
- **[D]** measured on his data in this session.

## 0. In one page

- **Most of what was asked has evidence behind its direction; little of it has evidence behind
  its exact form.** Effects are real but small (implementation intentions d ≈ 0.27 on behaviour;
  WOOP g ≈ 0.24–0.34), and the forms we would build — an assistant-led WOOP, a time block as an
  if-then plan, a monthly ritual — are untested in exactly that shape. §2 says which is which.
- **The design's real enemy is lapse, not missing features.** The uncapped design asks for 70–110
  minutes a week and three pushes a day (4.5). The proposal caps it and makes skipping free.
- **His staleness worry is measured, not hypothetical**: 108 open hub tasks, 20 "needs you",
  most from indexes untouched since Aug–Sep (2.2). Surfacing becomes one "reconcile?" line per
  stale index, on a 4-week trial he can end (4.1).
- **Calibration lies unless built against it**: a pre-filled "how long?" makes the ratio read 1.0
  forever; small samples replay noise. No pre-fill, block times as actuals, a sample floor (4.1).
  And free log-derived actuals would calibrate *Claude's* sizing, not his (§6.1).
- **People is an entity-resolution problem first.** His hybrid ruling is the textbook
  three-zone model (Fellegi–Sunter). What keeps it trustworthy: the zone is decided by evidence,
  never model confidence; a bare first name is never confident; every answer and every merge is
  reversible for *all* attached data, not just mentions (4.3).
- **Two settled rulings collide with what he asked for**, and need his call: the rituals need
  notification tiers the check-in pattern cannot give (5.3), and approvals may need more surface
  than a read-only Today (4.4). Every calendar write asks him forever — omni-me's own definition
  makes it irreversible (5.4).
- **20 questions for the planning session** (§6), grouped by what they block. Start with §6.20,
  his read-only-calendar idea: it removes more of the planning design than any other answer.

---

## 1. What he decided (the brief)

| # | Area | Ruling |
|---|---|---|
| 1 | Tasks | Hub **and** native. Repo/`~/life` tasks arrive read-only via `.omni/tasks.toml`; omni-me owns tasks made in-app or by the assistant. Caveat: "we'll have to see how effective it is" (noise, staleness). |
| 2 | Tasks | Project tasks: **pull, plus needs-you** (`USER` size or `due`) surfacing automatically. |
| 3 | Tasks | Calibrate **his** estimates (planning fallacy). Actuals from logs where possible, asked otherwise. |
| 4 | Tasks | **Native projects too.** |
| 5 | Tasks | Today: ~40% head, 30–40% calendar, ~5% reminders, rest lost paper. |
| 6 | Tasks | Session 2's task/goal design (`research.md:353-380`): **start fresh** — an input, not a baseline. |
| 7 | Planning | Calendar: **read + approved writes now; replace it eventually.** |
| 8 | Planning | Day plan: **mixed** — blocks for deep work, untimed list for small things. |
| 9 | Planning | **Assistant-initiated rituals**: weekly plan, morning day-plan proposal, light monthly review. |
| 10 | Planning | **Goals, kept light**, above projects; gates only if evidence supports them. |
| 11 | People | Not a CRM: "the central reference for everything I know about any other person I have interacted with directly or indirectly and recorded something about it". All four uses: keeping in touch, professional network, admin counterparts, recall before talking. The hard part, his words: "both wide and robust but also focused and useful". |
| 12 | People | **Two tiers**: automatic mention index (pointers) + facts proposed with a citation, confirmed by him. Warning: misspelt-then-corrected names, shared first names, no surnames — "a veritable minefield". |
| 13 | People | **Hybrid** ambiguity: confident → attach with a "guessed" marker; ambiguous → asked *the same day* in a batched evening check-in; backfill → pointers only, never questions. Open, his: how questions are posed and how answers are used. |
| 14 | All | **Chat** for capture, questions and planning, plus **one read-only Today view**. |

Standing rulings that bind all three: approval lives with the domain, review queues are never
merged into the inbox, irreversible actions are never grantable, beliefs are ask-only, the three
notification tiers and the code-built 19:00 summary (`RULINGS.md` 2, 3, 4 and
§ Notifications); LLM tools bind to `core`, not Tauri commands (`architecture.md:170`); the hub
never writes into a repo (`TASK_HUB_CONTRACT.md`).

---

## 2. What the science says

Organised by the design question it answers, not by paper. Every row ends in the consequence for
us; where the evidence is thin, it says so rather than borrowing confidence.

### 2.1 Capture: why his current system leaks

- **External memory works, and people misjudge when to use it.** Risko & Gilbert's review
  defines cognitive offloading and finds people offload when they *believe* it is cheaper than
  remembering, and that belief is often wrong [S]. A 2022 review of intention offloading (Gilbert's
  group): external reminders "can have a large impact" on whether delayed intentions are carried
  out [S].
  → **Capture must cost less than remembering feels like it costs**, or he will keep 40% in his
  head. One chat line from the phone is the bar; anything with required fields fails it [I].
- **An unplanned open item keeps intruding; a plan, not completion, quiets it.** Masicampo &
  Baumeister (2011, JPSP): unfulfilled goals caused intrusive thoughts during an unrelated task;
  forming a specific plan for them eliminated the effect, down to control level [S, abstract].
  → A captured item is not "done being handled" until it has a **specific plan** — when, and
  ideally where — which is what the study's plans were. "Someday" and "filed under a project" are
  not plans in that sense and should not be assumed to quiet the item [I]. This is GTD's
  "clarify" step, and Heylighen & Vidal (2008) argue GTD's external-memory design is what
  cognitive science predicts [S].

### 2.2 Tasks going stale

- **[D] His indexes already show it.** 7 indexes, 108 open tasks, **20** marked `USER` or `due`.
  Five of the seven were last reconciled in Aug–Sep (a client project's index from 09-10 holds
  7 `USER` items; a life-admin index from 09-03 holds 6, including a one-off errand that is surely
  long done). My earlier "about 3 items" estimate in the elicitation was wrong by ~7x.
  → **"Needs-you" surfacing is only as good as index freshness.** Surfacing a `USER` task from
  an index untouched for 10 weeks shows a probably-stale item as urgent. The hub must weight by
  the index's freshness — its last commit time, since `updated` moves only at phase boundaries
  (4.1): past a threshold, surface *"<topic> has 6 items waiting on
  you, last reconciled 74 days ago — reconcile?"* once, not the six items [I]. That is one line
  instead of six, and it attacks the cause (an unreconciled index) rather than the symptom.

### 2.3 Estimation calibration

- **The planning fallacy is real and directional**: people underestimate their own task
  durations; outside observers err the other way (Buehler, Griffin & Ross 1994; Kahneman &
  Tversky 1979) [S]. It is not universal across settings [S].
- **Why it happens matters for us**: Roy, Christenfeld & McKenzie (2005, *Psychological
  Bulletin* 131(5)) argue the underestimate comes largely from **misremembering how long past
  tasks took** [named by reviewer 3; not read]. If so, an actual recorded at completion is worth
  more than one recalled later — support for asking at the moment of completion, not in the
  weekly review [I].
- **The fix is the outside view**: reference-class forecasting — predict from how long *similar
  past tasks* took, not from imagining this one (Kahneman & Lovallo 2003) [S]. I found no
  meta-analysis of its effect on *personal* (non-project) estimates; the evidence is mostly
  project-level [S].
- **The software precedent is Evidence-Based Scheduling** (Spolsky 2007, shipped in FogBugz):
  record estimate and actual per task, keep the ratio history ("velocity"), and forecast a
  schedule by Monte Carlo — sample past ratios, many runs, report a probability of finishing
  by each date [S — the original post seen in search fragments only]. It only works if actuals
  are logged consistently, and it prefers small tasks (≤16 h) [S].
- **Unpacking helps, sometimes.** Listing a task's subtasks lengthens estimates and reduces the
  fallacy (Kruger & Evans 2004); segmenting does the same for time allocation (Forsyth & Burt
  2008) [S]. Replications are mixed; it helps least for tasks with few components or in the far
  future [S].
  → **Design**: store `estimate` at planning time and `actual` at completion; keep a per-person
  ratio history by task kind; show the ratio *at the moment of estimating* ("your errands take
  1.6x what you guess"), and use Monte Carlo only for the weekly "does this fit?" check. Prompt
  for unpacking only on large tasks [I].
- **⚠️ A tension in ruling 3 [I].** The actuals that can be derived for free come from Claude
  session logs, i.e. **project** work — whose sizes (`XS`–`L`) are set by Claude, not by him.
  His own estimates will mostly exist on **native** tasks, which have no log. So "derive from
  logs" calibrates Claude's sizing, and calibrating *his* estimates rests on asking at
  completion. Section 6 puts this to him.
- **[D] And the logs attribute poorly today.** One 11-15 session render mentions 11 distinct task
  ids; the renders carry a start time in the filename but no per-turn timestamps I could find;
  and only 3 of the 62 commits since 11-14 name a task id. Deriving "task X took 1 h 40" needs
  either a task id on commits or a session-to-task tag at session start — a process change in
  the workspace, not just code in omni-me [I].

### 2.4 Planning: day, week, month

- **Saying when and where works, but less than first claimed.** Implementation intentions
  ("if it is 9 am Wednesday, I write the essay"): Gollwitzer & Sheeran's 2006 meta-analysis, 94
  tests, d = 0.65 [S]. Sheeran, Listrom & Gollwitzer (*European Review of Social Psychology*,
  online 27 March 2024, doi 10.1080/10463283.2024.2334563), 642 tests: effective across outcomes
  (.27 ≤ d ≤ .66, abstract) [S]; **d ≈ 0.27 for behaviour**, the outcome a time block targets;
  bias-corrected overall estimates fall to 0.35 (trim-and-fill) or 0.15 (robust Bayesian, CI
  0.08–0.22) [S, secondary summaries only — not checked against the paper]. Real but small.
  → A time block **plausibly works as an implementation intention, if a cue fires at the time**
  (a calendar reminder) [I]. That makes the blocks the best-supported part of the mixed day plan
  and the cue the part that must not be lost (4.2's Android check).
- **Time blocking as a named method has no rigorous evidence.** No peer-reviewed study isolates
  it; the popular figures (30%, 80%) trace to vendor surveys [S]. General time-management
  behaviour correlates moderately with performance and well-being (Aeon et al. 2021, PLOS ONE,
  158 studies — seen only secondhand) [S].
- **Workload limits are the product precedent for the planning fallacy.** Sunsama's daily ritual:
  pull tasks in, estimate each, place on the calendar, and warn when the day is over-committed;
  plus a shutdown ritual at day's end [S, vendor docs]. That is a **capacity check on inside-view
  estimates**, not the outside view; it becomes reference-class forecasting only once estimates
  are ratio-corrected, as 4.1 proposes [I].
- **Rituals land better on landmarks.** The fresh-start effect: aspirational behaviour (goal
  commitments, gym visits) spikes at the start of a week, month or year (Dai, Milkman & Riis
  2014, Management Science) [P]. Placebo terms ("laundry", "gardening") showed no reliable rise
  [P]. **The month boundary is the weakest landmark measured**: diet searches +3.7% at month
  start vs +14.4% at week start and +82.1% at year start; gym visits +14.4% month vs +33.4% week
  [P].
  → Put the weightier ritual (the weekly plan) on the week boundary; the monthly review on the 1st
  is a convention with weak support from this effect, and framing it goal-level rather than
  chore-level follows the placebo result [I].
- **Batch the prompts.** RCT (Fitz et al. 2019): notifications batched 3x/day → more attentive,
  less stressed, more in control than as-they-come; *no* notifications → more anxious [S].
  → Supports the morning + evening rhythm and a hard cap on ad-hoc pushes. Notification volume is
  the real budget across all three domains; ntfy alerts already spend some of it [I].
- **Auto-schedulers vs. approval.** Motion's model is tasks + constraints + availability →
  schedule, re-run on every change, deadline-first, with a warning when something cannot fit [S,
  vendor help pages via search]. Re-running on every change collides with our approval rule (every write
  approved) — a replan cannot silently move his calendar [I]. Take the *fit check and warning*,
  not the silent rescheduling.

### 2.5 Goals

- **Specific, hard goals beat vague ones — except on complex, unfamiliar work**, where a specific
  *performance* goal can hurt and a *learning* goal is better (Locke & Latham) [S].
  → No SMART gate. A forced performance target on a learning goal like "learn RL" is the case the
  theory warns about [I]; an outcome goal like "get into a masters programme" is not, and could
  take a specific target if he wants one. The theory never tested a gate, so "kept light" rests
  on his ruling, not on this finding.
- **Mental contrasting + implementation intentions (WOOP)**: g = 0.34 across 21 studies,
  N ≈ 15,900 — but ~87% of that N is two online MOOC studies (Kizilcec & Cohen), which the
  moderator analysis excludes; trim-and-fill gives g = 0.24 (Wang, Wang & Gai 2021, *Frontiers in
  Psychology*) [P, full text]. The best-performing arm was **face to face with a human
  experimenter** (g = 0.47, k = 16) versus written/online material (g = 0.28, k = 6) [P].
  → An assistant conversation is **neither arm and is untested**; it resembles the human arm more
  than a form does, which is a reason to try it, not evidence that it works [I]. Use it only on a
  stalled goal, so it is not a weekly chore.
- **Goal conflict hurts, but the measurement is contested.** Emmons & King (1988): conflicting
  strivings → less action, more rumination [S]; later work disputes the bipolar measure [S]. **I
  found no evidence for any specific cap** (Session 2's 5). A cap is a design choice, not a
  finding.

### 2.6 People

- **Relationships decay with contact, friends faster than family.** Roberts & Dunbar (2011),
  18-month longitudinal, 25 students: friendship closeness fell with lower contact frequency much
  more than kin closeness did [S]. Small sample, one life transition.
  → Keeping-in-touch nudges should be **per person and opt-in by tie**, and weighted to friends;
  a uniform "you haven't contacted X in 30 days" for everyone is noise [I]. Monica's founder
  raises the same problem: "human relationships don't naturally follow schedules" [S].
- **People underrate how welcome reaching out is.** Liu, Rim, Min & Min (JPSP 2023, 13
  experiments, 5,900+ people): initiators underestimate recipients' appreciation, more so the
  more surprising the contact [S]. → A nudge to reach out has a real payoff, so it is worth a
  slot in the notification budget [I].
- **Personal knowledge graphs name his problem exactly.** Balog & Kenter (ICTIR 2019): a personal
  graph holds entities too minor for any public graph; its open problems are **linking** mentions
  to entities, **NIL detection** (is this a *new* person?), and **population** — and these are
  intertwined: the first mention of an entity should trigger its creation [S]. His misspelling
  and same-first-name cases are NIL detection and linking.
- **Record linkage gives the three-zone rule his hybrid already is.** Fellegi–Sunter: each field
  that agrees or disagrees adds a log-weight (`log2(m/u)`, where m = how often true matches agree,
  u = how often random pairs agree by chance); weights add; two thresholds split pairs into
  **match / possible match (clerical review) / non-match** (Fellegi & Sunter 1969, "A Theory for
  Record Linkage", *JASA* 64(328):1183–1210) [S, via Splink's explainer]. His ruling 13 is this
  model: confident → link with marker, ambiguous → review (the evening batch), weak → new or
  unlinked.
  → It also explains *why* first names are weak evidence: a common first name has a high u, so it
  contributes little weight; context fields (same thread, same place, same employer) carry the
  match [I, from the model].
- **LLMs are good at this when asked to select, not to judge pairs.** OpenSanctions Pairs (2026,
  755k labelled pairs): F1 99.0% (GPT-4o) and 98.2% (an open model) on structured sanctions
  records; the authors *suggest* attention shift to blocking, clustering and review because
  pairwise accuracy is nearing a ceiling [P, abstract]. Those records carry dates of birth and
  nationalities; a bare "Sarah" in a journal line is a far poorer input, so expect much lower
  accuracy on his data [I]. COLING 2025 ("Match, Compare, or Select?", Wang et al.): letting the
  model see the candidates together and *select* beats pairwise matching [P, abstract].
  → Resolution prompt shape: "this mention + context; candidates A, B, C; or a new person; or
  can't tell" — never "is X the same as Y?" [I].
- **Ask the question that splits the candidates best.** Clarification research frames question
  choice as information gain over the candidate set (ICLR 2025); an enterprise assistant
  (ECLAIR) shows the user the competing entities as a picker [S].
  → **Evening batch questions are pickers with a one-line context snippet per candidate**,
  always offering "new person" and "don't know" [I]. This is a *proposal* for his open question
  (how questions are posed); 4.3 proposes how answers are used; §6 puts both to him.
- **Not everyone needs the same attention: Dunbar's layers.** Personal networks nest in layers
  of roughly 5 / 15 / 50 / 150, each ~3x the last, with contact frequency and closeness falling
  outward; phone-call data (MacCarron, Kaski & Dunbar 2016) fits the inner and outer layers well,
  the middle ones loosely (*Social Networks* 47:151–155) [S]. → A **tie layer** on each person is
  one lever for "focused": keeping-in-touch nudges and cadence only for the inner layers he opts
  in [I].
- **But weak ties are where professional value often sits.** Granovetter (1973, "The Strength of
  Weak Ties") and Levin, Walter & Murnighan (2011, "Dormant ties: the value of reconnecting",
  *Organization Science* 22(4)) argue outer and dormant ties carry novel information and
  opportunity [named by reviewer 3; not read this session]. → Outer layers are not
  "reference only" for the **professional-network** purpose: reconnect prompts there are
  goal-triggered (a masters application needs a referee), not cadence-triggered [I].

### 2.7 The one finding that cuts across all three

- **Personal tracking lapses; design for the lapse.** Epstein et al.'s lived-informatics model
  (UbiComp 2015; surveys of ~100 trackers each of activity, finance and location, plus 22
  interviews) adds *lapsing and resuming* as normal stages of use, not failures [S].
  → He will stop using the planner for a fortnight at some point. Returning must be cheap: no
  wall of overdue items, no punishment streaks. On return, offer one triage ("14 items went past
  their dates — keep, reschedule or drop?") rather than replaying every missed morning [I].
  The same holds for the people inbox: unanswered evening questions must expire into
  "unlinked", not pile up — once his memory of the day fades, the question is worthless anyway
  (his own argument in ruling 13) [I].

### 2.8 Named but not read — read these before the build, not before planning

Reviewer 3 named work this pass did not consult; each is real and on-topic, none was opened:
- **Bellotti et al. (2004)**, "What a To-Do: Studies of Task Management Towards the Design of a
  Personal Task List Manager", CHI — direct empirical work on how to-do lists fail.
- **Roy, Christenfeld & McKenzie (2005)**, *Psychological Bulletin* 131(5) — the memory account
  of the planning fallacy (used in 2.3).
- **Payne (1993)**, "Understanding calendar use", *Human–Computer Interaction* 8(2) — calendars
  as prospective-memory cues; bears on whether a block gives a cue.
- **Whittaker, Jones & Terveen (2002)**, "Contact management: identifying contacts to support
  long-term communication", CSCW — the closest prior work to the people design.
- **Granovetter (1973); Levin, Walter & Murnighan (2011)** — weak and dormant ties (2.6).

---

## 3. Frameworks: conceptual and code

### 3.1 Conceptual frameworks worth adopting (and what to take from each)

| Framework | Take | Leave |
|---|---|---|
| **GTD** (Allen; Heylighen & Vidal 2008) | Capture everything cheaply; *clarify* each item into a next decision; the **weekly review** as the system's heartbeat. | Contexts (@phone, @home) — the assistant can infer them; the full taxonomy. |
| **PARA** (Forte) | The **area vs project** distinction: projects end, areas (finance, health, people) never do. Gives native tasks with no project a home. | Resources/Archives — omni-me's archive and notes already cover them. No research basis; a filing convention [I]. |
| **Evidence-Based Scheduling** | Estimate/actual pairs, ratio history, Monte Carlo fit check. | Team mechanics, hours-to-calendar conversion. |
| **Implementation intentions / WOOP** | Blocks that plausibly work as if-then plans when a cue fires [I]; WOOP for stalled goals, assistant-delivered (untested mode, 2.5). | A WOOP form on every goal. |
| **Fresh-start effect** | The weekly ritual on the week boundary; the month is the weakest landmark (2.4). | Treating the 1st as special. |
| **Fellegi–Sunter** | The three-zone decision; field weights; "first name is weak evidence". | Estimating m/u by EM — our data is too small; the LLM selects instead [I]. |
| **Personal knowledge graph** (Balog & Kenter) | Mentions, NIL detection, population as one loop. | — |
| **Dunbar's layers** | A tie layer per person; cadence and nudges only for inner layers. | Treating 150 as a cap. |
| **Taskwarrior urgency** | A transparent, tunable score: a sum of coefficients (due 12.0, blocking 8.0, age 2.0, blocked −5.0 by default) [S, taskwarrior.org via search]. A model for needs-you ranking and staleness that he can read and adjust. | Its CLI and sync. |
| **Lived informatics** | Lapse and resume as first-class states. | — |

### 3.2 Standards and code

- **iCalendar tasks**: `VTODO` (RFC 5545) has `DUE`, `PERCENT-COMPLETE`, `STATUS` [S, CalConnect
  guide]; RFC 9253 adds typed relations (`RELATED-TO` with new types, `LINK`, `CONCEPT`) for
  parent/child and dependency [S, RFC abstract]; RFC 10051 "Task Extensions to iCalendar" (Apthorp, Douglass;
  Proposed Standard, October 2026; updates RFC 5545) adds `ESTIMATED-DURATION`, `REASON`,
  `SUBSTATE`, `TASK-MODE`, a `VSTATUS` component, statuses `PENDING`/`FAILED`, and lets
  `DURATION` stand without `DTSTART` [P, datatracker, via reviewer 3]. → Shape our task fields so they **export** to VTODO losslessly; that keeps the
  "replace the calendar eventually" path open without adopting CalDAV now [I]. The Rust
  `icalendar` crate has a `Todo` component (docs.rs 0.17.x) [S, search result].
- **vCard** (RFC 6350) for importing phone contacts as person seeds, and exporting [I —
  standard well known, not re-read this session].
- **Google Calendar API**:
  - Incremental sync with `syncToken`; a `410 Gone` means discard and full-resync [P, Google
    sync guide]. Cancelled events come back in incremental sync, or in a full sync with
    `showDeleted=true` [P, events.list reference]. With a `syncToken`, the parameters `timeMin`,
    `timeMax`, `q`, `orderBy`, `updatedMin`, `iCalUID` and the extended-property filters are
    refused with a 400; changing other parameters between syncs is undefined behaviour [P].
  - ⚠️ **An incremental sync cannot be windowed by date** (`timeMin`/`timeMax` are refused with a
    token), so it always covers the whole calendar — a planner wanting "the next 14 days" filters
    locally after syncing everything [P]. Push `watch` channels say *that* something changed, not
    what — they trigger a sync [S].
  - Scopes: narrowest that works. `calendar.events` = read/write all events;
    `calendar.freebusy` = busy/free only, no titles; `calendar.app.created` = full control of
    calendars the app creates [P, Google auth page]; `calendar.events.owned.readonly` also exists,
    narrower still [P]. → **Candidate split**: read with `calendar.events.readonly` (titles
    matter for planning), write only into an
    **omni-me-owned secondary calendar** via `calendar.app.created`. Approved blocks then can
    never touch his real events; deleting that one calendar is the complete rollback [I]. This is
    "define the clean separation" applied to the calendar.
  - ⛔ **OAuth trap**: a consent screen left in *Testing* issues refresh tokens that **expire
    after 7 days** whenever any scope beyond name, email and profile is requested [P, Google
    Cloud support page]. A personal app must be switched to *In
    production* (unverified is fine for personal use, with a warning screen) [S]. Silent weekly
    breakage otherwise.
  - Sensitive/restricted classification of each scope: not confirmed; check before setup [gap].
- **Alternatives evaluated for calendar reads**:
  - *Secret iCal address* — read-only ICS URL per calendar, no OAuth, revocable by reset [S,
    Google help via search]. Freshness of the export itself is unknown; the 8–24 h figures found are for
    Google *subscribing to* outside feeds, a different path [S]. Cheapest read path; no writes.
  - *Android `CalendarContract`* — reads every calendar synced to the phone with a runtime
    permission, no OAuth [S, Android docs via search]; needs a custom Tauri Kotlin plugin (none found), and
    only works where the phone app runs — not on the box where rituals would run [I].
  - → **Recommendation [I]**: Google API with the split scopes above. The secret ICS is the
    fallback if OAuth setup proves heavy; CalendarContract loses because the agent is on the box.
- **Auto-scheduling products** (Motion, Reclaim): commercial, closed; useful as behaviour
  references only (2.4).
- **Task software — integrate or build?** Candidates: **Taskwarrior** (CLI, local data, urgency
  model above) and **Vikunja** (self-hosted to-do server that speaks CalDAV) [I — not re-read
  this session]. Either would hold tasks in a store the assistant, the archive and the ledger
  cannot see: a bill task could not cite the email it came from, and a task could not link to a
  person. The same objection as Monica below. → **Build in omni-me; borrow Taskwarrior's urgency
  model and VTODO's field set.** Alternative evaluated: Vikunja as the task store with omni-me as
  a client — rejected on cross-domain linking, which is the reason the push exists [I].
- **The "replace the calendar" end state** needs, beyond plans: his own events as records (not a
  cache), invites from other people, and phone alarms/reminders. A self-hosted **CalDAV** server
  (e.g. Radicale, Python) is the standard way phones sync a calendar that is not Google's [I —
  not re-read]. Not for this push; the data model should just not close it off (4.2).
- **Personal CRM software — Monica** (open source since 2017, AGPL, PHP/Laravel; v3 rewrite due
  before end of 2026 with an MCP server) [S]. Model: contacts, relationships between contacts,
  activities, reminders (birthdays, follow-ups, promises), notes, vCard import/export [S].
  - *Integrate vs build* (per the integration-first rule): adopting Monica means a second service
    holding people data that **cannot see** the email archive, journal, notes or transactions —
    and the mention index over those is ruling 12's whole first tier. Its value is its model, not
    its runtime. → **Build in omni-me, borrow Monica's model** (activities, reminders,
    relationship edges). Alternative evaluated: run Monica and sync into it — rejected on the
    mention tier [I].
- **Entity-resolution libraries — Splink** (Python, Fellegi–Sunter, EM-estimated weights,
  built for millions of records) [S, Splink docs via search]. Wrong scale and runtime for a few hundred people, and a
  Python dependency in a Rust stack [I]. → Adopt the **model** (zones, weights), not the
  library. Alternatives `dedupe`, `Zingg`: same objection, not investigated further.

---

## 4. Proposed shape (my proposal, for the planning session to accept or cut)

Everything in this section is **[I]** unless it cites a [P] or [S] finding above. None of it is
his decision until he says so; the choices that most need him are in §6.

### 4.1 Tasks, projects, goals

**Records and writers**
- **Three origins, one writer each**: *native* (omni-me owns: task, project, goal), *hub* (a
  read-only mirror of `.omni/tasks.toml`, keyed on the contract's `id`), and *capture* (hub-owned,
  per the contract). **Areas** (PARA) are a candidate fourth native kind, not ruled (§6).
- **Capture vs native task, one rule**: a chat line that **names** a repo or `~/life` project as
  its target ("for omni-me: …") is a *capture* — named explicitly, never inferred by the model,
  or two runs classify differently and the duplication returns. Otherwise it is native. A capture
  follows the contract's mechanism: a session later drains it into the index with
  `origin = "omni:<id>"`, and the hub reconciles. Without this
  rule a repo idea can be stored both ways and comes back duplicated when drained.
- **Hub terminal states**: `done` drops from every surfacing view and stays queryable; `cut` and
  `shelved` stay visible on the project with their note, because they answer "why isn't this
  happening" (`TASK_HUB_CONTRACT.md:88-90`).
- ⛔ **Hub ids can be reused under the contract as written** [P]: the next id is "the highest
  existing id in the file", and `done` entries "may be pruned" (`TASK_HUB_CONTRACT.md:86-90`).
  Prune the highest done id and the next session mints it again — two unrelated tasks, one
  history in the hub. Fix in two places: the contract gains a `next_id` high-water mark in the
  header (a workspace change, §6); the hub refuses an id it has seen before under a different
  title hash, and reads a **disappeared** id as "unknown", never as "done".
- **Index staleness from commit time, not `updated`** [P]: `updated` is bumped only at phase
  boundaries (`TASK_HUB_CONTRACT.md:128-130`), so it under-reports activity on a project being
  worked. Whatever carries the index to the server (§6.14) should carry its last commit time too.

**Capture**
- **One chat line.** The assistant proposes the structured task (title, maybe a date, maybe a
  project); approval lives with the task domain. If `task.create` later earns a grant, capture
  becomes zero-tap (`task.create` is reversible: it never leaves the log).
- **Offline**: capture is chat, and chat answers come from the agent on the box over the tailnet.
  Placement puts "must work offline" on the device (`project.md:417`). Proposal: a capture typed
  offline is stored on the device as a plain pending line and becomes a proposal on reconnect;
  nothing typed is lost.
- **Every item gets a next decision** (date, project, or someday) at the first ritual after
  capture, not at capture time, to keep capture cheap [I]. Only a *dated* decision resembles the
  specific plans in Masicampo & Baumeister (2.1); until that ritual, the item may still intrude.

**Dates, recurrence, obligations**
- **Deadlines only when external** (bill, form, `due` in an index); everything else carries a
  *scheduled* date and takes its urgency from its goal. Session 2 had the same split; it survives
  start-fresh as my judgement, not as evidence — no finding in §2 tests it [I].
- **Recurring tasks** for obligations (rent monthly, insurance yearly): not routines, which are
  habits capped at 2–31 days (`invariants.md:137-144`). Store an RFC 5545 `RRULE`; each
  occurrence is its own task when it comes due, so slipping one does not slip the series.
- **Penalty-dated bills** (moved into this push, `project.md:373`): an *obligation* is a task with
  an amount, a due date and a penalty date. Three ways one can come to exist, each [I]: (a)
  **recurring detection** already finds repeating charges — when a detected series has a known
  biller, propose an obligation series; (b) **email extraction** — a bill email with a due date
  proposes a one-off obligation, citing the email; (c) **chat**. The existing Critical alert
  "money due with a penalty date (the day before)" (`project.md:345-346`) then reads obligations.
  Which of (a)/(b) runs first, and on what host (intake → server), is §6.

**Staleness and the "see how effective it is" test (his caveat to ruling 1)**
- A native task untouched for **N weeks** is offered for drop in the weekly review; a hub index
  older than **M days** surfaces as one "reconcile?" line instead of its items (2.2).
- **Per-index mute**: a project whose index he never wants surfaced can be muted in one line.
- **Measured, so the caveat is testable**: for every surfaced hub item, record whether he acted,
  pulled it in, or dismissed it. After a fixed trial (proposal: 4 weeks), the weekly review shows
  the ratio and asks whether hub surfacing stays. Thresholds N and M are his (§6).
- **Ranking** needs-you items by a Taskwarrior-style coefficient sum (due, age, blocking, index
  freshness) that he can read and tune, not a model score.

**Estimates and actuals** — designed against the ways calibration lies
- Estimate optional at creation, prompted at planning.
- **Actual, never anchored**: at completion, buckets (15 m / 30 m / 1 h / 2 h / longer) with
  **no pre-fill** — showing his estimate makes "confirm" the cheapest tap and the ratio reads 1.0
  forever. Where a block existed, the block's real start and end are the actual, no tap needed.
- **Censoring**: dropped, split and abandoned tasks are disproportionately the overruns. They
  count as "took longer than estimated" rather than vanishing from the sample.
- **Sample floor**: pool all task kinds until a kind has ~20 samples; show **no ratio and no
  forecast** below the floor — "not enough data yet" — rather than replaying six data points
  through a Monte Carlo. At his volume the floor is weeks away, and that is the honest answer.
- **Goodhart**: once he sees "your errands take 1.6x", he may pad. Show the ratio when planning,
  never as a score, and track it over time so padding shows as a drift toward 1.0.
- **Log-derived actuals are conditional** on §6.1: today they would calibrate Claude's sizing,
  not his (2.3).

**Session 2's design, element by element** (start-fresh means weighing each, not dropping all)

| Session 2 element | Verdict | Why |
|---|---|---|
| Goal = project, any-depth tree | **Change**: goals above projects, one level | His ruling 10 separates them. Depth invites taxonomy upkeep, and upkeep burden is among the lapse reasons Epstein et al. report [S]; that depth specifically causes lapse is [I]. |
| No task deadlines | **Keep, amended**: external deadlines allowed | Bills (ruling, `project.md:373`). |
| Scheduled date, explicit deferral only | **Keep** | Matches "never silent roll-forward" (4.2). |
| Two-part deferral justification | **Drop as a gate; keep as an optional prompt** | Friction at the moment of slipping is a plausible lapse trigger [I — no source names a trigger]; the *pattern* is still worth asking about in the weekly review. |
| Opportunistic completion (`task_advanced`) | **Keep** | Free; it is just completing early. |
| Focus mode / `task_focused` | **Defer** | No evidence either way; not in his answers. |
| 5-active-goal cap with 24 h cooling-off | **Drop the number, keep a soft warning** | No evidence for any specific cap (2.5). |
| SMART on activation | **Drop** | Specific performance goals may underperform on novel, complex work (Locke & Latham, 2.5) [S]; nobody tested SMART *gating*, so dropping it is [I]. |

**Export**: tasks export as VTODO, people as vCard — the portability stance already held for notes
(`invariants.md:59-62`).

### 4.2 Planning

- ⚠️ **A simpler alternative is open** (§6.20, his idea after this draft): never write to the
  calendar at all; read it in, add everything new in omni-me. If adopted, the write-related
  bullets below (outlet, idempotent writes, his-edits-win, the secondary-calendar check) fall away.
- **Plans live in omni-me** (ruling 7's long-term goal): a *plan* record per day and per week,
  holding blocks and the day list. The calendar is an **outlet** that approved blocks are written
  to, in an omni-me-owned secondary calendar (3.2).
- **Fit check counts routines**: free time = calendar free time **minus routine minutes** for that
  day (Session 2 framed routines as a time budget, `research.md:385`) minus the ratio-adjusted
  estimates of the planned work.
- **Morning**: one proposal — blocks for deep work, a short list, anything due, with the fit check
  ("this is ~7 h of work in a 5 h day"). Every calendar write asks (5.4); the day's blocks go
  in one batch approval. **Skipping it costs nothing**: Today then shows calendar and due items,
  with no "plan not set" banner to feel guilty about.
- **A newer plan supersedes an older proposal** [I]: proposals have no expiry today. If the agent
  re-plans at 11:00 after a meeting moves while he approves the 08:00 version on the phone, both
  sets of blocks get written. Approval checks the plan version, the way `note.revise` resolves at
  approval, and refuses a superseded one.
- **Idempotent calendar writes** [P]: `events.insert` accepts a caller-chosen id (base32hex,
  5–1024 chars, unique per calendar), which Google documents as the way to avoid duplicates when a
  call fails after succeeding. Derive it from plan id + block. Caveat [P]: Google does not
  guarantee to detect an id conflict at insert time, so a retry looks the id up before re-inserting.
- **His edits in Google Calendar win** [I]: 30–40% of his system already lives there, so he
  will drag blocks. A moved block updates the plan record (and becomes the actual); a block
  deleted outside omni-me marks the plan item dropped; a `410` is a full resync that never
  deletes plan records. Token expiry maps to the existing Critical "source needs reauth" alert,
  and an approved write that failed shows in Today — never a silent gap.
- **Unverified**: whether an API-created secondary calendar shows on Android with reminders by
  default. If not, a block gives no cue, and the cue *is* the implementation-intention benefit
  (2.4). Check before relying on the secondary calendar (§6).
- **Weekly** (week boundary, 2.4): last week (done, slipped, dropped), needs-you and stale-index
  lines, the week's blocks, Monte Carlo fit over the ratio history.
- **Monthly** (the 1st): goals only; a WOOP conversation for any goal with no progress that month.
- **Evening**: the shutdown — done, roll or drop the rest (never silent roll-forward) — and the
  people questions (4.3). How it reaches him is a tier question (5.3, §6).
- **Prerequisite**: `CheckInPolicy` generalised into named scheduled prompts, each with its own
  tier, due-check and housekeeping (5.3).
- **Lapse, per ritual**: a ritual skipped 5 times running pauses itself and says so once, in the
  next weekly review ("morning plan paused — resume?"). Skipping mornings never pauses evenings.
  A full lapse (every ritual unanswered for N days) becomes one "welcome back" triage.
- **Weekly review hard-capped at 10 decisions**; the rest roll to next week. Uncapped, it holds
  eight sections and runs an estimated 25–40 minutes (4.5); that this length gets skipped is a
  judgement, not a finding [I].
- **Toward replacing the calendar** (ruling 7), later milestones the model must not close off:
  his own events as records, invites from others, phone reminders/alarms, and a CalDAV endpoint
  (3.2). Plans-as-records is milestone one.

### 4.3 People

**Records**
- `person` — opaque id; names are **aliases**, never keys (the model-derived-keys lesson applies
  to names). A **tie layer** (inner / middle / outer — a simplification of Dunbar's four layers,
  2.6 [I]) and optional **roles**
  (landlord, accountant, adviser) with a validity period.
- `mention` — pointer: source record, span, surface name, context snippet, resolution state.
- `fact` — claim + citation + state (proposed / confirmed / rejected), shaped like a belief
  (confidence, `review_after`, retired not deleted) with the document fields' source rank (5.5).
- **His own notes** about a person, always — the third input of ruling 12, no confirmation step.
- Relationship edges between people.

**Facts are proposed and confirmed, never recorded on inference** — ruling 12 as he gave it. (Not
"ask-only": for beliefs that means the assistant does not volunteer at all, which is the
opposite of ruling 12's proposals.)

**The four purposes (ruling 11), each with its smallest useful form**
- **Keeping in touch**: per-person cadence, opt-in, inner layers only; nudges ride a ritual,
  never an ad-hoc push (2.4, 2.6). Birthdays and dates he records become reminders.
- **Professional network**: how met, by whom introduced (an edge), what they can help with
  (a fact), and links to projects and goals — "who could write a masters reference?" is a
  query across people × goals.
- **Admin counterparts**: roles with periods ("landlord, 2025–") so "my landlord" resolves to
  whoever held the role **on the date of the mention** — today's mention means today's holder;
  every email from them cites into their record.
- **Recall before talking**: a brief on request — confirmed facts, last 3 interactions, open
  promises either way — and optionally, from the calendar, before an event with that person.

**Resolution pipeline** per new mention, the whole of it on the **agent host** (the server folds
no journal, so blocking there would silently lack journal context, 5.2):
*block* (candidates by alias, **sender address**, edit distance, shared thread or context) →
*select* (LLM picks a candidate, "new", or "can't tell", with reasons — 2.6) → *zone*.
- ⛔ **The zone is decided by deterministic evidence, not the model's confidence** [P, memory
  `project-model-derived-keys-are-nondeterministic`: the same email came back wrong at *higher*
  confidence]. Confident = a sender address he has confirmed (with sender authentication
  passing), or an exact confirmed **full-name** alias. The model may only choose *within* that;
  anything else is ambiguous.
- ⛔ **A bare first name is never confident — not even when only one known person carries it.**
  "The only Sarah I know of" is exactly how a *new* Sarah gets merged into an old one: NIL
  detection (2.6) is his shared-first-name case, and a single candidate says nothing about it.
- **A resolution is recorded once**, as an event. A later run (model change, reprocess) may only
  *propose* a different link, never overwrite one — or Monday's work-Sarah becomes Tuesday's
  sister-Sarah and the facts follow.
- Ambiguous → queued for the evening; backfill → unlinked pointer built from the parsed `From`
  alone, **no model call** (capacity, 5.1).
- **Sender address is an alias kind**: once he confirms `rchen@…` is Robert ("Bobby" in the
  journal), later mail from it links deterministically when sender authentication passes
  (`auto_import::sender_auth` already reads it). A printed, mechanical value — the kind it is
  safe to key on.
- **Source labels everywhere**: a picker never shows a raw display name or email body without
  "from an email" on it (`"Your sister Sarah – urgent"` is written to be tapped); a fact from
  email carries "unauthenticated source" until he confirms it.
- **"New person" is provisional**: hidden, with no facts proposed against it, until a second
  mention or his confirmation — otherwise a misspelling ("Shara") becomes a ghost record.
- **Roles resolve at the mention's date**: a March journal line about "the landlord" stays with
  March's landlord after a move. When a role-holder's sender address changes, propose
  "role ended?".
- **One writer for identity**: events apply in arrival order, which differs per device [P,
  `config_projection.rs:88-95`]. An offline phone answering "it's work-Sarah" while the agent
  merges two Sarahs would leave phone and agent with different records. The phone emits *answer*
  events only; the agent alone applies links, merges and splits.
- **Proposed narrowing of ruling 13** (§6 asks, it is not decided): an email's own *claims* (its
  display name, signature, body) never reach the confident zone, because mail text is
  attacker-controlled (5.5). A sender address *he* confirmed is his evidence, not the email's,
  so it still links. Cost: more email
  mentions land in the evening queue or stay unlinked.

**How his answers are used** (a proposal for his open question): every answer is an event that
becomes evidence —
- "it's work-Sarah" → link, and the context becomes a distinguishing feature next time;
- "new person" → creates the person, with this mention as the first;
- "not the same as X" → a stored **cannot-link** constraint, never proposed again;
- "the old name was a misspelling" → an alias with `misspelling` provenance, still resolvable;
- "don't know" → unlinked, never re-asked.
- **Every answer is reversible and visible**: the person view lists the decisions behind it
  (including cannot-links), each undoable. Unplaced mentions sit in a **pull-only** list he can
  open; it is never pushed.

**Volume control**: a cap on questions per evening (proposal: 5, most-informative first); the
rest wait one more day, then expire to unlinked — a question about a fading memory is worthless
(his reasoning, ruling 13).

**Merges and splits are events, reversible — for everything, not just mentions.** A wrong merge
picks up facts, notes, edges, roles, tie layer, cadence and cannot-links before he notices. If
`PersonUnmerged` replays only mentions, the rest stays on the wrong side: the role C lesson
exactly [P, memory `project-role-c-split-never-recrossed-callers`: an operation designed along
one axis silently decides every other property]. So every person-owned record carries the id it
was attached under; an unmerge proposal lists each attached item and asks him to assign the
ambiguous ones; and a test enumerates every person-referencing table against the unmerge. Nothing
in the codebase does this yet (5.5); it must exist from the first person event.

**Wide in storage, focused in presentation** (his hard part): the person view leads with
confirmed facts and the last interaction; mentions sit behind "also appears in 14 emails,
3 journal entries".

### 4.4 Today view (ruling 14)

Read-only: today's blocks, the day list, anything due, and the count of pending people questions.
No editing, so there is one write path (chat).
- **Before (or without) a morning plan**: calendar events and anything due. No "plan not set".
- **An approved calendar write that failed** shows here until resolved.
- **During a lapse**: the one welcome-back line, not a backlog.
- **⚠️ Tension with ruling 14 (§6)**: approving the morning plan and answering the people pickers
  are review flows, and "the review UI is the whole value … a generic card cannot carry it"
  (`project.md:229-232`). Read-only Today plus chat may not be enough surface for them.

### 4.5 What it asks of him — the budget that decides whether he keeps using it

The adversarial review's estimate of the *uncapped* design [I]: morning 2–4 min, evening shutdown
plus pickers 4–7 min, weekly 25–40 min, monthly ~15 min — **70–110 minutes a week, and three
pushes a day before any finance alert**. Lapsing is normal (2.7), and upkeep burden is among its
reported reasons [S]; no source gives a minutes threshold, so the budget below is a judgement
call [I].

Proposed budget, to be ruled (§6), and measured the way the Critical alert budget already is —
measured, never enforced, with the weekly review asking what to demote when over:
- **≤ 30 min a week** of ritual time in total; weekly review ≤ 10 decisions.
- **≤ 2 planning/people pushes a day**, on top of the existing alert tiers.
- Every ritual skippable at zero cost; skipping is data, not failure.

**What a 6-month post-mortem would most likely say** (reviewer 2's, kept because it names the
failure the design must be built against): not one bug but trust eroding on two fronts. The
planner asked for ~90 minutes a week and three pings a day; calibration said he was "well
calibrated" because actuals were pre-filled, so the fit check never caught an overloaded day and
plans stopped being believed; the weekly review ran long, got skipped, and a lapse met a backlog
of hub items from indexes that looked stale but were not. One wrong Sarah merge that the unmerge
could not separate taught him person records could not be trusted, and email contacts never
linked, so the central reference stayed mostly empty. He went back to his head and Google
Calendar — which the plan record had been quietly disagreeing with since the first block he
dragged. Every mitigation in 4.1–4.4 marked "against the ways calibration lies", "deterministic
evidence", "for everything, not just mentions", "his edits win" and the caps above answers one
clause of that paragraph.

---

## 5. Codebase fit

From a read-only mapping pass over the code (file:line citations are the mapping agent's,
spot-checked where marked). [P] = read in code.

### 5.1 What each domain costs to add

- **New event types** [P]: enum + Display + FromStr + `ALL` + `authoring_features` + payload +
  `validate_payload` (`core/src/events/types.rs:10,106,170,242,350,1851`) + the hand-counted test
  (`:2093`). FromStr and projection `apply` arms fail **silently** (catch-all `other =>`,
  `_ => Ok(())`); the roundtrip test catches FromStr only once the variant is in `ALL`.
- **One append boundary, not one builder** [P]: `EventWriter` (`core/src/events/writer.rs:36-118`)
  is the only local append; builders are per-type `NewEvent::*` factories
  (`core/src/events/store.rs:86-180`). → Add `NewEvent::task_created`-style factories so chat,
  app and intake share one builder — the drift-between-paths lesson from the sync-stranding bug.
- **Record types cannot carry these domains** [P]: `core/src/record_type.rs` is date-keyed,
  text-only frontmatter with no relations, status, numbers or dates; only the journal uses it.
  The catalog says so (`catalog.rs:8-21`). → Tasks, goals, people are **code**, not configuration.
- **Projections** [P]: 11 today; a new one goes in `all_projections`, `ALL_PROJECTIONS`,
  `owned_projections` under a feature, and the catalog or `NOT_QUERYABLE`
  (`core/src/events/registry.rs:38-50,72,111`; `catalog.rs:30-32`).
- **Assistant actions** [P]: discovered only through `describe_type`, which matches the
  `{type}.` prefix (`actions.rs:1256-1262`, `verbs.rs:377`). `task.create` needs a catalog entry
  named exactly `task`, `person.*` an entry named `person`. Autonomy has one variant
  (`AlwaysAsk`) and grants refuse irreversible actions (`promotion.rs:164-172`).
- **Embedding needs no new code — but is not free** [P]: the vector sweep covers every visible
  catalog entry's `text_fields` (`vector_store.rs:130,209`), so `task` and `person` become
  searchable without new retrieval code. It still costs RAM and CPU on a CX22 already hosting
  the server, the agent's replica, ntfy and the embedder, with a resize trigger at < 500 MB
  available. Keep `mention` out of `text_fields`; it is a pointer, and there will be thousands.
- **An older phone build drops unknown events silently** [P]: FromStr has a catch-all arm, so a
  phone on the previous APK may discard task and person events it cannot parse and show an
  empty Today until it updates [I]. Deploy order is server, then every client, before rituals
  start sending.

### 5.2 Where each job runs (placement rule, `docs/src/agents.md`)

| Job | Host | Needs |
|---|---|---|
| `.omni/tasks.toml` intake | ⚠️ **open** — see below | a non-transaction intake path |
| Calendar sync (read) | server (intake) | tasks/plans/calendar projections in `build_projections_server` |
| Entity resolution, all of it (blocking + select + zone) | agent host | — (the server folds no journal; splitting the pipeline across hosts adds a sync gap) |
| Morning / weekly / monthly / evening rituals | agent host | generalised scheduled prompts (5.3) |

- ⛔ **Server folds few projections** [P] (`registry.rs:198-216`): a job reading a table the server
  does not fold gets an empty result, silently. The job contract test
  `every_job_sits_on_a_host_that_folds_what_it_reads` (`core/src/jobs/mod.rs:307-320`) catches it
  *if* the job declares its `reads`.
- ⚠️ **The `.omni/tasks.toml` files are not where the server is** [I]. They live in repos on his
  laptops (and GitHub); the server on the box has none of them, and the contract forbids the hub
  a git dependency. The contract's handoff ("a tasks-helper walks configured project roots")
  silently assumes intake runs where the repos are. Options: (a) the **desktop app** runs intake
  on the laptop and syncs events like any device; (b) the workspace's existing **session hooks
  push** each index to the server when they commit it; (c) the server reads indexes from GitHub
  over HTTPS (no git, but private repos need a token, and public-repo titles are already public).
  (b) fits best: the hooks already run at exactly the moment an index changes, and it needs no
  laptop to be awake [I]. **His call** — it touches the dotfiles hooks
  (`feedback-dont-edit-synced-shared-scripts` applies: extend through args/env, or ask).
- **The draft pipeline is transaction-shaped** [P]: `DraftTransaction` requires postings
  (`types.rs:1029-1037`), dedup keys on `Unmatched` legs (`duplicates.rs:12-31`), the batch events
  are gated on `Feature::AutoImport`. Reusing `SubprocessSource` for tasks would force postings
  onto tasks. Reusable: the process boundary, verbs, `external_id`/`dedup_key` semantics.
- **The contract's handoff is stale** [P]: "the existing five" projections (11 now), "a sixth Tab"
  (7 now, `tauri-app/frontend/src/main.rs:39-47`), and "reuse SubprocessSource … no new source
  kind is needed" (`TASK_HUB_CONTRACT.md:151-155`), which the postings finding above refutes. Correct it in the workspace when the push
  starts.

### 5.3 Rituals collide with the check-in

- [P] The check-in is one `AssistantQuestionAsked` with `scheduled: true`, once per calendar day
  (`check_in.rs` ~`:150-185`; agent tick `responder.rs:162-201`). `last_check_in` and
  `check_in_threads` select **every** scheduled message (`check_in.rs:38-43,105-110`).
  → A morning-plan question would make `is_due` think the check-in already ran, and check-in
  housekeeping would archive the ritual threads. `CheckInPolicy` must become a **list of named
  scheduled prompts**, each with its own due-check and housekeeping, before any ritual ships.
- [P] Quiet hours (`core/src/jobs/alerts.rs:157-198`) apply to critical alerts only; the review
  summary is one silent daily send at `notify.summary_hour` (`core/src/jobs/summary.rs`).
- ⚠️ **Conflict with a ruling** [P]: the 19:00 summary is "one silent daily notification of
  counts, built by code … It does not depend on the check-in" (`project.md:360`), and the
  check-in is tier **Ambient**, which never notifies (`project.md:347`). So (a) LLM-written
  people questions cannot ride *inside* the summary, and (b) a ritual modelled on the check-in
  would never reach him. Each ritual needs a tier of its own — §6 asks. Batching (2.4) still
  argues for one evening touch; the summary can carry the *count* ("3 people to place") and
  tap through to where the questions are [I].

### 5.4 Calendar writes are irreversible by our own definition

- [P] `docs/src/assistant.md:418-424`: an action that "calls an external service" is
  irreversible, and irreversible actions are never grantable (`promotion.rs:171`).
  → Every calendar write **always asks**, forever — consistent with ruling 7 ("approved writes"),
  but it retires my earlier argument that calendar writes are safe because events can be deleted.
  The omni-me-owned secondary calendar (3.2) still matters: it bounds the blast radius of an
  approved-but-wrong write, it does not change the approval rule.
- Morning approval stays **one tap** because a proposal can carry a batch (`BATCH_LIMIT` 12, no
  partial approval, `actions.rs:873`) [P]. A day of more than 12 blocks splits — not a real
  constraint at his scale [I].
- **No Google OAuth exists anywhere** [P]: no oauth/google crate; Gmail is IMAP with app
  passwords (`credentials.rs:442`); `credentials.toml` is read-mostly, mode 0600, server-only.
  → Rotating refresh tokens need a writable token store — a new credential pattern, not a reuse
  [I].

### 5.5 People: what exists and what does not

- [P] **Email gives senders only**: `From` and `Subject` are folded into document text
  (`core/src/archive.rs:171-177`); `ParsedMessage` has no To/Cc (`core/src/mime.rs:20-33`).
  → The email mention tier is "people who wrote to me", not "people I wrote to or was copied
  with". Adding To/Cc means a parser change and a re-read of stored `.eml` blobs (they are on the
  server; blobs do not sync).
- [P] **No entity, alias or dedup machinery exists.** Payee "normalisation" is the first
  lowercased word (`ledger_grounding.rs:229-230`); merchant matching is token-Jaccard
  (`reconciliation.rs:138`). Nothing to reuse for people beyond Jaccard as one blocking signal.
- [P] **Ready seam for extraction**: note structuring already has `extract_task` / `extract_date` /
  `extract_expense` tools landing in `NoteLlmProcessed.derived` (`core/src/llm/tools.rs:58-95`).
  An `extract_person` tool fits there.
- [P] **Two shapes to copy for facts**: beliefs (`confidence`, `review_after`, `evidence` filled
  from the trace, never by the model; retired by `BeliefSuperseded`, never deleted;
  `types.rs:1542-1567`) and document fields (source rank human > parser > model with a `verified`
  flag; `docs/src/archive.md:24-97`). A person fact = a belief-shaped record with document-style
  source rank [I].
- [P] **No unmerge exists anywhere**: `TransactionsMerged` sets `superseded_by` and has no
  inverse; merge is not an assistant action (`project.md:249`). → `PersonMerged` /
  `PersonUnmerged` must be designed in with the first person event; retrofitting reversibility
  onto merged history is the expensive order [I].
- ⛔ [P] **Email text is attacker-controlled** (`docs/src/assistant.md:431-448`, quarantined
  reader). A name or "fact" extracted from mail is data, never identity: an email signed "your
  landlord, Tom" must not be able to attach itself to the landlord's person record without his
  confirmation. So the proposal (4.3, asked in §6.8) keeps an email's own claims out of the
  confident zone, or a phishing mail could write into a person record [I].
- [P] **The system prompt routes told facts to notes**: "a fact they told you belongs in a note
  rather than in your memory of them" (`verbs.rs:68-90`). Person facts are a third thing — about
  someone else, cited, confirmed — and **need their own ruling** alongside beliefs.

### 5.6 Other rulings these domains touch

- Journal is never proposable (`actions.rs:371`): mentions may be **read from** the journal;
  nothing derived is written back into it.
- Approval lives with the domain and review queues are not merged into the inbox; a guard test
  refuses routing without a screen (`inbox.rs:100-106`). With chat as the interface (ruling 14),
  tasks and people proposals would stay in the assistant inbox — unless §6.6 gives the morning
  plan or the people pickers a surface of their own [I].
- Routines are habits, `Frequency::Custom` 2–31 days (`invariants.md:137-144`): **recurring
  obligations** (rent monthly, insurance yearly) are not routines; they are recurring tasks or
  recurring-detection outputs. Where penalty-dated bills come from is open (4.1, §6.4).
- New event types reach the **server first**: an older server rejects unknown types with 400
  (`server/src/routes/sync.rs:37-43`) — deploy order is server, then clients.

---

## 6. Open questions for the planning session

His to decide. Asked one at a time, each with its context (explain-before-asking). Grouped by
what they block.

**Answers** (`RULINGS.md`): 6.1 → 93 · 6.2 → 102 · 6.3 → 94 · 6.4 → 95 · 6.5 → 98 · 6.6 → 100 ·
6.7 → 104 (no count cap: his burst-day case) · 6.8 → 103 · 6.9 → 105, 106 (seeding is offline
data prep, not in-app review) · 6.10 → 107 (synthetic benches, not the ZDR-real option) · 6.11 →
108 · 6.12, 6.13 → 105 · 6.14 → 91 · 6.15 → 92 · 6.16 approved, a contract edit · 6.17 → 99 ·
6.18 superseded by 96 · 6.19 → 109 · 6.20 → 96 (adopted).

**Gate the data model**
1. **Whose estimates?** Free actuals exist only for project work, sized by Claude (2.3). Calibrate
   his estimates on native tasks by asking at completion, and treat log actuals as a later check
   of Claude's sizing? Or add a session-to-task tag so project work carries his estimate too?
2. **Person facts vs notes** (5.5): the system prompt sends told facts to notes. Is a person fact
   a third kind of record (cited, confirmed, about someone else), and what stays a note?
3. **Areas** (PARA): a fourth native kind for things that never end, or tasks without a project
   just stand alone?
4. **Penalty-dated bills**: derived from recurring detection, from bill emails, or both — and
   which first?

**Gate the rituals**
5. **Tier per ritual** (5.3): the check-in is Ambient and never notifies; the summary is counts
   built by code. Does the morning plan notify (Review? its own tier?), and where do the evening
   people questions live — their own push, or a count in the summary that taps through?
6. **Which surface hosts approvals** (4.4): chat cards, the assistant inbox, or does the Today view
   need to become interactive for the morning plan and the people pickers — against ruling 14's
   read-only?

**Gate people**
7. **His open question from ruling 13, as a proposal to confirm**: pickers with one-line context
   per candidate, "new" and "don't know" always offered (2.6); answers become evidence as listed
   in 4.3; at most 5 an evening; unanswered questions expire after a day.
8. **An email's own claims** (4.3): accept the narrowing of ruling 13 — display names, signatures
   and bodies never auto-link; only a sender address he confirmed does — knowing it raises evening
   volume?
9. **Backfill boundary**: which date separates "backfill, pointers only" from "new, may ask"? The
   archive backfill starts 2026-11-01 (`project.md:315`) and is still running.
10. **Third-party data to model providers** (`research.md:584`: "Resolve before implementing"):
    people extraction and selection on zero-data-retention seats only, and benches on synthetic
    data only?
11. **Citations vs retention**: a confirmed fact cites an email that retention later deletes. Does
    the fact outlive its source (keeping a quoted snippet), or retire with it?
12. **To/Cc parsing** (5.5): widen the email mention tier now, or senders only to start?
13. **Seeds**: import phone contacts (vCard), or people appear only from mentions?

**Gate the hub**
14. **How `.omni/tasks.toml` reaches the server** (5.2): desktop intake, hook push, or GitHub read.
15. **Thresholds and trial**: N (native staleness), M (index staleness), and the length of the hub
    trial before its acted-vs-dismissed ratio is reviewed (4.1).
16. **Amend the contract** (a workspace file, `setup_files/TASK_HUB_CONTRACT.md`): a `next_id`
    high-water mark so pruning can never cause id reuse, and the stale handoff lines corrected
    (5.2). Every project's sessions follow that contract, so the change is his to approve.

**Gate the calendar** (his idea, 2026-11-18, raised after this draft — a candidate amendment to
ruling 7, not ruled)
20. **Read-only calendar, never write?** Calendar events flow *into* omni-me; everything new is
    added in omni-me; over time his instinct moves to omni-me and the calendar keeps only a few
    legacy things. Replacement by gravity rather than by sync. Consequences [I]:
    - **Removes**: the write scope and its OAuth weight (the secret iCal address may suffice,
      3.2), every irreversible action (5.4) so plan approval becomes reversible and could earn
      autonomy, the always-ask morning approval, and most of reviewer 2's calendar findings
      (duplicate retries, stale-plan writes, his edits vs the plan record) — there is nothing of
      ours in the calendar to conflict with.
    - **Answers my old objection**: I argued against read-only because it leaves two places to
      look. In his version omni-me shows both, so there is one place — the objection does not
      hold.
    - **Costs the cue**: a block that lives only in omni-me gets no phone-calendar reminder, and
      the cue is the part of a block with evidence behind it (2.4). omni-me must fire block
      reminders itself. Via ntfy they spend the push budget (4.5) and pause while the phone's
      Tailscale is off; a **device-scheduled local notification** would not depend on the tailnet
      [I — unverified for Tauri on Android]. Also: his blocks become invisible to anyone who sees
      his calendar's free/busy.
    - **Supersedes** §6.18 (the secondary-calendar check) if adopted, and makes 3.2's write-scope
      split, 4.2's idempotent writes and 5.4 moot.
    My read: it beats approved writes on nearly every failure mode found, and its one real cost
    (the cue) has a plausible fix. Worth deciding first in the planning session, because it
    removes more of §4.2 than any other answer.

**Gate everything**
17. **The time budget** (4.5): ≤ 30 min a week of rituals and ≤ 2 planning/people pushes a day,
    measured like the Critical budget — or different numbers?
18. **Check before building on it**: does an API-created secondary calendar show on his Android
    phone with reminders by default (4.2)? If not, blocks give no cue.

**Order** (priority is his; these are dependencies, not preferences)
19. Ruled already: records come before estimation calibration (`project.md:248`). Hard
    dependencies: the scheduled-prompt generalisation gates every ritual and the evening people
    questions; plan records gate the calendar outlet. Planning does **not** strictly need native
    tasks (it could run on hub tasks and the calendar alone), and people is independent except
    for the evening questions. Which of the three first, and which people purpose first?

---

## 7. Review log

Each pass was an independent read-only reviewer with one lens, run in sequence on the revised
draft. Every cited ruling was re-checked in the source before a change was made.

**Pass 1 — coverage against his 14 answers and the settled rulings.** 3 blockers, 15 should-fix,
6 nits; all applied. The ones that changed the shape:
- *Penalty-dated bills were dropped*, though `project.md:373` moved them into this push → 4.1
  obligations, §6.4.
- *The evening plan broke a ruling*: the 19:00 summary is code-built counts and the check-in is
  Ambient, which never notifies (`project.md:347,360`) → 5.3 conflict stated, §6.5.
- *Three of four people purposes had no design* → 4.3 one bullet each.
- *"Ask-only" misread the belief ruling* (it means the assistant does not volunteer; ruling 12 is
  proposals) → reworded; his "own notes always" restored.
- *His open question from ruling 13 had been answered and closed by me* → returned to him as §6.7.
- *The email-only narrowing changed his ruling unasked* → §6.8.
- Also added: capture-vs-native rule, hub terminal states, a testable trial for his ruling-1
  caveat, recurrence, routines in the fit check, Session 2 element-by-element verdicts, the
  replace-the-calendar milestones, offline capture, the Today empty state, third-party data to
  providers, citations vs retention, Taskwarrior/Vikunja/CalDAV/Dunbar in the frameworks.
- Cut: the 41% to-do statistic (vendor data, added nothing to the [D] measurement).

**Pass 2 — adversarial: assume it ships as written; how does it fail over 6 months?** 4 blockers,
9 should-fix, 2 nits; all applied after re-checking the load-bearing claims (arrival-order
application in `core/src/events/store.rs:553` and the authoring-time workaround in
`config_projection.rs:88-95`; Google's caller-chosen event ids; the contract's id-minting and
pruning lines). The ones that changed the shape:
- *Calibration would have lied*: a pre-filled actual makes "confirm" the cheapest tap, so the ratio
  reads 1.0 → buckets with no pre-fill, block times as actuals, censoring, a sample floor.
- *Unmerge moved only mentions*: facts, notes, edges and roles stayed on the wrong person — the
  role C failure again → every person-owned record unmerges, with a test over every table.
- *The confident zone trusted model confidence*, which this project has seen be wrong at higher
  confidence → deterministic evidence decides the zone; a resolution is recorded once.
- *The contract allows hub id reuse* (highest-id minting + pruning) → `next_id`, refuse a seen id
  with a new title, disappearance = unknown; §6.16.
- Also: per-device ordering → one identity writer; stale-plan approvals → version check;
  idempotent calendar writes; his Google Calendar edits win; sender address as an alias kind and
  source labels against injection; roles resolved at the mention's date; reversible answers and a
  pull-only unplaced list; provisional new persons; the whole resolution pipeline on the agent
  host; per-ritual lapse; a 10-decision weekly cap; a time budget (4.5); capacity notes.
- Kept verbatim: the reviewer's 6-month post-mortem (4.5), as the failure the design is built
  against.

**Pass 3 — evidence quality: is §2 honestly "what the science says"?** The reviewer fetched the
primary pages it could and spot-checked 11 code citations in §5 (all correct). 1 blocker,
7 should-fix, 5 nits; all applied, the two load-bearing ones re-confirmed independently:
- *The WOOP delivery claim was false as written*: the best arm was face to face with a **human
  experimenter**, not a conversation with an assistant; ~87% of the headline N sat outside the
  moderator analysis; trim-and-fill g = 0.24 → 2.5 rewritten, the assistant mode labelled
  untested.
- *Implementation-intention numbers*: the 2024 paper is now named (Sheeran, Listrom &
  Gollwitzer), the mis-attached Konstanz link replaced, and the behaviour estimate (d ≈ 0.27)
  leads; the bias-corrected figures stay [S].
- *My inferences that ran the wrong way or too far*: Sunsama is a capacity check, not the outside
  view; "someday" does not discharge Masicampo's effect; lapse "triggers" and "thresholds" cited
  to 2.7 that no source gives → all relabelled [I]; SMART "harms" softened; the masters example
  removed; OpenSanctions softened to its own claim and its richer domain noted; Dunbar's three-
  level simplification labelled.
- *Google API facts corrected*: cancelled events and the 400 rule were misstated; added the trap
  that incremental sync cannot be date-windowed, and the narrower `owned.readonly` scope.
- *Upgraded*: RFC 10051 confirmed [P]; the fresh-start placebo result is in the primary paper,
  and the month boundary is its weakest landmark (now stated).
- *Added*: §2.8, five named bodies of work not consulted; weak/dormant ties, which cut against
  "outer layers are reference only" for professional use.

**Pass 4 — my own read-through after all three.** Checked that §4, §5 and §6 still agree after
three rounds of edits, and that every [P] was actually read. Changed:
- ⛔ *A design flaw none of the three caught*: "a single blocked candidate" counted as confident
  evidence. If he knows one Sarah and meets a new one, she auto-links to the old — his
  shared-first-name case reproduced by the design → a bare first name is never confident; only a
  confirmed sender address or a confirmed full-name alias is.
- *Two people rules contradicted each other*: "my landlord" resolved to the current holder in one
  bullet and to the holder at the mention's date in another → the mention's date everywhere.
- *Assertions left behind by earlier passes*: 5.5 still said email evidence "must" be excluded and
  5.6 still said where bills "come from", after both became open questions → softened to point at
  §6.
- *Labels*: ten [P] downgraded to [S] where I had seen only a search snippet (Spolsky, Motion,
  Taskwarrior, Splink, VTODO, RFC 9253, the icalendar crate, the secret iCal address, Android's
  calendar provider). §2.2 now measures index freshness by commit time, matching 4.1.

---

## 8. Sources

Grouped by section. Where only a secondary page was read, the primary citation is given with the
page actually consulted.

**Capture and offloading (2.1)**
- Risko & Gilbert (2016), "Cognitive Offloading", *Trends in Cognitive Sciences* 20(9) — via
  [UCL Discovery](https://discovery-pp.ucl.ac.uk/id/eprint/1508770),
  [ScienceDaily](https://www.sciencedaily.com/releases/2016/08/160816134021.htm)
- Intention-offloading review — [PubMed 35789477](https://pubmed.ncbi.nlm.nih.gov/35789477/)
- Masicampo & Baumeister (2011), "Consider it done!", JPSP —
  [PubMed 21688924](https://pubmed.ncbi.nlm.nih.gov/21688924/)
- Heylighen & Vidal (2008), "Getting Things Done: The Science behind Stress-Free Productivity",
  *Long Range Planning* 41(6) —
  [preprint](https://web-archive.southampton.ac.uk/cogprints.org/6289/1/Heylighen-Vidal-GTD-Science.pdf)

**Estimation (2.3)**
- Planning fallacy, reference-class forecasting —
  [Wikipedia](https://www.Wikipedia.org/wiki/Planning_fallacy),
  [PMI](https://www.pmi.org/learning/library/planning-fallacy-causes-solutions-project-expectations-6374)
- Spolsky (2007), "Evidence Based Scheduling" —
  [joelonsoftware.com](https://www.joelonsoftware.com/2007/10/26/evidence-based-scheduling/),
  [Wikipedia](https://en.wikipedia.org/wiki/Evidence-based_scheduling)
- Kruger & Evans (2004); Forsyth & Burt (2008), *Memory & Cognition* 36(4) —
  [doi 10.3758/MC.36.4.791](https://dx.doi.org/10.3758/MC.36.4.791),
  [JDM backward-planning paper](https://dlab.sauder.ubc.ca/sjdm/journal/16/16101/jdm16101.html)

**Planning and goals (2.4–2.5)**
- Gollwitzer & Sheeran (2006), *Advances in Experimental Social Psychology* 38:69–119 — cited
  secondhand; Sheeran, Listrom & Gollwitzer (2024), *European Review of Social Psychology* —
  [doi 10.1080/10463283.2024.2334563](https://informahealthcare.com/doi/full/10.1080/10463283.2024.2334563),
  bias-corrected figures via the
  [Behavioral Scientist glossary](https://www.thebehavioralscientist.com/glossary/implementation-intentions)
- Wang, Wang & Gai (2021), MCII meta-analysis, *Frontiers in Psychology* —
  [PMC8149892](https://pmc.ncbi.nlm.nih.gov/articles/PMC8149892)
- Time management meta-analysis (Aeon et al. 2021), secondhand only —
  [goalsandprogress.com](https://goalsandprogress.com/productivity-strategies/)
- Dai, Milkman & Riis (2014), "The Fresh Start Effect", *Management Science* 60(10) —
  [PDF](https://faculty.wharton.upenn.edu/wp-content/uploads/2014/06/Dai_Fresh_Start_2014_Mgmt_Sci.pdf)
- Fitz et al. (2019), batching notifications, *Computers in Human Behavior* —
  [PDF](https://static1.squarespace.com/static/57a40c19414fb54f51f8095f/t/614a55faa7b89e25f4e48ad1/1632261627146/2019+Fitz+Batching.pdf),
  [Duke Scholars](https://scholars.duke.edu/display/pub1402953); Pielot et al. (2014) —
  [MobileHCI PDF](https://ic.unicamp.br/~oliveira/doc/MHCI2014_An-in-situ-study-of-mobile-phone-notifications.pdf)
- Locke & Latham (2002), *American Psychologist* —
  [PDF](https://goal-lab.psych.umn.edu/orgpsych/2020/readings/5.%20Motivation/Locke%20&%20Latham%20(2002).pdf)
- Emmons & King (1988), goal conflict —
  [World Database of Happiness](https://worlddatabaseofhappiness.eur.nl/publications/conflict-among-personal-strivings-immediate-and-long-term-implications-for-psychological-and-physical-well-being-2860)
- Sunsama rituals — [daily planning and shutdown](https://www.sunsama.com/features/daily-planning-and-shutdown)
- Motion auto-scheduling — [what it considers](https://www.usemotion.com/help/time-management/auto-scheduling/reference-auto-scheduling/what-auto-scheduling-considers)

**People (2.6)**
- Roberts & Dunbar (2011), "The costs of family and friends", *Evolution and Human Behavior* —
  [Oxford ORA](https://ora.ox.ac.uk/objects/uuid:b8ab748c-5892-4e7a-a8e0-f529d694a94c/files/s6969z1798)
- Liu, Rim, Min & Min (2023), "The Surprise of Reaching Out", JPSP 124(4) —
  [APA release](https://www.apa.org/news/press/releases/2022/07/friends-enjoy-being-reached)
- Dunbar layers; MacCarron, Kaski & Dunbar (2016) —
  [phys.org](https://phys.org/news/2016-05-humans-capable-people-closet-circle.pdf),
  [Research Outreach](https://researchoutreach.org/articles/size-matters-social-groups-human-evolution/)
- Balog & Kenter (2019), "Personal Knowledge Graphs: A Research Agenda", ICTIR —
  [Google Research](https://research.google/pubs/personal-knowledge-graphs-a-research-agenda/);
  ecosystem survey [arXiv 2304.09572](https://arxiv.org/abs/2304.09572v2)
- Fellegi–Sunter — [Splink topic guide](https://moj-analytical-services.github.io/splink/topic_guides/theory/fellegi_sunter.html),
  [Linacre's interactive intro](https://robinl.github.io/intro_to_probabilistic_linkage/)
- OpenSanctions Pairs — [arXiv 2603.11051](https://arxiv.org/html/2603.11051);
  "Match, Compare, or Select?" — [COLING 2025](https://aclanthology.org/2025.coling-main.8/)
- Fellegi & Sunter (1969), "A Theory for Record Linkage", *JASA* 64(328):1183–1210 — via Splink
- Clarification as disambiguation — [Active Task Disambiguation, ICLR 2025](https://arxiv.org/pdf/2502.04485),
  [ECLAIR](https://arxiv.org/html/2503.20791v1)
- Monica — [monicahq.com](https://monicahq.com/), [v3 rewrite post](https://monicahq.com/en/blog/we-are-rebuilding-monica)

**Cross-cutting (2.7)**
- Epstein et al. (2015), "A Lived Informatics Model of Personal Informatics", UbiComp —
  [PDF](https://my.eng.utah.edu/~cs5540/au16/readings/PersonalInformatics-Epstein2015.pdf)

**Standards, APIs, tools (3.2)**
- [RFC 9253 iCalendar Relationships](https://www.rfc-editor.org/rfc/rfc9253.html);
  [CalConnect VTODO guide](https://devguide.calconnect.org/tasks/component/);
  [RFC 10051 datatracker](https://datatracker.ietf.org/doc/rfc10051/);
  [icalendar crate Todo](https://docs.rs/crate/icalendar/0.17.9/source/src/components/todo.rs)
- Google Calendar — [sync guide](https://developers.google.com/calendar/api/guides/sync),
  [events.list](https://developers.google.com/workspace/calendar/api/v3/reference/events/list),
  [events.insert](https://developers.google.com/calendar/v3/reference/events/insert),
  [create-events guide](https://developers.google.com/workspace/calendar/create-events),
  [auth scopes](https://developers.google.com/calendar/api/auth),
  [OAuth testing-mode expiry](https://support.google.com/cloud/answer/15549945),
  [secret iCal address](https://support.google.com/calendar/answer/37648)
- [Android Calendar provider](https://developer.android.com/identity/providers/calendar-provider)
- [Taskwarrior urgency](https://taskwarrior.org/docs/urgency/)

**Our own sources**: `TASK_HUB_CONTRACT.md`, `research.md:340-490`, `project.md` § Decisions in
force and § Notifications, `docs/src/{assistant,agents,invariants,archive}.md`, and the code paths
cited in §5; the seven `.omni/tasks.toml` indexes (measured 2026-11-16); session renders under
`productive_learning/logs/omni-me/`.

---

## Appendix A — his answers, as recorded (2026-11-16, idea 7b added 2026-11-18)

Prior art found: setup_files/TASK_HUB_CONTRACT.md (single-writer, .omni/tasks.toml, hub read-only;
handoff section partly stale: "sixth Tab", "five projections"). research.md:403 People Tracker.
Indexes exist: 5 repos + 2 ~/life topics.

### Tasks & projects
1. Source: HUB + NATIVE. Repo/~/life tasks read-only; omni-me owns in-app/assistant tasks.
   His caveat: "we'll have to see how effective it is" — fears noise + staleness from project tasks.
2. Surfacing: PULL + NEEDS-YOU. Catalogue by default; USER-sized or due-dated project tasks
   surface automatically; everything else pulled in deliberately.
   Research ask: frameworks — code AND conceptual — for all three sections; "what does the science say".
3. Calibration = HIS estimates (planning fallacy). Actuals derived from session logs where possible,
   asked at completion otherwise. Claude's XS–L sizes are a possible later reuse, not the goal.
4. Projects: NATIVE TOO. omni-me owns its projects; native tasks point at native or hub (slug) projects.
   PARA (areas) deferred to framework research, not ruled.
5. Today: ~40% head, 30–40% calendar, ~5% phone reminders, rest paper scribbles that get lost.
   Inconsistent. → capture friction is the real enemy; calendar is already half the system.
6. Session 2 task/goal design (research.md:353-380: goal=project tree, no task deadlines, scheduled
   date, two-part deferral justification, 5-active-goal cap, SMART on activation): START FRESH.
   It is one input to research, not the baseline.

### Planning
7. Calendar (Google assumed, Session 2 chose Google Calendar API): READ + APPROVED WRITES now;
   LONG-TERM GOAL: omni-me replaces the calendar eventually ("would take a while").
   → design so calendar is an adapter, not the store of record for plans.
8. Day plan: MIXED — deep/important work gets calendar blocks; small tasks on an untimed day list.
9. Cadence: ASSISTANT RITUALS — prompted weekly planning + short morning day-plan proposal;
   lighter monthly review. (Existing hooks: check-in clock docs/src/agents.md:30, ntfy.)
10. Goals: GOALS, KEPT LIGHT above projects; reviews check goal progress; gates (SMART, caps)
    only if research supports them.

### People (not "CRM" — his reframe)
11. Purpose: ALL — keeping in touch, professional network, admin counterparts, recall before talking.
    His words: "the central reference for everything I know about any other person I have
    interacted with directly or indirectly and recorded something about". The hard part (his):
    "both wide and robust but also focused and useful".
12. Input: TWO TIERS — automatic mention index (pointers) + facts proposed with source citation,
    confirmed by him; own notes always. His warning: misspells names then corrects later; several
    people share a first name with no surname known; only surrounding context separates them —
    "a veritable minefield" for how records update. → entity resolution is a headline topic.
13. Ambiguity: HYBRID — confident matches attach with a "guessed" marker; genuinely ambiguous asked
    SAME DAY in a batched evening check-in (resolve while memory is fresh — his reason: waiting
    until needed means he's forgotten); old backfill = pointers only, never questions.
    Open (his): how the questions are posed and how his answers are used.

7b. (2026-11-18, his idea, NOT yet ruled — he wanted it noted): READ-ONLY calendar, never write.
    Calendar events show up in omni-me; new things are added in omni-me; "ideally over time ... my
    first instinct would slowly become to use omni-me exclusively with only a few legacy things
    coming from the calendar". Replacement by gravity instead of by sync. Candidate amendment to 7.

### Cross-cutting
14. Interface: CHAT for capture/questions/planning + ONE read-only TODAY view (blocks, day list, due).
