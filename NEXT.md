# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — record seat B's slate, then stage 2 (his phone test)
`tasks.md` § "THE RELEASE PROCESS". Stage 1 is DONE (2026-10-05/06) except seat B's result:
- **Seat B slate, in flight**: `~deploy/omni-bench/runs/*-b-review` (7 models × 3, image
  `dev-72fafe6`, throwaway hub `omni-benchhub` on network `omni-benchnet`, volume
  `omni-benchhub-data`). Score per `MODEL_THRESHOLDS.md` § Seat B (keys ruled), apply per the item 8
  ruling, then ⛔ remove the hub container, its volume and the network, and confirm.
- Then stage 2: hand him the checklist in `tasks.md` § Stage 2 (dev: server `dev-bbf48fa-5c2f3ac`,
  agent `dev-72fafe6`, APK 1.1.25-dev installed).

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Seat D switch on dev**: run `switch-dev-structurer.sh` (session scratchpad; credentials file).
- **C2 reader**: unmeasured by the rules, but gemma-4-26B beats the seated model outside the noise
  floor. Switch or not is his (`tasks.md` item 8).
- **The phone test** (stage 2).
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
