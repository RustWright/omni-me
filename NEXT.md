# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `RULINGS.md`, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`), rationale in
`docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — [0109], the planning build, as slices [0121]–[0127] ([0115] waits for it)
[0121] and [0122] are done (CI green on `0ce554c`). Next: [0123], plans and the planning
specialist, in a fresh session (it is [L]): read `core/src/assistant/specialist.rs` first, since
planning routes like the ledger does (112). Ruling 133 settles the calendar read path; the Google
setup is walked through live at [0127].
v1.2.8 is fully out: live server `sha-fb67e96`, agent `dev-acc868b`, app published to the updater
(he updates his daily phone himself). The backup ([0006]) ran its first live pass. Still to watch:
[0118] when the next joined transfer's busy leg ages out (`~/omni-dev-tools/ws_transfer_dupes_probe.py`).
[0119] (overlay `a9d79e4`) and [0120] (verified on the S9) wait for the next release (ruling 46).

⛔ Live is the pause: every live step waits for his go, one at a time. The task seed import still
waits for his go. [0117]'s logcat watch is for the next dev APK on the S9 (the dev phone), never
the release on his daily phone.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`); the mock demo
page is the one exception. Renders use the cached headless shell
(`~/.cache/ms-playwright/chromium_headless_shell-*/…/chrome-headless-shell --screenshot`).
The dev agent runs on this machine in Docker, since the box cannot fit it beside live's: stopped
between uses (ruling 45), `docker start omni-me-agent-dev` resumes it against dev :3001.

## ⛔ Waiting on him
- **R2 `dev/` folder:** delete it in the dashboard (dev's test run; dev's backup is off).
- **[0115]:** held until [0109] lands (his call: planning may revise task decisions); then his go for
  the seed import, hook wiring, and the LEARN feed address.
- **Calendar leftover (his, in Google):** delete the biweekly bookkeeping series in
  `efeogheneerhie.o` (didn't stick). Tim Sarvis's 2027 double: his call, not an issue.
- [0013] is parked until a real blank delivery item appears in review; ask with it on screen.
- **Promoting ~11 general feedback notes to `~/.claude/rules/`:** the always-loaded tier is at 315 of
  320 lines, so promotion means compressing something. His call on what.
