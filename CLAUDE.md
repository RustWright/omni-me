# Claude Code Instructions

This project follows the structured process defined in `PROJECT_PROCESS.md` (mirror of `setup_files/PROJECT_PROCESS.md`; the canonical source lives there).

## Session Management

**Session start:**

0. **`NEXT.md` is already in your context** — SessionStart printed it. Start there. Trust it about **decisions** (if it says a choice is settled, or names files as not-worth-re-surveying, skip them — that is permission, take it). Never trust it about **state**.
1. Re-verify live state (`git status`, read what you'll touch). Git sync itself is automatic (SessionStart hook) — don't run it by hand. Inherit decisions already agreed in prior sessions; don't re-derive settled context.
2. Read `project.md` to find current state and next session. Confirm with user before proceeding.
3. If resuming mid-session, also read `tasks.md` and `architecture.md` for context.

**Session end:** Logging and git sync are **automatic** (the session hooks — see `~/.claude/CLAUDE.md` § Session Sync): the transcript is rendered into `.log/`, `.log/` + `.curiosities/` are synced to the parent, and work is committed + pushed. Your jobs are the content updates in `PROJECT_PROCESS.md` § End-of-Session Protocol: **rewrite `NEXT.md` wholesale every sitting** (max 40 lines — decisions + next action, never a state snapshot), and at a phase boundary also update `project.md`'s session log and `tasks.md`. No `/export`, no manual parent-sync.

**Session model:** Six-session process per `PROJECT_PROCESS.md` § Process Flow (Initiation → Research → Architecture → Planning → Implementation → Code Review). The AI role per session is documented in `PROJECT_PROCESS.md` § AI's Role.

## Current Project State

- Check `project.md` Session Checklist for completed sessions.
- Check the Status field at the top of `project.md` for project state.
- If `tasks.md` exists, check for in-progress work.

## Key Files

- `project.md` — Persistent tracker, decision summaries, session log
- `research.md` — Research findings (Session 2 output)
- `architecture.md` — Technical decisions with rationale (Session 3 output)
- `tasks.md` — Current cycle's task list (reset each cycle)
- `reviews/` — Per-cycle code review findings (one file per perspective per cycle)
- `UI_WORKFLOW.md` — How to develop the UI (dx serve + Playwright MCP). Read before any UI work.
- `ui-checklist.md` — UI interaction checklist with test results
- `.log/` — Raw conversation exports (gitignored here; parent-synced)
- `.curiosities/` — Cycle-scoped curiosity captures (gitignored here; parent-synced)
