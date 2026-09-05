# Project: omni-me

A personal life-operating-system app — journal, routines, and finances over one
event-sourced store, with an LLM deriving structure from raw notes. Tauri v2
(Android + Linux desktop) · Dioxus + CodeMirror 6 · SurrealDB · Rust throughout.
Open-core: the bank-free engine is public, a private overlay carries real
sources and credentials.

**Status:** **v1.0.0 tagged 2026-08-31; v1.0.3 published.** Cycle 4 (Polish →
Stable v1) is at its close-out: the pre-v1 code review is closed, Phase 6's
branch-gate + version stamp are done, and the **OTA update round-trip is proven
on both mobile and desktop**. The **final go-live wipe ran 2026-08-31** — the box
was emptied to a genuine clean slate (`/data` 21M → 64K), every snapshot and local
backup deleted, and real data put back by a **fresh import from the canonical
ledger + vault**, reconciling to **0 diffs** against `ledger bal` and seeding the
box with 12 277 events. **Both personal devices are installed and syncing** — the
phone and `surface`, the Linux desktop — so the app is in real daily use.
**v1.0.4 is released** (published for both platforms), carrying a mobile
prose-typing fix for the note editor and a CI change that makes the desktop
AppImage runnable on older glibc. What remains is confirming the 1.0.3 → 1.0.4
OTA update on both devices, the brokerage reconnect, and box auth.

**The finance tab is deliberately offline** (user, 2026-09-04) until statement
import here beats the system it replaces, and **both bank auto-import sources are
off**. Journal and routines are unaffected and in daily use. Progress since: the
old comma-splitting importer is gone and every import runs through a parser that
accounts for each line it reads; statements that arrive as rendered PDFs now parse
in both layouts and check themselves against the totals they declare, verified
across 136 real files with zero failures. Categorization stays deferred to
`Unmatched` — amounts are externally verified, categories by nothing.

**Last Updated:** 2026-09-05

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
| `logbook/` | Published write-ups of shipped work |
