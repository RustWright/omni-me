# NEXT

**Next action: plan feedback capture** — item 1 of three the user set 2026-09-05. Plan first,
build second (defer-major-phases rule). The sequence, in order, one at a time:
**1. feedback capture · 2. generalization (decide *or* address) · 3. AI/LLM/ML integration —
detailed planning: how to integrate, host and securely use it for best value at least cost.**
Full framing of all three in `tasks.md` § The agreed sequence. Do not run ahead.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY** (user, 2026-09-05). The user stopped using the
  section. What exists ships as-is. **Do not start finance work, do not propose it, do not
  "just fix" an item.** This *overrides* THE BAR rather than clearing it — only the state
  survives: finance tab offline, both bank sources OFF, categorization deferred to
  `Unmatched`. The parser work that landed 2026-09-05 is done and verified; it stops there.
- **The "nothing new until finances work end-to-end" gate is SUPERSEDED** — it would have
  blocked the sequence above forever. Journal and routines are end-to-end and in daily use,
  which was the part that mattered.
- ⚠️ **NO LLM in the statement path**, and `DocumentExtractor`/Gemini is itself queued for
  re-evaluation — that re-examination *is* item 3. Do not treat the current LLM design as
  settled just because it exists and compiles.
- **Almost none of the statement work was bank-specific** (user's question, 2026-09-05).
  Layout shapes and checks generalize; column *words*, date formats and the password rule
  don't. Password = a named `[secrets]` entry. A live input to item 2.
- ⚠️ **"Nothing failed" ≠ "verified"** — a format with nothing to check clears every gate by
  offering none. `Verifiability` says so in words; never collapse it into a tick.

> **Public repo — fictional names only; real balances/institutions never enter it.** Institution
> detail lives in the overlay (`CONVENTIONS.md` is CANONICAL). The privacy guard is an **ingress
> filter, not an audit** — it sees only newly staged lines.

## Do NOT re-survey
**All three on-device confirmations are CLOSED** (user, from real use). **The Cycle 3 code
review owed-item was struck** — the pre-v1 gate was a full end-to-end read-the-code pass and is
closed, so it double-counted. Statement parser rules were validated against 136 real files,
twice. Backlog: `tasks.md`; `probe_realdb.rs`'s clippy lint predates you.

## Open threads
⚠️ **OOM-killed a shell this session** — 7.2GB RAM, 1.9GB swap: `CARGO_BUILD_JOBS=1`, one crate
at a time, never two cargo processes. · Statement upload is **untried against a live box** and
needs a `[secrets]` entry — finished work, not a task. · Curiosities→concepts pass owed at
cycle close · memory prune owed · `server_url` precedence question still open.
