# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — stage 2: his phone test, then fix what it finds
`tasks.md` § "THE RELEASE PROCESS". Stage 1 is DONE (2026-10-06). The checklist is handed over:
`tasks.md` § Stage 2 (dev: server `dev-bbf48fa-5c2f3ac`, agent `dev-72fafe6`, APK 1.1.25-dev).
Take his findings, fix, repeat until he is satisfied; then stage 3 (review: security, logic,
performance, bloat), then stage 4 (merge dev → live, go-live sequence drafted then).

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **The phone test** (stage 2).
- **Seats B + D switch on dev**: run `switch-dev-seats-b-d.sh` (session scratchpad; supersedes
  `switch-dev-structurer.sh`). `tasks.md` item D and the seat B entry.
- **C2 reader**: unmeasured by the rules, but gemma-4-26B beats the seated model outside the noise
  floor. Switch or not is his (`tasks.md` item 8).
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
