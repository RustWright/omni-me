# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `RULINGS.md`, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`), rationale in
`docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — ask his go to release PR 9 to live (it carries the backup)
[0006] backup is built and proven on dev (`docs/src/backup.md`); live needs his secrets step and
the release. PR 9 also carries [0106], [0107], [0114] (proven on dev) and [0118] (after release,
probe live with `~/omni-dev-tools/ws_transfer_dupes_probe.py`). The next build is his to pick; [0109]
leads the [0074] chain.

⛔ Live is the pause: every live step waits for his go, one at a time. None of the three builds is on
live: live deploy, app release and the task seed import each wait for his go.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`); the mock demo
page is the one exception. Renders use the cached headless shell
(`~/.cache/ms-playwright/chromium_headless_shell-*/…/chrome-headless-shell --screenshot`).
The dev agent runs on this machine in Docker, since the box cannot fit it beside live's: stopped
between uses (ruling 45), `docker start omni-me-agent-dev` resumes it against dev :3001.

## ⛔ Waiting on him
- **[0118] until the fix ships:** skip lone "Money transfer: from…" rows in Reconcile.
- **[0006] on live:** he runs `python3 ~/omni-dev-tools/backup_secrets.py live` in his own terminal
  (hidden prompts; prints key names only), then the PR 9 release.
- **[0115]:** go for the task seed import, hook wiring, and the LEARN feed address.
- **Calendar leftover (his, in Google):** delete the biweekly bookkeeping series in
  `efeogheneerhie.o` (didn't stick). Tim Sarvis's 2027 double: his call, not an issue.
- [0013] is parked until a real blank delivery item appears in review; ask with it on screen.
- **Promoting ~11 general feedback notes to `~/.claude/rules/`:** the always-loaded tier is at 315 of
  320 lines, so promotion means compressing something. His call on what.
