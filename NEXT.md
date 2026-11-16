# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `project.md` § Decisions in force, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`),
rationale in `docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — overnight unattended run, 2026-11-16 (his choice: sets A and B)
Work through in order, one commit per fix. Each fix: CI on the overlay (dispatch rules below),
then a dev build on the dev phone, driven over CDP (`~/omni-dev-tools/cdp/`) to verify on device.
A fix that passes both goes into the 1.2.4 queue, `tasks.md` [0002]. Log every judgement call in a
new overlay `DECISION_LOG_2026-11-16.md`; back up dev data before mutating it. Nothing on live.
- **A, visible (release candidates):** [0096] sidebar badges stale after a queue clears · [0030]
  citation chips show kind, not title · [0095] retention stale-list message · [0031] Ledger shows
  pre-wipe rows until Apply · [0033] mail photos unrotated · device check of `8a03d19` (zone stamp).
- **B, hidden:** [0028] 413 push retries forever · [0094] flaky shrink test · [0027] frontend
  clippy `--all-targets` · [0039] small cleanups.
- Out of scope tonight: [0029] and the assistant features ([0040], [0075]); anything waiting on him.
- Morning, his: [0001] the WS buys check, then his go on 1.2.4 [0002] with live's ntfy lines [0079].

⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys, releases
and writes to live data are his to run (the permission check refuses mine), so hand him the command.
⚠️ A fast-forward of public `main` closes the draft PR (now #8). Open a new one after each.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`); the mock demo
page is the one exception. Overlay: dispatch from the paired branch, with `ci.yml -f
public_ref=<full SHA>` before any build. Live tools: `~/omni-dev-tools/`.

## ⛔ Waiting on him
- **Rename four real fixtures** on the machine holding `.reference/`, [0093]: commands in the overlay log.
- **His decisions, `tasks.md` § Waiting on his decision:** [0003]–[0016], unchanged.
- **Promoting ~11 general feedback notes to `~/.claude/rules/`:** the always-loaded tier is at 315 of
  320 lines, so promotion means compressing something. His call on what.
