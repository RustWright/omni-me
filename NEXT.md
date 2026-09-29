# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — exercise the cursor reset live on dev
The wipe demonstration is **done and passed**, re-import included; `tasks.md` § DoR item 1 carries
every number. The cursor-reset route he ruled for is built and green in CI but has **run nowhere**.

Deploy dev onto public `fece433` (image build `36516548153`, dispatched 2026-09-28 — pull as
`deploy`, gate the `~/omni-dev/.env` rewrite on `docker image inspect`, never on the pull), then
reset one source and show the next tick re-fetching what it had already read.
⚠️ **Dev's two IMAP sources are PAUSED** — resume them when the reset work is finished.
Tooling: `~/omni-dev-tools/devsrv.py` (inventory · wipe · reset-cursor) and `import-tools/`.

⛔ **Never `cargo build`/`test` on this box** — three VM kills on 2026-09-28. `check`/`fmt`/`clippy`
only; anything that links goes to CI. Recipe and the measured reason are in the
`project-dev-machines-and-build-limits` memory note.

## ⛔ Waiting on him
- **The official finances import**: after September closes, when every statement is ready. ⛔ Do not
  build toward importing before that. `tasks.md` § DoR item 1.
- **Mass document ingestion** — he raised it as a possibility, not a commitment, and the corpus he
  supplied carries ~700 PDFs. ⚠️ `archive::ingest_paths` and `walk_dir` exist but **nothing calls
  them**; a caller has to be written first. His call whether that is in scope.
- **R27's account side** — his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **Gate 1 — the server backup.** Verify by size, never by exit code. ✅ Done for dev 2026-09-28
  (`~/omni-dev-backups/`); the LIVE one is still owed before any live wipe.
- **The public-repo identity scrub** — authorised, end of a session only. `tasks.md` § Owed at cycle close.
