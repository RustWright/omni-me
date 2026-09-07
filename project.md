# Project: omni-me

A personal life-operating-system app — journal, routines, and finances over one
event-sourced store, with an LLM deriving structure from raw notes. Tauri v2
(Android + Linux desktop) · Dioxus + CodeMirror 6 · SurrealDB · Rust throughout.
Open-core: the bank-free engine is public, a private overlay carries real
sources and credentials.

**Status:** **v1.1.0 released 2026-09-06** — the server deployed to the box first
(`sha-9a0b1dd`, health-gated), then the tag cut on a green CI matrix and the APK +
AppImage published to `/updates`. On-device verification is the open item, and its
first step is perishable: a still-v1.0.5 client syncing against the new server can
only be observed before either device takes the update. The release carries items 1
and 2 of the three-item sequence — feedback capture, and generalization through
record types. **v1.0.5 was in real daily use** on both personal devices —
the phone and `surface`, the Linux desktop — syncing against the box. Cycle 4's
build work is done: the pre-v1 review is closed, the branch gate and version stamp
are in place, and the OTA round-trip is proven on both platforms. What the cycle
still owes is its **close-out** — the curiosities→concepts pass and a memory prune.
**All three on-device confirmations are closed** (user, 2026-09-05, from real use
rather than a staged test): note and journal body edits propagate across devices,
ledger transaction edits do too, and Android predictive text commits on space.

Work now follows a **three-item sequence set by the user 2026-09-05, taken one at a
time: 1. feedback capture · 2. generalization · 3. AI/LLM/ML integration.** Items 1 and
2 are done; **item 3 is now designed** (2026-09-07) and its build has not started.

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

**Last Updated:** 2026-09-06

> **What's next lives in [`NEXT.md`](NEXT.md)** — the next action and the
> decisions in force, rewritten at every completion. Open work lives in
> [`tasks.md`](tasks.md). This file is current state only.

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
and it collects in `tasks.md` § *Running friction log* to be triaged into
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
| `tasks.md` | Open work, the roadmap to v1, and the running friction log |
| `architecture.md` | Technical decisions with rationale |
| `research.md` | Session 2 research findings |
| `UI_WORKFLOW.md` | How to develop the UI (`dx serve` + Playwright) |
| `SOURCE_REAUTH_DESIGN.md` | App-entered OTP re-auth for bank sources |
| `SUBPROCESS_SOURCE_CONTRACT.md` | The plugin contract for data sources |
| `docs/` | mdbook published to GitHub Pages; `src/invariants.md` is what omni-me insists on and why |
| `logbook/` | Published write-ups of shipped work |
