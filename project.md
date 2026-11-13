# Project: omni-me

A personal life-operating-system app — journal, routines, and finances over one
event-sourced store, with an LLM deriving structure from raw notes. Tauri v2
(Android + Linux desktop) · Dioxus + CodeMirror 6 · SurrealDB · Rust throughout.
Open-core: the bank-free engine is public, a private overlay carries real
sources and credentials.

**Status (2026-11-12):** **v1.2.3 is live and in daily use.** The go-live sequence
finished 2026-11-11: server 1.2.3 on the box, the agent running there, both devices on
1.2.3, the Finances tab on, the historical ledger (11,048 transactions) and the document
corpus imported, and mail and bank feeds running from 2026-11-01. Small fixes now batch
into 1.2.4 (`tasks.md` § Now). Cycle 4 is closed out: `tasks.md` was reset and memory
pruned that day (the pre-reset file is `git show 7ef0e55:tasks.md`). Cycle 5 has begun.
The narrative below this point is the September record and is kept as history.

Work now follows a **three-item sequence set by the user 2026-09-05, taken one at a
time: 1. feedback capture · 2. generalization · 3. AI/LLM/ML integration.** Items 1 and
2 are done; **item 3 is designed (2026-09-07) and building** — Phases 0 through G are in.

**The document archive is the next block, planned 2026-09-11 and started the same
day.** The user reopened finance and named six threads; the planning session found they
share one blocker, and it is not finance — nothing in the system can represent a
document that is not a transaction. `ExtractionResult` is `postings`/`commodity`/
`total`, while the pipeline around it is domain-neutral, so serving another domain is a
new output schema rather than a new pipeline. **The archive became the spine**: email
ingestion and finance statements become producers into it, and finance propose actions
drop to a separate, smaller, later item.

Two decisions shape it. **Extracted fields are allowed from a model and are flagged** —
each carries where it came from and whether anything verified it, folding by origin so
a human correction survives any later machine pass. That is the honest answer to a trap
the project already hit: a model once put "HAND WASH" in `commodity` with the arithmetic
perfect, and every guard here inspects numbers while none inspects whether a field means
what it claims. And **a field cannot be corrected without the document visible beside
it**, which turned out to matter more than it sounds — the existing viewer renders
images and iframes PDFs, so the 276 CSVs in the first corpus have no viewer at all.

The survey also found three things the code does not say. Retrieval is already
catalogue-driven, so a new record kind inherits search, embedding and the assistant's
read verbs for free. **Blob bytes do not sync** — the on-device store is a 200 MB LRU
cache, not a replica, so documents live in exactly one durable place and a server backup
became a dependency of the real backfill rather than a chore. And capacity, which the
user had flagged as a worry, is measured and not a constraint: 29 GB free against a
233 MB corpus, on a box currently holding 71 MB.

**The assistant can read (2026-09-08).** Phase B put the four read verbs behind a
multi-turn loop with cost instrumentation, and it works end-to-end against a real
endpoint: asked what he had written about a rent notice, it discovered the record
kinds, searched, read a journal entry and a note, and answered citing both. Free-form
verb selection scored 10/10.

**It proposes; the user disposes (Phases D–G).** There is no write verb. `propose`
records an intention over notes, beliefs and routines, and nothing reaches the log
until he accepts it — at which point one approval and the events it authored go out in
a single batch. ⛔ The journal is permanently absent from that list, and that is a
product invariant rather than a phase it is passing through.

⛔ **What it does not have is initiative about him.** Beliefs are proposed **only when
asked for** (his decision, 2026-09-10). The two wider policies considered — propose
whenever a durable pattern is noticed, and propose only on repeated independent
evidence — are **deferred, not dropped**, and neither is built. ✅ Of the two, the
**second is preferred and is role B's intended long-term shape** (2026-09-16); it
needs a per-role turn budget and a second scheduled question first, both in `tasks.md`. That is why `Trigger`
is an enum with one variant rather than a comment: an absent policy should be a
declaration someone can review, not a silence. ⚠️ Nothing in this build volunteers an
opinion about him, and "the assistant can write" must not be read as saying otherwise.

The phase's real decision was one it did **not** take. Reaching for `RecordType` — the
only declaration mechanism the codebase has — would have meant widening a permanent,
unversioned event payload. Reading why it was built ruled that out: it is a *document
form* schema, describing frontmatter keys so the client can render an editor, and it
cannot express a routine, which is three tables and no prose. The assistant instead
reads a separate catalogue that lives in code, because every table it names is created
by a projection in code and a declaration pointing at a table that cannot exist would
be genericity in name only. The two compose rather than one absorbing the other.
Building it also surfaced four traps that a design review would not have — including
BM25 scoring a legitimate match at exactly zero on a small corpus, and search
highlighting leaking its markers into a user-facing answer.

**Item 3 is planned, and its original premise turned out to be wrong** (2026-09-07,
design session, no code). It was filed as "re-examine `DocumentExtractor`/Gemini"; an
inventory found the LLM surface has **no reachable consumer at all** — the text path
lost its UI trigger in July, and extraction's only call sites sit behind the deferred
finance feature — so there was nothing to re-evaluate. The user reframed it into the
real question: he is overwhelmed and holds too much in his head, and wants **an
assistant with reach across the whole app**, growing as more of his life lands in it.
Three decisions carry the design. The model's tool surface is a **fixed verb set over
declared record types**, because tool-calling accuracy degrades past roughly fifteen to
twenty tools — so a tool per feature would rot exactly as the app grew, while data-driven
verbs stay constant forever. The assistant is **a headless omni-me device** with its own
database and identity, syncing like the phone, which the embedded store forces and which
keeps an agent failure from taking sync down with it. And **autonomy is earned rather
than granted**: it proposes, he approves, and behaviours are promoted per action type —
a lifecycle that doubles as the audit trail, the personalization corpus and the model
eval set. Hosting is rented and open-weights only; buying hardware is out because he has
no permanent address. Full design and its research in
`~/.claude/plans/lets-plan-how-to-sprightly-hartmanis.md`.

**Item 1 is done** — both stages built and verified 2026-09-05. Problem reports are
captured in-app as a `FeedbackCaptured` event carrying screen and build context, read
back over `GET /feedback`, and a diagnostic ring buffer attaches the error trail. Its
two device-only legs (cross-device end-to-end, a real panic) are deliberately held for
the next release test pass, to be tested together rather than piecemeal.

**Item 2's structural work is done** (2026-09-06). The narrow framing — three hardcoded personal
strings in the public engine — was replaced by the real question: omni-me is meant to
become a system other people run and customize, without inheriting one person's
preferences. Twelve decisions settled, the load-bearing ones being that client
customization must be data while server customization may be code (it follows from
fork cost, not taste), that journal becomes one instance of a **declared record type**,
and that enforced built-ins are legitimate but must be *published*. What omni-me
insists on is now a document a second user can read before committing a weekend:
[`docs/src/invariants.md`](docs/src/invariants.md), published as an mdbook site.
Build phases A (config foundation), B (inert feature toggles) and C (record types) are
in `tasks.md`. **All three are built.** What remains of item 2 is trailing rather than
structural: a preset picker UI, extra presets, record-type export/import.

**Phase A is built** (2026-09-06). Configuration is now two layers — an event-sourced
value shared across devices, and a local override file that never syncs — resolved
`device → shared → built-in default`, with the settings screen naming which layer won.
Startup reads the materialized table before registering projections, which is the seam
Phase B needs. The six feature keys ship **inert**: they record and sync, and nothing
reads them yet. Appearance became the first key that does something, at the user's
request: a light theme alongside the original dark one, and a choice of six accent
hues. Blue was never a decision — it came from the Obsidian theme the first build
referenced — so it is now data like everything else. On-device confirmation and the
two-device override check are held for the next release test pass, with the same
reasoning as item 1's device-only legs: tested together rather than piecemeal.

**Phase B is built** (2026-09-06), which closes the one deadline this year carried:
a feature shipped without a toggle is a retrofit owed later, so the map had to exist
before the next feature did. Switching a feature off now makes it inert — no tab, no
projections, no schedulers, and commands that refuse — and turning it back on replays
the log to rebuild what it held. That round trip is the payoff for event sourcing, and
it is why "off" could be made non-destructive at all.

The map is **enforced in three places rather than documented in one**, because a
convention drifts: a closed `Feature` enum whose exhaustive matches break the build at
every site that must decide about a seventh feature, a registry that fails its own test
if a projection is claimed by nobody, and — the write side — an `EventType` that cannot
be added without naming the feature allowed to author it. That last one means the
guard sits at the three shared append tails instead of at seventy commands, so write
paths that do not exist yet inherit it. Only outbound HTTP needed hand-placed guards,
and a source-scanning test holds that line the way three others already do here.

Two things surfaced that the design had not: the projection behind journal entries
serves **three** features rather than two, because LLM results land in the same tables;
and the hledger journal writer reads the transaction projection's table while being the
only handler of exchange rates, so dropping it would have removed currency conversion
rather than a file. One gap is recorded rather than fixed — the server does not read
config, so switching auto-import off on a client does not stop the box fetching; that
is what the per-source pause controls are for, and it is written down in
[`docs/src/features.md`](docs/src/features.md) rather than left for someone to discover.

**Phase C is built** (2026-09-06), and with it the promise `invariants.md` makes to a
second user — *your data shape is yours, the screens are the app's* — stops being half
broken on the screen where it mattered most. The daily journal's three reflection
prompts were compiled into four independent places, so a second user inherited one
person's journaling framework and could only escape it by forking. They now come from
a **declaration carried as an event**, so the shape of a record syncs and replays like
any other change, and journal is one instance of that rather than a special case.

The whole phase turned on one migration risk, because a year of entries had already
been written against the old shape. Completeness drives auto-close, auto-close makes an
entry read-only, and the generalized check would return `true` for an empty required
list — so a naive rewrite would have silently closed the entire back catalogue. Three
things hold that line: completeness fails closed on an empty list, the preset seeded on
an install that already has journal entries is the reflective one rather than the
minimal one, and a gate test replays a corpus of awkward real frontmatter both ways and
asserts the verdict is identical. That test was **deliberately sabotaged to confirm it
fails** before being trusted.

What is not built is a screen for editing a declaration, so replacing the preset means
emitting the event by hand. `docs/src/invariants.md` therefore reads **(partly today)**,
which is the contract being checked against rather than back-fitted.

⛔ **Finances are deferred indefinitely** (user, 2026-09-05). This **supersedes** the
earlier "offline until statement import beats the system it replaces" gate rather
than clearing it: the user stopped using the section, and there is no intent to
reopen it. Only the *state* survives — the finance tab stays offline, both bank
auto-import sources stay off, and categorization stays deferred to `Unmatched`. As of
Phase B this has a mechanism rather than a convention: `feature.finances` switches the
whole section off, which is what the user intends to do on their own device.
What was built before the stop still stands: every import runs through a parser that
accounts for each line it reads, and rendered-PDF statements parse in both layouts
and check themselves against the totals they declare, verified across 136 real files
with zero failures. Journal and routines are unaffected and in daily use.

**The archive block is complete, and the next one is validation (2026-09-14).** Phases 1–5
are in, the last of them being notify-and-route, which the user widened from the archive to
**every approval queue on every feature** — one count in `core::approvals`, rendered as nav
badges and as a reminder on the assistant screen. Two silent-failure classes were closed with
it: an axum handler returning `Err(String)` was serving **HTTP 200**, and `store::read`
truncated long bodies without saying so.

What the block ends on is a measurement problem rather than a build one. **Model selection
was designed and nothing was benched** — deliberately, because a fair test cannot be designed
while the system underneath it moves, and it has now stopped. The design found that
`build_llm_client` had never consulted the per-role config at all, so `[llm.batch]` and
`[llm.structurer]` parsed, validated, and were silently ignored; every role had been running
on the interactive model. It also found the role table itself was stale — two seats
documented as having "no caller" both had one. **No seat is treated as decided**, including
the interactive one: the user's position is that the incumbent may well win again but is
re-tested against the app as it now stands, not inherited.

The validation pass that follows is the one deferred all through this block — real data, a
dev sync server, and the on-device work — now unblocked on both sides it was waiting for.
Its shape is set by one clarification worth recording: *"not connected to my actual data"*
was always a rule about **writes**, junk entries that would have to be cleaned, so a clone
satisfies it exactly. The runbook was `tasks.md` § ▶ RUNBOOK (now `git show 7ef0e55:tasks.md`), and it opened with an isolation
guard rather than a test, because a clone of the live database is indistinguishable from
production from the inside.

**Last Updated:** 2026-11-12

> **What's next lives in [`NEXT.md`](NEXT.md)** — the next action and the
> decisions in force, rewritten at every completion. Open work lives in
> [`tasks.md`](tasks.md). This file is current state only.

## Decisions in force

Rulings carried out of the pre-reset `tasks.md` (2026-11-12), one line each. They are
the user's unless marked "mine". The reasoning behind each is in that file at
`git show 7ef0e55:tasks.md` (search the topic), in `docs/src/`, or at the edit site.
These are settled. Do not re-open one without a new reason.

**Product shape**
- The journal is user-authored only. The assistant reads and cites it and can never
  propose a write to it (`NEVER_PROPOSABLE`). This is an invariant, not a phase.
- Approval lives with the domain. The assistant inbox holds only proposals with no other
  home. A badge routes, it never decides. Never merge review queues into the inbox: the flows
  share an event shape, but the review UI is the whole value (a receipt needs editable lines,
  a re-run and a correction flow, which a generic card cannot carry).
- Autonomy is earned per action type and only the user grants it. Irreversible actions
  (deletion, purge, `transaction.clear`) can never be granted. A purge confirms once per
  group, listing every document, each one sparable.
- Beliefs are ask-only. The preferred long-term shape (proposing only on repeated
  independent evidence, run by role B) needs a per-role turn budget and a second scheduled
  question first. `belief.record` is refused until the run has listed a date range reaching
  the last 30 days. The daily check-in proposes retiring beliefs that no longer hold, and
  it is off by default.
- The archive is a filing cabinet, not an inbox: no badge. Emails render in the archive.
  It is not a mail client. The mailbox is never written to (`EXAMINE`). Gmail write-back is
  the stated destination once trust is built: deferred, not rejected.
- Where a clean separation can be defined, define it (wipes, isolation, imports).
- Task and project records come before estimation calibration. Nothing task-shaped exists yet.
- Finance actions: everything except delete and merge. `transaction.update` reaches the
  description and date only. `transaction.clear` requires a cited statement.
- Notes: `note.append`, `note.rename` and `note.revise` exist. `note.update` was rejected.
  `note.revise` matches an exact, unique text anchor, resolves at approval, and logs the
  finished body. Never make it fuzzy, and never move resolution into the projection.
- `routine.complete` and `routine.skip` can be proposed, because the journal is readable
  for exactly this. There is no partial batch approval (`BATCH_LIMIT` 12).
- The release is the assistant, the archive and finances together. His sign-off on UX
  and performance is the gate.
- A new day opens the journal on today. The same day restores the date last viewed.

**Assistant engineering**
- Keep our Rust loop, with no framework (`ASSISTANT_RESEARCH.md`). A full rewrite is
  acceptable if it is markedly better. Quality outranks token cost. Specialists come per
  feature, the ledger first.
- A small fixed verb set over declared record types. Changes are proposed with a reason.
- The turn budget is tunable within 3–50 (the floor is the halting guarantee). `propose`
  sits outside the budget. The final turn withholds tools, so a reply is guaranteed.
- The app writes an outcome line under every reply, built from its suggestions and never
  from the prose.
- Chats untouched for 30 days auto-archive, and archived chats auto-delete after 90.
  Check-ins are archived, then deleted after 14 days. A thread with an undecided
  suggestion is never deleted.
- Waiting indicator: an animation now, real step-by-step progress after the merge.
- Ledger grounding: his `CONVENTIONS.md` (paisa ledger, ⛔ never in this repo) is the
  source. Known accounts are the ledger's, plus those declared, plus any the note names.
  I draft the live conventions note and he reviews it.

**Models and benches** (`MODEL_BENCH.md`, `MODEL_THRESHOLDS.md`)
- Hosting is DeepInfra, open weights only, with US jurisdiction an accepted risk. Closed
  weights are refused unless `allow_closed_weights` is set.
- One model per job, chosen statically by benchmark, never by a runtime orchestrator.
  Re-evaluate every 3–6 months. The synthetic bench is a bootstrap: shadow mode and the
  correction log supersede it.
- Thresholds and tie-breaks are written before a run. A saturated instrument is published
  as unmeasured. A winner is switched on dev only. No hand-labelled gold set (his time).
- Sampling: roles B, C1–C3 and D at temperature 0; role A at the provider default.
- Spend the OpenRouter credits generously on screening. The deciding run goes direct.
- R27's account side is his. No list of affected runs exists, by his decision.
- The triage seat stays in shadow mode until real weeks score it. It never gates archiving.
- Do not tune the extractor prompt against the four sample receipts. `json_schema` beats
  `json_object`: a loud 400 beats a wrong 200.

**Mail, import and reconciliation**
- Never add a vendor to fix a missed receipt. The sender gate is inverted and the
  classifier decides (overlay `GATE_INVERSION_DESIGN.md`). Candidate group keys were
  refused. A printed order reference outranks the model's reading.
- Newest wins, because later documents are more authoritative. Any scoring respects
  document authority first.
- A receipt's stated total takes precedence. Never fetch a linked page for missing items.
- Duplicates are dropped only on a shared Message-ID, otherwise flagged. The two halves of
  an own-account transfer are joined. A receipt with no total becomes one transaction.
- Matching an authorisation plus its adjustment, and cross-currency reimbursements: dropped,
  matched by hand. Do not re-raise either unprompted.
- Bank feeds are the long-term source. Statements are for reconciliation and closing.
- Old mail is archived on live by a slow backfill, with no financial extraction before the
  imported statements. Cursors and the backfill both start 2026-11-01.
- Retention: one category per email from a fixed list, chosen at intake. The category's
  timer decides unless a topic tag he set carries a longer one. The clock counts from the
  email's own date. Social mail is not archived. The reader tags area of life only.
- A draft dated after today cannot be committed from review. An empty batch reaches review
  only when it is a receipt.
- Encrypted PDFs: every `pdf_password*` secret is tried. The server serves a decrypted copy,
  so the phone never holds a password. A multi-photo receipt is one combined PDF. Photos
  shrink on the device before upload.

**Data and operations**
- Live is the pause. Every live step waits for his go, one at a time. Dev is mine.
- A wipe is a server route (preview, then confirm), generic over features, with documents
  refused and a snapshot taken first. Devices follow a wipe. A cursor reset is its own route.
- Stay on the CX22, with Cloudflare R2 (free tier) as the off-box copy of the blobs. Resize
  only on a trigger: under ~500 MB available, or an OOM kill.
- Dev's agent stays stopped. It runs only while live's agent is idle.
- Fixes batch into the next release, not one release per fix.
- If config and the instance marker disagree, the server refuses to boot. The live box and
  his real phone are outside the dev grant, always.
- On a significant failure, stop, and write down what stopped it and what came next.
- The phone-width layout sweep, including the gutter check, is a CI gate.
- The ledger-classification rulings (F2, 2026-11-11) live in paisa's `CONVENTIONS.md` and
  live's "Ledger conventions" note, not here.

**Notifications** (design session, 2026-11-12; `DESIGN_PREREAD.md` Part 1)
- Three tiers by what happens if he never sees an item: critical interrupts, review rolls
  into one daily summary plus the badge, ambient never notifies.
- Critical means missing it costs money or leaves the record wrong, and waiting makes it worse.
- Finance mapping. Critical: a source needing a code or reauth (at once); a source erroring
  for 24 h; a statement that disagrees with the ledger; a large charge; a new-merchant charge;
  money due with a penalty date (the day before). Review: proposed batches and transactions,
  unmatched (its own count, apart from review), the purge queue. Ambient: duplicate flags,
  recurring-charge detection, the check-in.
- The large-charge threshold is a setting, and the new-merchant alert has its own on/off.
- Critical's budget is measured, never enforced: target ~3 a week. Over it, alerts still go,
  and the next summary asks whether to demote the noisiest kind.
- Quiet hours (default 22:00–08:00, adjustable) hold every critical alert until morning. A
  setting lets charge alerts through at night, off by default.
- Review reaches him as one silent daily notification of counts, built by code (default
  19:00, adjustable), skipped when nothing is new, tapping through to the tab. It does not
  depend on the check-in.
- Delivery starts with ntfy self-hosted on the box and the ntfy app on the phone, with no
  change to omni-me's app, chosen as the prototype. UnifiedPush or Google push (FCM) into
  omni-me's own app stays open as a later upgrade of the last mile only.
- Build order: (1) ntfy, source alerts and quiet hours; (2) the summary, with unmatched split
  from the review count; (3) budget measurement; (4) each new detector, ruled on its own.
  Slices 1–2 lead Cycle 5's build work and do not wait for Part 2's contract. Building
  starts once Part 2 has been discussed.

**Agent framework** (design session, 2026-11-12; `DESIGN_PREREAD.md` Part 2)
- The haphazardness is: no common shape across jobs, results he does not fully trust, jobs
  in odd places, and no clear, visualisable layout of how the agents are built and composed.
  He wants to understand how modern agentic systems (Claude Code, Codex) are composed, learn
  from them, and write mylearnbase posts about omni-me's agent structure.
- Part 2 splits: 2b, an architecture study (how Claude Code/Codex-style agents are composed,
  omni-me's agents mapped and diagrammed, run as a learning thread), comes first. 2a, the job
  contract, registry and placement rule, is decided after it. Part 1's build waits for both.
- Right after the study: one session on logbooks and per-feature page demos as proof, then
  the logbook backlog (mylearnbase Cycle 7 has a January deadline).
- Codebase scaling (ruled in the study, 2026-11-13): no per-area "process owner" agents. Nothing
  was named that one adds beyond area knowledge and tests, and every handoff is a cold start plus
  a lossy report. Instead, area traps, rulings and past failures go in path-scoped Claude Code
  rules (loaded when matching files are touched), procedures in skills, and the test layers stay
  as they are: module tests for development work, the benches for the runtime agent.
- Those rules never reach the public repo: synced and project-scoped like memory. Mechanism:
  `tasks.md` [0083].

## Where the history went

Everything through the v1.0.0 cut — the full session log (Session 1 → the Cycle 4
pre-v1 review), the Cycle 1–3 phase records, and the May-2026 Phase 2/3
known-limitation snapshots — is archived verbatim at
[`.archive/v1.0.0/project-history.md`](.archive/v1.0.0/project-history.md).
Completed tasks and resolved friction are at
[`.archive/v1.0.0/tasks-completed.md`](.archive/v1.0.0/tasks-completed.md).

The archive is **historical**: dates and "known gap" claims in it were true when
written and many were resolved later in Cycle 4 without the original text being
updated. Read it for how the project got here, not for what is true now.

## How the project runs

Six-session process per
[`../../setup_files/PROJECT_PROCESS.md`](../../setup_files/PROJECT_PROCESS.md)
(Initiation → Research → Architecture → Planning → Implementation → Code Review),
repeated per cycle. Cycles 1–3 shipped the MVP, the daily-usable
Obsidian-replacement, and the budget feature; Cycle 4 is polish, deploy, the
open-core split, and v1.

Dogfooding is the test harness — real daily friction is the primary bug-finder,
and it collects in `tasks.md` § *From daily use* to be triaged into
whichever phase is live.

## Standing at the v1.0.0 cut

- **Verification:** 798 tests green (645 core+server, 85 frontend, 68 app),
  clippy clean across all four configs, `cargo fmt --check` enforced in CI.
- **Data:** the box holds ~12.3k events from the clean real-data re-import
  (2026-08-30); reconcile is byte-faithful against `ledger bal`.
- **Delivery:** public CI on every push (free, unlimited — public repo); the
  private overlay builds and signs releases and deploys the server over the
  tailnet, health-gated with auto-rollback.
- **Branch gate:** the public repo carries GitHub rulesets `main-protection`
  (no deletion, no force-push) and `release-tags` (`v*` immutable). The private
  overlay is on a free plan where GitHub offers no gating at all, so it enforces
  the same two guarantees with a tracked `scripts/git-hooks/pre-push`.

## Key documents

| File | What it holds |
|---|---|
| `NEXT.md` | Next action + decisions in force (≤40 lines, rewritten at completion) |
| `tasks.md` | This cycle's open work only, under a size cap; reset each cycle |
| `architecture.md` | Technical decisions with rationale |
| `research.md` | Session 2 research findings |
| `UI_WORKFLOW.md` | How to develop the UI (`dx serve` + Playwright) |
| `SOURCE_REAUTH_DESIGN.md` | App-entered OTP re-auth for bank sources |
| `SUBPROCESS_SOURCE_CONTRACT.md` | The plugin contract for data sources |
| `docs/` | mdbook published to GitHub Pages; `src/invariants.md` is what omni-me insists on and why |
| `logbook/` | Published write-ups of shipped work |
