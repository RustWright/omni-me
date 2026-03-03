# Claude Code Instructions

This project follows the structured process defined in `PROJECT_PROCESS.md`.

## Session Management

### At Session Start
1. Read `project.md` to understand current state and which session comes next
2. Read `PROJECT_PROCESS.md` if needed to refresh on session details
3. Confirm with user which session we're starting
4. If resuming mid-session, read relevant files (`tasks.md`, `architecture.md`) for context

### At Session End
When the user indicates they want to end the session or take a break or the session comes to its natural conclusion:

1. **Remind user to export:** Ask them to run `/export` command
2. **Provide filename:** `session-0X-[type]-cycle-Y.txt`
   - Examples: `session-01-initiation.txt`, `session-03-planning-cycle-1.txt`
3. **Confirm save location:** `.log/` directory
4. **Update project.md:** Add/update session summary in the Session Log section
5. **Update tasks.md:** If Session 4, ensure task statuses are current
6. **Suggest commit:** Remind user to commit changes if appropriate

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
- `.log/` — Raw conversation exports
