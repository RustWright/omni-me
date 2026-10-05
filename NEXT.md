# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — finish stage 1 of the release process, then build and install the dev APK
`tasks.md` § "THE RELEASE PROCESS" (ruled 2026-10-04): clear the list → phone test loop → code
review gate → merge to live. Every open stage-1 item there carries its ruling; work them in order.
In flight on the box when this was written (2026-10-05 ~03:50Z), check each, never assume:
- **Agent memory re-trial** (item 6): container `omni-agenttrial` on volume `omni-dev_agenttrial`,
  image `dev-551d35c` (batch 16). Read its cgroup `memory.peak`; on finish record, then remove both.
- **Reading slate** (item 8): `~deploy/omni-bench/runs/20261005-034414-c-reading`. Then
  transcription and extraction slates against `~deploy/omni-bench/corpus` (copy was in progress;
  verify size and file count against `~/paisa-ledger-restore/extracted/paisa-ledger` first).
  ⛔ Delete the corpus copy after, and confirm. Apply results per the item 8 ruling.
- Then: 20-file bulk-upload trial (item 5), Rust 1.99 (item 10), dev images (server + agent from
  the dev branch tips), dev APK installed on the phone (item 9b), then the phone checklist.
Dev now: server `dev-b7f9397-84a0cd8` (SurrealDB 3.3), agent `dev-84a0cd8`, APK 1.1.24-dev.
⚠️ CI hang fix: one flake since, and it was the SurrealDB race (fixed by 3.3). Keep watching.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **The phone test** (stage 2), once the APK is on the dev phone and the checklist is written.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
