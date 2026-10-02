# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — his choice; F1, F2, F2a, F3 and F5 are done and verified on the dev phone
Dev phone on 1.1.20-dev, dev server on the matching image. `tasks.md` § Design calls holds each
item's decision log and what stayed unverified (F3: the camera itself, and a double-counting
reading of a receipt whose second page repeats the first). Nothing is queued without his priority.
Owed and not blocking: the Rust 1.99 upgrade (CI is pinned to 1.98.1), and the triage scoring
query once batches carrying a verdict have been reviewed.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **What comes next** (F4 needs an example from him; the 1.99 upgrade; or something else).
- **F3's camera path**: take a two-photo receipt on the dev phone; and whether multi-page reading
  should be told pages can repeat (the one test double-counted). `tasks.md` § F3.
- **Review the re-proposed September receipts** (~70 batches, dev phone on 1.1.19-dev). His first
  commit with a correction is also the first real run of that path.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **Should a wipe clear the vector index synchronously?** `tasks.md` § DoR item 1, fourth finding.
- **The official finances import**: September has closed; when every statement is ready.
  ⛔ Gate 1, the LIVE server backup, comes first. Verify by size, never by exit code.
- **Mass document ingestion**: `archive::ingest_paths` has no caller yet.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
