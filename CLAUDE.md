# Claude Code Instructions

This project follows the structured process defined in `PROJECT_PROCESS.md`.

## Session Management

See `~/.claude/CLAUDE.md` for git pull/push/submodule sync protocol — that file governs all session start/end git operations.

Project-specific session steps:
- **Start:** Read `project.md` to find current state and next session. Confirm with user before proceeding. If resuming mid-session, read `tasks.md` and `architecture.md` for context.
- **End:** Remind user to run `/export`, save to `.log/` as `session-0X-[type]-cycle-Y.txt`. Update `project.md` session log. Update `tasks.md` if Session 4.

### Session Flow Reference
```
Session 1: Initiation     → Define goals, users, success, motivation
Session 2: Architecture   → Tech decisions, MVP scope, risk review
Session 3: Planning       → Break work into tasks (tasks.md)
Session 4: Implementation → Build with velocity
Session 5: Testing        → User writes tests, review, close cycle

First cycle: 1 → 2 → 3 → 4 → 5
Subsequent:      3 → 4 → 5 (repeat)
```

## AI Role by Session

| Session | My Role |
|---------|---------|
| 1-3 | Interview (one question at a time), propose options with trade-offs, document decisions |
| 4 | Orchestrate implementation, track progress, maintain velocity |
| 5 | Minimal scaffolding for tests, assist only when user is blocked |

## Key Files
- `project.md` — Persistent tracker, decision summaries, session log
- `architecture.md` — Technical decisions with rationale (created Session 2)
- `tasks.md` — Current cycle's task list (created Session 3, reset each cycle)
- `UI_WORKFLOW.md` — How to develop the UI (dx serve + Playwright MCP). Read before any UI work.
- `ui-checklist.md` — UI interaction checklist with test results
- `.log/` — Raw conversation exports
