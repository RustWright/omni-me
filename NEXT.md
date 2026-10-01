# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — build the triage scoring query
The seat runs on dev in shadow mode (`tasks.md` § Mail triage seat). Scoring joins each
proposal's `source_metadata.triage` with his decision on it: a committed batch that triage called
`none` is a miss. It is only meaningful once he has reviewed batches carrying a verdict, so the
September batches (proposed before the seat existed) do not score it.
⛔ Dev only; live runs the stamp.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Device pass on 1.1.12-dev**, installed on the dev phone 2026-09-30 (built from `2ee3468`):
  the four UX fixes, plus the journal jumping to 2026-09-24 after the rebuild. Both in
  `tasks.md` § Open — from daily use.
- **Review the re-proposed September receipts** on the dev phone: ~82 batches from the re-fetch,
  68 with postings. Recovery is rewind → drain → his review and commit.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **Should a wipe clear the vector index synchronously?** `tasks.md` § DoR item 1, fourth finding.
- **The official finances import**: after September closes, when every statement is ready.
  ⛔ Gate 1, the LIVE server backup, comes first. Verify by size, never by exit code.
- **Mass document ingestion**: `archive::ingest_paths` has no caller yet.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
