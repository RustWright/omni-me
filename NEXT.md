# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — close item 6 (agent memory), then stage 2
`tasks.md` § "THE RELEASE PROCESS". Stage 1 is done except item 6. Done 2026-10-05: items 5, 8
(all four seats re-ranked, corpus deleted), 9b, 10, gate 9, and seat B's bench (built, not run).
- **Item 6, in flight**: trial B (`omni-agenttrial`, image `dev-c79f823`, ONNX Runtime arena off)
  vs trial A (`dev-bcb6688`). Samplers: `~deploy/omni-bench/runs/agenttrial-{A,B}-*.tsv` (B has an
  anon column). On B's exit: compare per phase, record, remove container and `omni-dev_agenttrial`
  volume; if B holds, deploy `dev-c79f823` as dev's agent.
- Then stage 2: finalise the checklist in `tasks.md` § Stage 2 and give it to him.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Seat D switch on dev**: run `switch-dev-structurer.sh` (session scratchpad; credentials file).
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
