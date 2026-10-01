# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — F1: full screen + zoom for images and PDFs, with downscaled previews
His order 2026-09-30: bugs, then F1, then F2. The bugs are done and verified on the phone
(`tasks.md` § On-device pass on 1.1.12-dev). F1 includes B4's fix: a few-hundred-KB preview to
open, the original only on zoom, because his link measured 0.6–1.2 MB/s. Design first.
Then F2 (edit a proposal before commit) with F2a (batch rows that say what they are).
Parallel, not blocking: the triage scoring query, once he has reviewed batches carrying a verdict.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
The layout sweep has now failed for a real regression and passed after the fixes; whether to drop
its `continue-on-error` is his call, `tasks.md` § Open — from daily use.

## ⛔ Waiting on him
- **The dev phone runs 1.1.14-dev** (from `af3bbcd`): tab restore, both overflows, the settings
  rows. Nothing needs his re-check unless he wants to; all four were measured on the device.
- **A new day: should Journal open on today** rather than the last date viewed? `tasks.md` B3.
- **Review the re-proposed September receipts** (~68 batches). Easier after F2/F2a.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **Should a wipe clear the vector index synchronously?** `tasks.md` § DoR item 1, fourth finding.
- **The official finances import**: September has closed; when every statement is ready.
  ⛔ Gate 1, the LIVE server backup, comes first. Verify by size, never by exit code.
- **Mass document ingestion**: `archive::ingest_paths` has no caller yet.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
