# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `project.md` § Decisions in force, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`),
rationale in `docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — the agent architecture study, `tasks.md` [0076]
The design session ran 2026-11-12: Part 1 is ruled, Part 2 is restructured (`project.md`
§ Notifications, § Agent framework). The order he set is `tasks.md` § Cycle 5 sequence. Open the
study in a fresh session as a **learning thread**: he wants to understand how agentic systems are
composed and to see omni-me's drawn, not to approve a finished write-up. The codebase-scaling
discussion runs inside it. ⚠️ He has not read `DESIGN_PREREAD.md`, so explain from zero.

## ⏳ Dated, independent of the sequence
- **11-16:** the weekly WS buys arrive whole: `tasks.md` [0001].
- **Then** release 1.2.4, his go: `tasks.md` [0002] holds the queue. Until then, fixes land on the dev
  branch and go onto that list, not onto live.

⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys, releases
and writes to live data are his to run (the permission check refuses mine), so hand him the command.
⚠️ A fast-forward of public `main` closes the draft PR (now #8). Open a new one after each.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`). Overlay: dispatch
from the paired branch, with `ci.yml -f public_ref=<full SHA>` before any build. Live tools: `~/omni-dev-tools/`.

## ⛔ Waiting on him
- **His decisions, `tasks.md` § Waiting on his decision:** N-to-1 matching [0003] · the two
  pre-November drafts [0004] · R2 [0006] · the Support purge [0007] · F3's open half [0008] · tap
  targets [0009] · R27's account side [0010] · tab order [0011] · monthly crypto [0012] · the
  estimate fallback [0013] · `server_url` precedence [0014] · journal timestamps [0015] · paisa
  parity [0016].
- **Promoting ~11 general feedback notes to `~/.claude/rules/`:** the always-loaded tier is at 315 of
  320 lines, so promotion means compressing something. His call on what.
- **The public-repo identity scrub**: authorised, at the end of a session only. `tasks.md` [0072].
