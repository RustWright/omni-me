# Claude Code Instructions

This project follows the structured process defined in `PROJECT_PROCESS.md` (mirror of `setup_files/PROJECT_PROCESS.md`; the canonical source lives there).

## Session Management

**Session start:**

1. Run start-of-session sync per `~/.claude/CLAUDE.md` § Session Sync Protocol.
2. Read `project.md` to find current state and next session. Confirm with user before proceeding.
3. If resuming mid-session, also read `tasks.md` and `architecture.md` for context.

**Session end:** Follow `PROJECT_PROCESS.md` § End-of-Session Protocol. The 7-step recipe (export → name → save → update `project.md` → update `tasks.md` → commit → sync to parent) lives there as the **single source of truth** — don't duplicate it here. Step 7 defers to `~/.claude/CLAUDE.md` § Session Sync Protocol Step 2 for the parent-sync mechanics (covers both `.log/` and `.curiosities/`).

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
