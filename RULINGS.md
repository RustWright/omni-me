# Rulings

Product and process rulings in force, numbered. They are the user's unless marked "mine".
Code that depends on one cites it as `ruling N (RULINGS.md)`, and `scripts/check-rulings.py`
fails CI when a cited number is missing here. A number is never reused: a revised ruling
keeps its number, and retiring one means changing the code that cites it.

The reasoning behind 1–80 is in `git show 7ef0e55:tasks.md` (search the topic), in
`docs/src/`, or at the edit site. 81–89 were recorded only in code comments until [0073]; the
edit site carries their reasoning. These are settled. Do not re-open one without a new reason.

## Product shape
- **1.** The journal is user-authored only. The assistant reads and cites it and can never
  propose a write to it (`NEVER_PROPOSABLE`). This is an invariant, not a phase.
- **2.** Approval lives with the domain. The assistant inbox holds only proposals with no other
  home. A badge routes, it never decides. Never merge review queues into the inbox: the flows
  share an event shape, but the review UI is the whole value (a receipt needs editable lines,
  a re-run and a correction flow, which a generic card cannot carry).
- **3.** Autonomy is earned per action type and only the user grants it. Irreversible actions
  (deletion, purge, `transaction.clear`) can never be granted. A purge confirms once per
  group, listing every document, each one sparable.
- **4.** Beliefs are ask-only. The preferred long-term shape (proposing only on repeated
  independent evidence, run by role B) needs a per-role turn budget and a second scheduled
  question first. `belief.record` is refused until the run has listed a date range reaching
  the last 30 days. The daily check-in proposes retiring beliefs that no longer hold, and
  it is off by default.
- **5.** The archive is a filing cabinet, not an inbox: no badge. Emails render in the archive.
  It is not a mail client. The mailbox is never written to (`EXAMINE`). Gmail write-back is
  the stated destination once trust is built: deferred, not rejected.
- **6.** Where a clean separation can be defined, define it (wipes, isolation, imports).
- **7.** Task and project records come before estimation calibration. Nothing task-shaped exists yet.
- **8.** Finance actions: everything except delete and merge. `transaction.update` reaches the
  description and date only. `transaction.clear` requires a cited statement.
- **9.** Notes: `note.append`, `note.rename` and `note.revise` exist. `note.update` was rejected.
  `note.revise` matches an exact, unique text anchor, resolves at approval, and logs the
  finished body. Never make it fuzzy, and never move resolution into the projection.
- **10.** `routine.complete` and `routine.skip` can be proposed, because the journal is readable
  for exactly this. There is no partial batch approval (`BATCH_LIMIT` 12).
- **11.** The release is the assistant, the archive and finances together. The user's sign-off on UX
  and performance is the gate.
- **12.** A new day opens the journal on today. The same day restores the date last viewed.
- **13.** Tab order, when configurable, is per device (device layer). The app opens on the first
  visible tab of that order; there is no separate landing-tab key.
- **14.** A device saves a server address only when the user sets one. An untouched device follows the
  build's default, so a hostname change is one CI variable.
- **81.** The finances count includes rows waiting to be reconciled: the ledger needs both
  queues cleared to stay current.
- **86.** A skipped routine item counts as done: a skip is a deliberate act with a reason.
- **87.** The routines screen keeps its own day roll-up rather than calling
  `core::routines::roll_up` over IPC. The shared fixture `routine_day_agreement.json` holds
  the two together.
- **88.** Archive review moves to the next document with something unchecked once the current
  one has none, or back to the list when none does.

## Assistant engineering
- **15.** Keep our Rust loop, with no framework (`ASSISTANT_RESEARCH.md`). A full rewrite is
  acceptable if it is markedly better. Quality outranks token cost. Specialists come per
  feature, the ledger first.
- **16.** A small fixed verb set over declared record types. Changes are proposed with a reason.
- **17.** The turn budget is tunable within 3–50 (the floor is the halting guarantee). `propose`
  sits outside the budget. The final turn withholds tools, so a reply is guaranteed.
- **18.** The app writes an outcome line under every reply, built from its suggestions and never
  from the prose.
- **19.** Chats untouched for 30 days auto-archive, and archived chats auto-delete after 90.
  Check-ins are archived, then deleted after 14 days. A thread with an undecided
  suggestion is never deleted.
- **20.** Waiting indicator: an animation now, real step-by-step progress after the merge.
- **21.** Ledger grounding: the user's `CONVENTIONS.md` (paisa ledger, never in this repo) is the
  source. Known accounts are the ledger's, plus those declared, plus any the note names.
  I draft the live conventions note and the user reviews it.

## Models and benches (`MODEL_BENCH.md`, `MODEL_THRESHOLDS.md`)
- **22.** Hosting is DeepInfra, open weights only, with US jurisdiction an accepted risk. Closed
  weights are refused unless `allow_closed_weights` is set.
- **23.** One model per job, chosen statically by benchmark, never by a runtime orchestrator.
  Re-evaluate every 3–6 months. The synthetic bench is a bootstrap: shadow mode and the
  correction log supersede it.
- **24.** Thresholds and tie-breaks are written before a run. A saturated instrument is published
  as unmeasured. A winner is switched on dev only. No hand-labelled gold set (the user's time).
- **25.** Sampling: roles B, C1–C3 and D at temperature 0; role A at the provider default.
- **26.** Spend the OpenRouter credits generously on screening. The deciding run goes direct.
- **27.** `MODEL_BENCH.md` R27's account side is the user's. No list of affected runs exists, by decision.
- **28.** The triage seat stays in shadow mode until real weeks score it. It never gates archiving.
- **29.** Do not tune the extractor prompt against the four sample receipts. `json_schema` beats
  `json_object`: a loud 400 beats a wrong 200.

## Mail, import and reconciliation
- **30.** Never add a vendor to fix a missed receipt. The sender gate is inverted and the
  classifier decides (overlay `GATE_INVERSION_DESIGN.md`). Candidate group keys were
  refused. A printed order reference outranks the model's reading.
- **31.** Newest wins, because later documents are more authoritative. Any scoring respects
  document authority first.
- **32.** A receipt's stated total takes precedence. Never fetch a linked page for missing items.
- **33.** Duplicates are dropped only on a shared Message-ID, otherwise flagged. The two halves of
  an own-account transfer are joined. A receipt with no total becomes one transaction.
- **34.** Matching an authorisation plus its adjustment, and cross-currency reimbursements: dropped,
  matched by hand. Do not re-raise either unprompted.
- **35.** One receipt to several charges: shelved, hand-merged, until more than 5 occur in one week.
  False suggestions would cost more than the hand merges.
- **36.** Imported history keeps the granularity it was imported at (crypto stays monthly); feeds book
  each purchase as it happens. No aggregation on import, no conversion.
- **37.** Bank feeds are the long-term source. Statements are for reconciliation and closing.
  Import parity with paisa is retired; a source's statement reading is built only when the
  feed health check needs it.
- **38.** Old mail is archived on live by a slow backfill, with no financial extraction before the
  imported statements. Cursors and the backfill both start 2026-11-01.
- **39.** Retention: one category per email from a fixed list, chosen at intake. The category's
  timer decides unless a topic tag the user set carries a longer one. The clock counts from the
  email's own date. Social mail is not archived. The reader tags area of life only.
- **40.** A draft dated after today cannot be committed from review. An empty batch reaches review
  only when it is a receipt.
- **41.** Encrypted PDFs: every `pdf_password*` secret is tried. The server serves a decrypted copy,
  so the phone never holds a password. A multi-photo receipt is one combined PDF. Photos
  shrink on the device before upload.
- **82.** No intake without the retention filter: with no category timed, the backfill refuses
  to run rather than keep every message.
- **83.** Bytes already held are not filed again, purged ones included, but the message is
  still dispatched to the handlers.
- **84.** An amount charged makes an email a receipt, whatever its sender. A skip-listed sender
  is offered to the handlers first, and a charge is written at parser rank, above the reader's
  guess.
- **85.** An email past about 50 pages of text is not sent to the model. It reaches review
  empty, pointing at its archived copy.
- **89.** A batch may be committed from the list without opening it only when the sender
  authenticated, the line items summed to a stated total, and nothing raised a warning.

## Data and operations
- **42.** Live is the pause. Every live step waits for the user's go, one at a time. Dev is mine.
- **43.** A wipe is a server route (preview, then confirm), generic over features, with documents
  refused and a snapshot taken first. Devices follow a wipe. A cursor reset is its own route.
- **44.** Stay on the CX22, with Cloudflare R2 (free tier) as the off-box copy of the blobs. Resize
  only on a trigger: under ~500 MB available, or an OOM kill.
- **45.** Dev's agent stays stopped. It runs only while live's agent is idle.
- **46.** Fixes batch into the next release, not one release per fix.
- **47.** If config and the instance marker disagree, the server refuses to boot. The live box and
  the user's real phone are outside the dev grant, always.
- **48.** On a significant failure, stop, and write down what stopped it and what came next.
- **49.** The phone-width layout sweep, including the gutter check, is a CI gate.
- **50.** The ledger-classification rulings (F2) live in paisa's `CONVENTIONS.md` and
  live's "Ledger conventions" note, not here.

## Notifications (`DESIGN_PREREAD.md` Part 1)
- **51.** Three tiers by what happens if the user never sees an item: critical interrupts, review rolls
  into one daily summary plus the badge, ambient never notifies.
- **52.** Critical means missing it costs money or leaves the record wrong, and waiting makes it worse.
- **53.** Finance mapping. Critical: a source needing a code or reauth (at once); a source erroring
  for 24 h; a statement that disagrees with the ledger; a large charge; a new-merchant charge;
  money due with a penalty date (the day before). Review: proposed batches and transactions,
  unmatched (its own count, apart from review), the purge queue. Ambient: duplicate flags,
  recurring-charge detection, the check-in.
- **54.** The large-charge threshold is a setting, and the new-merchant alert has its own on/off.
- **55.** Critical's budget is measured, never enforced: target ~3 a week. Over it, alerts still go,
  and the next summary asks whether to demote the noisiest kind.
- **56.** Demoting is a setting per alert kind, Critical or Review, default Critical. A
  demoted kind's alerts are listed in the daily summary instead of interrupting.
- **57.** Slice 4 starts with the large charge, threshold 500 by default.
- **58.** A notification tap opens omni-me on the right tab through an `omni-me://<tab>` link.
  If it does not work properly on Android, it is shelved until a more holistic
  notifications rewrite rather than patched.
- **59.** Quiet hours (default 22:00–08:00, adjustable) hold every critical alert until morning. A
  setting lets charge alerts through at night, off by default.
- **60.** Review reaches the user as one silent daily notification of counts, built by code (default
  19:00, adjustable), skipped when nothing is new, tapping through to the tab. It does not
  depend on the check-in.
- **61.** Delivery starts with ntfy self-hosted on the box and the ntfy app on the phone, with no
  change to omni-me's app, chosen as the prototype. UnifiedPush or Google push (FCM) into
  omni-me's own app stays open as a later upgrade of the last mile only.
- **62.** ntfy is reachable over the tailnet only, with auth on. Alerts pause while the
  phone's Tailscale is off, and ntfy's 72 h cache delivers them on reconnect.
- **63.** Build order: (1) ntfy, source alerts and quiet hours; (2) the summary, with unmatched split
  from the review count; (3) budget measurement; (4) each new detector, ruled on its own.
  Slices 1–2 lead Cycle 5's build work and do not wait for Part 2's contract. Building
  starts once Part 2 has been discussed.
- **64.** Slice 4 stops at the large charge. New-merchant alerts are dropped: the banks
  already send them, and payee descriptions are too noisy to key on. Penalty-dated bills go
  into the next big push (scheduling and day/week/month planning, plus a personal CRM of the
  people the user deals with and what is known about them), not built on their own.

## Agent framework (`DESIGN_PREREAD.md` Part 2)
- **65.** The haphazardness is: no common shape across jobs, results the user does not fully trust, jobs
  in odd places, and no clear, visualisable layout of how the agents are built and composed.
  The user wants to understand how modern agentic systems (Claude Code, Codex) are composed, learn
  from them, and write mylearnbase posts about omni-me's agent structure.
- **66.** Part 2 splits: 2b, an architecture study (how Claude Code/Codex-style agents are composed,
  omni-me's agents mapped and diagrammed, run as a learning thread), comes first. 2a, the job
  contract, registry and placement rule, is decided after it. Part 1's build waits for both.
- **67.** Page demos are a showcase, not each feature's proof: built where a visitor can
  try a screen, chosen for what they show recruiters, one page or flow per mock bundle. Pipeline
  features keep evidence that fits them (recorded runs, bench scores), since a mock demo of a
  backend shows fake data arriving and proves nothing about real input.
- **68.** The logbook set (all four published on mylearnbase) is four entries, in order: the assistant asks before it
  acts, receipts from email, search by meaning, the document archive. Statements and OTA
  releases are dropped, not deferred. A one-page mock build is measured first (my proposal) so
  the demos for 1, 3 and 4 can follow.
- **69.** Logbooks are objective and descriptive, so I draft the prose and the user checks it;
  this overrides mylearnbase `editorial/editing.md`'s "the prose is his" for logbooks only.
- **70.** Each demo shows the one interaction that proves its claim, nothing more. Order:
  the assistant (scripted questions, a waiting proposal, a refused journal write), then the
  archive (a correction that survives a re-run), then search.
  The archive demo has no re-run: the app has no such control, so the demo shows
  fields by source and a correction that sticks, and the cited fold tests carry the re-run claim.
  Search gets a screenshot, not a demo: the Assistant page answering four questions,
  each citing the record the server's exact model ranks first (`scripts/search-demo-rank.py`,
  `OMNI_DEMO_SCENARIO=search`). A standalone ranking page was rejected as showing no app, and an
  in-browser model as ~35 MB in a different runtime; the measurements carry the post.
- **71.** Demos are built locally: the mock web page only, memory-capped, never the app,
  server or Android build. The user installs any tool it needs.
- **72.** Order: Part 2a, then notifications slice 1, then the logbooks, so the
  source-failure alert lands before the writing. Slices 2–4 sit after the logbooks in
  `tasks.md`; that placement is my reading, not stated.

## Job contract (Part 2a; rationale in `docs/src/agents.md`)
- **73.** Every job declares: name, shape (agent, workflow, plain code), trigger, host, off switch,
  failure policy, output and tier, and a run record (start, end, outcome, counts, spend). A job
  that calls a model also declares its model seats (with effort or thinking), a per-run budget
  and its permission (read, propose, autonomous). A correction record waits for enrichment.
- **74.** A registry lists every job, and a status view reads the run records: trust is built from
  that record, not from checking every output.
- **75.** Placement: device only for what must work offline or is computed when a screen opens; server
  by default and always for intake; agent host only for the agent loop and what serves it. A job
  may only sit on a host that folds every table it reads, guarded by a test per job. Applied as
  each job migrates, never as a mass move; recurring detection is the one misfit (to the server).
- **76.** The status view is Settings → Jobs, a collapsed section listing every job.
- **77.** Mail import migrates first, inside notifications slice 1, whose alerts are its failure policy
  and tier; bank import follows on the same alert.
- **78.** No Rig spike: the one OpenAI-compatible client already covers every provider in use. Revisit
  only for a provider that does not speak that protocol.
- **79.** Codebase scaling (ruled in the study): no per-area "process owner" agents. Nothing
  was named that one adds beyond area knowledge and tests, and every handoff is a cold start plus
  a lossy report. Instead, area traps, rulings and past failures go in path-scoped Claude Code
  rules (loaded when matching files are touched), procedures in skills, and the test layers stay
  as they are: module tests for development work, the benches for the runtime agent.
- **80.** Those rules never reach the public repo: synced and project-scoped like memory. Mechanism:
  `tasks.md` [0083].

## Tasks, planning and people ([0074]; rationale in `PUSH_0074_RESEARCH.md` §1–§6)
- **90.** Tasks are hub and native. Repo and `~/life` tasks arrive read-only through
  `.omni/tasks.toml` (`~/life`'s `USER` items are handed over, 121); omni-me owns tasks and
  projects made in-app or by the assistant. Session 2's task design (`research.md`) is an input,
  not a baseline.
- **91.** Project tasks are pulled in deliberately; only needs-you items (`USER` size or a `due`)
  surface on their own. Indexes reach the server by the session hooks pushing each changed index,
  never by reading GitHub, which serves one branch while work lives on dev branches.
- **92.** Staleness thresholds are settings: an index unchanged for 21 days surfaces as one "still
  current?" line instead of its items; a native task untouched for 4 weeks is offered for drop in
  the weekly review. Hub surfacing is a 4-week trial of acted versus dismissed, after which the
  weekly review asks whether it stays.
- **93.** Calibration is of his estimates, on native tasks: estimate at planning; the actual from
  a start/pause/stop timer when he used one, else asked at completion. A timed session keeps both
  active and elapsed time, and scheduling calibrates on elapsed, because breaks take day time. A
  timer running well past its block gets one nudge. Start sits on the block reminder and on the
  current block in the view; pause and stop on the view, an exception to read-only like Undo.
  Project work keeps Claude's `XS`–`L` sizing, which log actuals may later check. No
  session-to-task tag.
- **94.** No areas: a task without a project stands alone. Goals sit above projects, kept light,
  with gates only where evidence supports them.
- **95.** Penalty-dated bills (amount, due date, penalty date) are created by chat for now.
  Bill-email and recurring-detection sources are deferred, not cancelled.
- **96.** The calendar is read-only and never written. Events flow into omni-me, everything new is
  created there, and the calendar retires as his habits move. No calendar action is irreversible,
  so plan approval can earn autonomy. omni-me fires its own block reminders.
- **97.** The day plan is mixed: blocks for deep work, an untimed list for small things.
- **98.** Rituals are assistant-initiated: a morning day-plan proposal, an evening shutdown, a weekly
  plan and a light monthly review. The morning plan has its own ritual tier: one silent push,
  drafted at his first activity of the day with a set time only as the fallback (114). Unanswered,
  it lapses when its first block would start, not at a fixed hour, since a late start can pass
  noon; the day then runs as after "Skip today", and asking for a plan drafts a fresh one.
  Evening, weekly and monthly ride the 19:00 summary (60) as code-built count lines that tap
  through to chat.
- **99.** Ritual load is measured, never enforced, as in 55: at most 30 minutes a week, a weekly
  review of at most 10 decisions with the rest rolling over, and at most 2 planning or people
  pushes a day. Block reminders do not count, and burst days are measured, not blocked. A ritual
  skipped 5 times running pauses itself and says so once.
- **100.** Chat carries capture, questions and planning. Approvals are chat cards to start: the
  morning plan is one batch proposal, changed by saying so (a re-plan supersedes it), and people
  questions use a picker card. One read-only view sits beside chat; whether it is a Today view or
  spans several timescales is decided in the UI design session, from rendered candidates.
- **101.** People is the central reference for everyone he has interacted with and recorded
  something about, not a CRM. Its uses: keeping in touch, professional network, admin
  counterparts, recall before talking. Wide in storage, focused in presentation.
- **102.** Two tiers: an automatic mention index of pointers, and facts proposed with a citation
  and confirmed by him. A person fact is its own kind of record, neither a note nor a belief;
  its fields are settled by the data prep.
- **103.** A confident match attaches with a "guessed" marker. In email, only a sender address he
  confirmed, on mail passing sender authentication, is confident; display names, signatures and
  bodies never are, because mail text is the sender's to forge.
- **104.** Ambiguous mentions are asked the same evening, with no count cap. A picker shows the
  source line, one context line per candidate, "new person" and "don't know"; names with no
  candidate come as one group card per source. Unanswered questions expire after one more day.
  Every answer is a reversible evidence event: link, provisional new person, cannot-link,
  misspelling alias, or unlinked and never re-asked.
- **105.** Each domain is seeded by offline data prep, as finance was: records are pulled locally,
  worked through with him until the shape is right, then imported. People's prep takes one vCard
  export and checks To/Cc before any parser change; the live email tier is senders only.
- **106.** Live, a mention whose source date is older than the one-day question window gets a
  pointer only (mine, uncontested).
- **107.** Other people's data goes to ZDR seats only. Benches use synthetic people data to start,
  reconsidered if quality disappoints. The OpenRouter privacy setting ([0010]) is confirmed before
  OpenRouter's next use, for anything.
- **108.** A confirmed fact outlives its source email under retention, keeping the quoted sentence
  and the sender, date and subject, marked "source expired".
- **109.** Order: tasks, planning, people, with the people data prep beside the task build. The UI
  design session comes before any build, and the Cycle 5 test gaps on the summary and tiers come
  before the scheduled-prompt work.
- **110.** The read-only view beside chat (100) is one scroll: what is left of today, then the rest
  of the week as planned-against-free rows, then the month's goals (candidate C, `design/0102/`).
  Provisional until he has used it.
- **111.** An all-day calendar event is a label: it never takes free time and never enters the
  timeline. One that looks like it takes the day is asked about once, and the answer holds for
  that event or series. "Show me as" is not trusted, since hand-entered birthdays are ordinary
  events. Reviewed after the calendar data prep ([0112]).
- **112.** Planning is a specialist, as the ledger is (15): routing picks a planning brief, code
  tools compute free time and fit, and a validator refuses a plan with a block on an event, outside
  the window or overlapping another, back to the model on the same turn. The morning ritual is a
  job with that brief and its own seat. Not a separate agent: re-planning lives in the chat thread.
- **113.** The server checks the calendar for changes every 15 minutes, and every plan or re-plan
  fetches it fresh first. Only the "now overlaps" flag on an accepted block waits on the interval.
- **114.** The day is planned from when he is actually up toward a bedtime target, with no fixed
  start window: his waking time varies widely, and a plan that assumes an early start on a late
  morning gets ignored whole.
  The day starts at his first activity (first routine tick, chat, or the push) plus his learned
  morning routine, and ends at the bedtime target less the evening routine. The bedtime target is
  the one number he sets, as something to aim at. The evening shutdown shows what tomorrow's
  bedtime buys, measured, never enforced (99). Day shape: overlay `PLANNING_DAY_SHAPE.md`.
- **115.** Planning gets named places and a travel time between them, from his estimates, and
  buffers travel before an event at a different place. Location history (`research.md`
  § Locations) is its own later cycle and reuses those places, supplying real travel times.
- **116.** Overlapping calendar events count once toward busy time; exact duplicates (same title
  and time) merge silently. A real clash is one line on the plan card asking which he is going
  to; travel (115) assumes the earlier until he answers, and an answer holds for that day only.
  Reviewed after the calendar data prep ([0112]), as 111 is.
- **117.** The morning plan card is section 1 of `design/0102/`: calendar events inline and greyed
  so the fit can be checked, the fit line the one coloured line and never blocking Accept, "Skip
  today" free of any follow-up, and a re-plan collapsing the old card to one line, its approval
  refused. No "From:" row; a line appears only when an input could not be read.
- **118.** People questions are section 2 of `design/0102/`, provisional until tested: one tap
  answers with Undo and no confirm, email sources labelled amber (103), a new person provisional
  until a second mention or his confirmation, and misspellings handled in "Someone I know…": a
  search of everyone in the row, prefilled with the name, then one typo question (a hidden alias,
  or a shown "also" name).
- **119.** A person view is reached three ways: a name tapped in chat or on a card, the assistant
  answering with a link, and a People tab with search. The tab is kept despite screens retiring
  for chat, because it must work when the assistant is down or slow: search and the person view
  read local records with no model call.
- **120.** The person view is section 3 of `design/0102/`: read-only with Undo and "Wrong?" on its
  links, a source chip on every fact, and roles, keep-in-touch cadence and project or goal links
  left to the people data prep ([0108]). The People tab searches locally by text and spelling
  closeness, one matcher shared with "Someone I know…" (118), never semantically: questions in
  plain language go to the assistant.
- **121.** A `USER` item in a `~/life` index is proposed once as a native task, keyed by its hub
  id, and once accepted omni-me owns its status and ignores that id in the index. The topic's
  next session closes the entry: SessionStart asks the server which ids were taken over (about a
  2 s timeout, skipped with one line when unreachable), and the session marks them `cut`, "moved
  to omni-me". omni-me never writes the repo; the server keeps a receipt until a pushed index
  shows the cut. Revises 90 for `~/life`, whose indexes go unrefreshed for months.
