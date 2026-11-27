# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `RULINGS.md`, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`), rationale in
`docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — [0132] evening people questions, then [0134] (`tasks.md`)
[0131] landed green and runs on dev; its leftover is in `tasks.md`. UI is checked in the mock
render (`UI_WORKFLOW.md`; Playwright MCP has no Chrome here, so drive Playwright's bundled
Chromium over CDP), with three self-review passes before he sees it (his rule).
**Dev credits and dev actions were approved for the 11-27 session only**; ask again.
Dev agent runs go through `~/omni-dev-tools/ritual_rerun_dev.sh <tag> <ritual>` (always stops it).
The box: MagicDNS fails inside WSL; reach it by tailnet IP (memory `reference-box-ssh-access`).
Judgment calls this session: overlay `DECISION_LOG_2026-11-27.md`.
⛔ Order: server before any client (`server/src/routes/sync.rs` refuses unknown event types).
⚠️ The next live deploy adds People to the server: its boot replays every event first (dev: 13,800
in ~2.5 min); the deploy's default 450 s health gate should hold.
Still to watch: [0118] when the next joined transfer's busy leg ages out
(`~/omni-dev-tools/ws_transfer_dupes_probe.py`). [0119] and [0120] wait for the next release (46).

⛔ Live is the pause: every live step waits for his go, one at a time. [0117]'s logcat watch is for
the next dev APK on the S9 (the dev phone), never the release on his daily phone.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`); fmt, clippy
and check in a capped cgroup are fine, and the mock demo page is the one build exception.

## ⛔ Waiting on him
- **[0127] Google Cloud setup, walked through live:** overlay `SETUP.md` § Google Calendar.
- **[0124] prompt texts:** the evening, weekly and monthly prompts in `planning/rituals.rs` are mine.
- **[0116] hub trial record (92):** how "acted" and "dismissed" are recorded is a design call.
- **[0128] at the live release (his call, 11-25):** decide how live's startup compaction gets room
  (planned off-box compaction, a bigger box, or leave it); options in `tasks.md` [0128].
- **[0123] on live:** his go to seed places and set Settings → Planning (`tasks.md` [0123]).
- **Public history:** an earlier public `NEXT.md` named part of his email and a contact; this
  rewrite drops both, but history keeps them. His call whether that matters.
- **R2 `dev/` folder:** delete it in the dashboard (dev's test run; dev's backup is off).
- **[0115]:** held until [0109] lands; then his go for the seed import, hook wiring, LEARN feed.
- **Calendar leftover (his, in Google):** delete the biweekly bookkeeping series that didn't stick.
- [0013] is parked until a real blank delivery item appears in review; ask with it on screen.
- **Promoting ~11 general feedback notes to `~/.claude/rules/`:** the always-loaded tier is at 318 of
  320 lines, so promotion means compressing something. His call on what.
