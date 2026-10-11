# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — assistant build order (R4-3 ruling): bench steps 2-6, then step 7
Order and rulings: `tasks.md` § "Round 4 early feedback" (R4-2, R4-3, R4-4). Baseline 25/36.
Steps 2-6 built; bench them (`--bench-cases`, agent `f3de10e`, on a copy of the dev agent
volume, dev agent stopped meanwhile), then deploy, then step 7 (stronger open-weights models
on the same set). R4-1/R4-4 deployed and verified on the S9 (1.1.32-dev).
Then his round 4 test, stage 3 (review), stage 4 (merge).

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Dev actions need no permission; live ones do.

## ⛔ Waiting on him
- **Round 4 of the phone test** (now 1.1.32-dev). `tasks.md` § round 4 checklist.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
