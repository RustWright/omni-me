# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `RULINGS.md`, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`), rationale in
`docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — [0109]'s last checks, then [0127] with him
[0125] is done and verified on the S9; [0124] is built, and dev runs it (`dev-0d29dfb-58ee6f1`).
Next: a dev-agent run of each ritual ([0124]) and `e152a79`'s check, both on the agent image
`omni-me-private-agent:dev-58ee6f1`, which needs his go to pull here. Then [0127] with him.
Judgment calls made unattended: overlay `DECISION_LOG_2026-11-25.md`.
⛔ Order: server before any client. A server older than its clients refuses the whole push batch
on an unknown event type (`server/src/routes/sync.rs`), and this push adds two.
[0127] (Google connector) waits for him: his Google Cloud setup, walked through live.
Still to watch: [0118] when the next joined transfer's busy leg ages out
(`~/omni-dev-tools/ws_transfer_dupes_probe.py`). [0119] and [0120] wait for the next release (46).

⛔ Live is the pause: every live step waits for his go, one at a time. [0117]'s logcat watch is for
the next dev APK on the S9 (the dev phone), never the release on his daily phone.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`); fmt, clippy
and check in a capped cgroup are fine, and the mock demo page is the one build exception.
The dev agent runs on this machine in Docker, since the box cannot fit it beside live's: stopped
between uses (ruling 45), `docker start omni-me-agent-dev` resumes it against dev :3001.

## ⛔ Waiting on him
- **[0128] before live's next deploy:** his go to compact a copy of live off-box (`tasks.md` [0128]).
- **Agent image pull:** his go for `docker pull …-agent:dev-58ee6f1` (the [0124] ritual run).
- **[0124] prompt texts:** the evening, weekly and monthly prompts in `planning/rituals.rs` are mine.
- **[0123] on live:** his go to seed places and set Settings → Planning (`tasks.md` [0123]).
- **Public history:** an earlier public `NEXT.md` named part of his email and a contact; this
  rewrite drops both, but history keeps them. His call whether that matters.
- **R2 `dev/` folder:** delete it in the dashboard (dev's test run; dev's backup is off).
- **[0115]:** held until [0109] lands; then his go for the seed import, hook wiring, LEARN feed.
- **Calendar leftover (his, in Google):** delete the biweekly bookkeeping series that didn't stick.
- [0013] is parked until a real blank delivery item appears in review; ask with it on screen.
- **Promoting ~11 general feedback notes to `~/.claude/rules/`:** the always-loaded tier is at 315 of
  320 lines, so promotion means compressing something. His call on what.
