# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — close the last of stage 1, then hand him the phone checklist
`tasks.md` § "THE RELEASE PROCESS". Done 2026-10-05: items 5, 9b, 10, gate 9, the C2 slate.
Still in flight on the box (check each, never assume):
- **Agent memory re-trial** (item 6): container `omni-agenttrial`, volume `omni-dev_agenttrial`.
  Read cgroup `memory.peak`; on exit record it, remove both, then deploy agent `dev-5c2f3ac` to dev.
- **Transcription slate**: `~deploy/omni-bench/runs/20261005-043836-c-transcription`. Then extraction
  on its survivors, apply per the item 8 ruling, ⛔ delete `~deploy/omni-bench/corpus` and confirm.
Then stage 2: finalise the checklist drafted in `tasks.md` § Stage 2 and give it to him.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Seat B's bench keys** (`MODEL_THRESHOLDS.md` § Seat B): proposed, not his; the run waits.
- **C2 reader**: unmeasured by the rules, but gemma-4-26B beats the seated model outside the noise
  floor. Switch or not is his (`tasks.md` item 8).
- **The phone test** (stage 2), once the checklist is final.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
