# Project: omni-me

A personal life-operating-system app — journal, routines, and finances over one
event-sourced store, with an LLM deriving structure from raw notes. Tauri v2
(Android + Linux desktop) · Dioxus + CodeMirror 6 · SurrealDB · Rust throughout.
Open-core: the bank-free engine is public, a private overlay carries real
sources and credentials.

**Status:** **v1.0.5 released and in real daily use** on both personal devices —
the phone and `surface`, the Linux desktop — syncing against the box. Cycle 4's
build work is done: the pre-v1 review is closed, the branch gate and version stamp
are in place, and the OTA round-trip is proven on both platforms. What the cycle
still owes is its **close-out** — the curiosities→concepts pass and a memory prune.
**All three on-device confirmations are closed** (user, 2026-09-05, from real use
rather than a staged test): note and journal body edits propagate across devices,
ledger transaction edits do too, and Android predictive text commits on space.

Work now follows a **three-item sequence set by the user 2026-09-05, taken one at a
time: 1. feedback capture · 2. generalization · 3. AI/LLM/ML integration.**

**Item 1 is done** — both stages built and verified 2026-09-05. Problem reports are
captured in-app as a `FeedbackCaptured` event carrying screen and build context, read
back over `GET /feedback`, and a diagnostic ring buffer attaches the error trail. Its
two device-only legs (cross-device end-to-end, a real panic) are deliberately held for
the next release test pass, to be tested together rather than piecemeal.

**Item 2 is designed, not built.** The narrow framing — three hardcoded personal
strings in the public engine — was replaced by the real question: omni-me is meant to
become a system other people run and customize, without inheriting one person's
preferences. Twelve decisions settled, the load-bearing ones being that client
customization must be data while server customization may be code (it follows from
fork cost, not taste), that journal becomes one instance of a **declared record type**,
and that enforced built-ins are legitimate but must be *published*. What omni-me
insists on is now a document a second user can read before committing a weekend:
[`docs/src/invariants.md`](docs/src/invariants.md), published as an mdbook site.
Build phases A (config foundation), B (inert feature toggles) and C (record types) are
in `tasks.md`; **B carries the only real deadline** — it must land before the next
feature ships, or every feature added this year is a retrofit owed later.

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

⛔ **Finances are deferred indefinitely** (user, 2026-09-05). This **supersedes** the
earlier "offline until statement import beats the system it replaces" gate rather
than clearing it: the user stopped using the section, and there is no intent to
reopen it. Only the *state* survives — the finance tab stays offline, both bank
auto-import sources stay off, and categorization stays deferred to `Unmatched`.
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
