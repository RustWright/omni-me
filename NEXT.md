# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — fix round 2's findings and build D1–D4, then deploy to dev for round 3
Everything is in `tasks.md` § "Stage 2, round 2 results" (R2-1..R2-5 and his D1–D4 rulings).
Then write round 3's checklist, deploy, hand it to him. Repeat rounds until he is satisfied;
then stage 3 (review: security, logic, performance, bloat), then stage 4 (merge, go-live
sequence drafted then).

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Dev actions need no permission; live ones do.

## ⛔ Waiting on him
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
