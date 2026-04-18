# Claude Code Instructions

This project follows the structured process defined in `PROJECT_PROCESS.md`.

## Session Management

See `~/.claude/CLAUDE.md` for git pull/push/submodule sync protocol — that file governs all session start/end git operations.

Project-specific session steps:
- **Start:** Read `project.md` to find current state and next session. Confirm with user before proceeding. If resuming mid-session, read `tasks.md` and `architecture.md` for context.
- **End:** Remind user to run `/export`, save to `.log/` as `session-0X-[type]-cycle-Y.txt`. Update `project.md` session log. Update `tasks.md` if Session 5 (Implementation).

### Session Semantics

"Session N" is a **phase of work** with a defined purpose, not a single sitting. A session may span multiple Claude Code invocations as context/token limits (or fatigue) require — the number only indicates ordering.

### Session Flow Reference
```
Session 1: Initiation      → Define goals, users, success, motivation (first cycle only)
Session 2: Research        → Feature + stack deep research (first cycle only)
Session 3: Architecture    → Tech decisions, MVP scope, risk review (first cycle only)
Session 4: Planning        → Break work into tasks; optional embedded research
Session 5: Implementation  → Build with velocity
Session 6: Code Review     → 4-perspective review, test-gap audit, fix cycle

First cycle: 1 → 2 → 3 → 4 → 5 → 6
Subsequent:          4 → 5 → 6 (repeat)
```

## AI Role by Session

| Session | My Role |
|---------|---------|
| 1-4 | Interview (one question at a time), propose options with trade-offs, document decisions |
| 5 | Orchestrate implementation, track progress, maintain velocity |
| 6 | Run 4-perspective parallel review, explore test gaps, drive agent-assisted fix cycle |

## Key Files
- `project.md` — Persistent tracker, decision summaries, session log
- `research.md` — Research findings (created Session 2)
- `architecture.md` — Technical decisions with rationale (created Session 3)
- `tasks.md` — Current cycle's task list (created Session 4, reset each cycle)
- `reviews/` — Per-cycle code review findings (one file per perspective per cycle)
- `UI_WORKFLOW.md` — How to develop the UI (dx serve + Playwright MCP). Read before any UI work.
- `ui-checklist.md` — UI interaction checklist with test results
- `.log/` — Raw conversation exports
