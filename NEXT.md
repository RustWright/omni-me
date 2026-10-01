# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — F2: correct a proposal before commit, with F2a (rows that say what they are)
His order 2026-09-30: bugs, F1, then F2. F1 is built and running on the dev phone (1.1.17-dev);
`tasks.md` § On-device pass, F1. Design first: which fields are editable, and how an edit is
recorded so the original proposal stays auditable.
Owed and not blocking: the Rust 1.99 upgrade (CI is pinned to 1.98.1 since it broke on
`stable`), and the triage scoring query once batches carrying a verdict have been reviewed.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Try F1 on the dev phone (1.1.17-dev)**: pinch-zoom a receipt photo and a PDF. Everything
  else was measured on the device; a real pinch could not be.
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
