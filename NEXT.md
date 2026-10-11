# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — assistant build order (R4-3 ruling), step 1 baseline then step 2
Order and rulings: `tasks.md` § "Round 4 early feedback" (R4-2, R4-3, R4-4). Step 1 is built
(`--bench-cases`); run it on the box against a copy of the dev agent's volume, dev agent
stopped meanwhile (4 GB box, live beside it). Then step 2, ledger grounding from his
CONVENTIONS.md. R4-1/R4-4 built (`6f9bcc5`): deploy with APK 1.1.32-dev, check on the S9.
Then his round 4 test, stage 3 (review), stage 4 (merge).

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Dev actions need no permission; live ones do.

## ⛔ Waiting on him
- **Round 4 of the phone test** (1.1.31-dev). `tasks.md` § round 4 checklist.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
