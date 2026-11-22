# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `RULINGS.md`, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`), rationale in
`docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — confirm [0118]'s CI, then the OpenRouter privacy check ([0010])
[0118] (his live report: WS transfer halves booked twice) is fixed on PR 9's branches, public and
overlay, pending CI. Dev has no WS source, so its proof is the tests, then a probe of live after release
(`~/omni-dev-tools/ws_transfer_dupes_probe.py`). He chose to take [0010] after it, then R2 ([0006]).
[0106], [0107] and [0114] are built and proven on dev. The next build is his to pick; [0109] leads.

⛔ Live is the pause: every live step waits for his go, one at a time. None of the three builds is on
live: live deploy, app release and the task seed import each wait for his go.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`); the mock demo
page is the one exception. Renders use the cached headless shell
(`~/.cache/ms-playwright/chromium_headless_shell-*/…/chrome-headless-shell --screenshot`).
The dev agent runs on this machine in Docker, since the box cannot fit it beside live's: stopped
between uses (ruling 45), `docker start omni-me-agent-dev` resumes it against dev :3001.

## ⛔ Waiting on him
- **[0118] until the fix ships:** skip lone "Money transfer: from…" rows in Reconcile.
- **[0006] R2 account and [0010] OpenRouter privacy setting** (10 gates OpenRouter's next use, 107).
- **[0115]:** go for the task seed import, hook wiring, and the LEARN feed address.
- **Live's role D seat:** confirm it is zero-retention before the journal-tick job goes to live (107).
- **Calendar leftover (his, in Google):** delete the biweekly bookkeeping series in
  `efeogheneerhie.o` (didn't stick). Tim Sarvis's 2027 double: his call, not an issue.
- [0013] is parked until a real blank delivery item appears in review; ask with it on screen.
- **Promoting ~11 general feedback notes to `~/.claude/rules/`:** the always-loaded tier is at 315 of
  320 lines, so promotion means compressing something. His call on what.
