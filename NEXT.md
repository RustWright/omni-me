# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `RULINGS.md`, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`), rationale in
`docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — [0116], then [0124]'s rerun (his order, 11-25)
**Dev credits pre-approved** (his words, 11-25). [0126] is built (`c3d67c8`).
1. [0116] task surfaces, incl. the view's pause/stop/Start ([0125] built only Start on the reminder).
2. [0124]: rerun the rituals on an agent image with `1820d85` (`tasks.md` [0124] says what the
   11-26 run found). Images were dispatched 11-26 against `1820d85`: agent `dev-1820d85`, APK
   1.2.10-dev for the S9 (also carries [0126]'s taps to look at). Then [0127] with him; [0110] after.
Run the agent as `docker run --rm` by tailnet IP, not the old `omni-me-agent-dev` container
(stale image, MagicDNS). Judgment calls made unattended: overlay `DECISION_LOG_2026-11-2*.md`.
⛔ Order: server before any client (`server/src/routes/sync.rs` refuses unknown event types).
Still to watch: [0118] when the next joined transfer's busy leg ages out
(`~/omni-dev-tools/ws_transfer_dupes_probe.py`). [0119] and [0120] wait for the next release (46).

⛔ Live is the pause: every live step waits for his go, one at a time. [0117]'s logcat watch is for
the next dev APK on the S9 (the dev phone), never the release on his daily phone.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`); fmt, clippy
and check in a capped cgroup are fine, and the mock demo page is the one build exception.

## ⛔ Waiting on him
- **[0124] first run of a weekly or monthly ritual:** with no earlier run it fires on the first
  evening it can, any weekday (`scheduled::is_due`); on live that is the release evening.
- **[0128] at the live release (his call, 11-25):** decide how live's startup compaction gets room
  (planned off-box compaction, a bigger box, or leave it); options in `tasks.md` [0128].
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
