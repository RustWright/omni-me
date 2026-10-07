# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — stage 2 round 1: fix his findings, build the interaction map, deploy dev
He is AWAY until his next message; his rulings for the stretch are in `tasks.md` § "Stage 2, round
1" (R1-1..R1-8). Order: R1-1..R1-6 fixes → CI green → dev deploy + APK install (R1-8, authorised)
→ R1-7 interaction map and walkthroughs (design changes listed for him, not applied) → hand him
round 2 of the checklist. Then stage 3 (review), stage 4 (merge, go-live).

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Round 2 of the phone test**, once the stretch lands; check-in result (turned on 2026-10-07).
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
