# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `RULINGS.md`, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`), rationale in
`docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — [0134] the assistant's person lookup and fact proposals (`tasks.md`)
[0132] is built and runs on dev and the S9 (1.2.13-dev); its leftover is in `tasks.md`.
**Dev credits and dev actions are approved per session**; ask again at the start.
UI: the static mock (`dx build` + a file server) beats `dx serve` for captures; drive
Playwright's bundled Chromium over CDP, three self-review passes before he sees it (his rule).
The box and the S9: MagicDNS fails inside WSL; use tailnet IPs (`tailscale status`).
Judgment calls: overlay `DECISION_LOG_2026-11-27.md` (both sessions).
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
  Deferred by him again on 11-27.
- **[0132] on the S9:** answer tonight's people questions there; 3 cards wait on dev data.
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
