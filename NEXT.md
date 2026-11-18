# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Rulings
live in `project.md` § Decisions in force, open work in `tasks.md` (ids `[NNNN]` = `.omni/tasks.toml`),
rationale in `docs/src/` and at the edit site. ⛔ If a line here could be derived from those, cut it.

## ▶ NEXT ACTION — release 1.2.6, then finish closing Cycle 5
The Cycle 5 review is triaged in full (11-17/18): every finding has his disposition in
`reviews/2026-11-16-*.md` (gitignored); deferrals are [0099], [0100], [0098] and `project.md`
§ Carried to Cycle 6 Planning. The fixes since 1.2.5 are on `dev/role-split-model-seats`,
CI-green to `1bbc103`, on dev (`dev-93b6de0-1bbc103`) and the S9 (1.1.48-dev), smoke-passed.
1. **1.2.6, his go, one at a time:** staged and CI-green (stamp `eaff0bc`, overlay lock `b7bcf55`;
   S9 1.1.49-dev from the stamp passed). Push public `main` to `eaff0bc`; push overlay `main` =
   local `b7911cd`; dispatch the deploy (`agent_image` empty); verify; App release `version=1.2.6`.
2. [0073] the comment-convention revision: its own design-first session.
3. Then bump `.omni/tasks.toml` to cycle 6, and [0074]'s planning session starts the next push.

⛔ Live is the pause: every live step waits for his go, one at a time.
⛔ **Never `cargo build`/`test` on this box** (`project-dev-machines-and-build-limits`); the mock demo
page is the one exception.

## ⛔ Waiting on him
- The 1.2.6 go (step 1).
- **His actions:** [0006] the R2 account (he chose later), [0010] the OpenRouter privacy setting.
- [0013] is parked until a real blank delivery item appears in review; ask with it on screen.
- **Promoting ~11 general feedback notes to `~/.claude/rules/`:** the always-loaded tier is at 315 of
  320 lines, so promotion means compressing something. His call on what.
