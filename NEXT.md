# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — finish the dev re-import, then build the cursor-reset route
The scoped wipe is **demonstrated and passed on dev** (`tasks.md` § DoR item 1 carries the numbers).
Two things remain, in order:

1. **The ledger re-import on dev.** He supplied the recovered corpus; it restores to
   `~/paisa-ledger-restore/extracted/paisa-ledger/` — ⛔ real financial data, keep it out of both
   repos. Expect **~10,575** transactions, not dev's pre-wipe 10,598; the gap is September
   auto-import and is explained in `tasks.md`, not a discrepancy to chase.
   Runner: `scratchpad/reimport.sh {import|push}` against a throwaway device DB at
   `~/reimport-device`, pushing to `:3001`.
2. **The cursor-reset route** — his ruling 2026-09-28, rationale and the rejected alternatives in
   `tasks.md` § DoR item 1 and `docs/src/features.md` § Three things this does not cover.

⚠️ **Local builds: use the detached capped unit, never a bare `cargo build`.** Recipe and the
measured reason (`surrealdb_core` is one rustc at ~2.9 GB, so `JOBS=1` cannot help) are in the
`project-dev-machines-and-build-limits` memory note. A bare build took WSL down twice on 09-28.

## ⛔ Waiting on him
- **The official finances import**: after September closes, when every statement is ready. ⛔ Do not
  build toward importing before that. `tasks.md` § DoR item 1.
- **R27's account side** — his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **Gate 1 — the server backup.** Verify by size, never by exit code. Precedes any real-data wipe.
  ✅ Done for dev on 09-28; the LIVE one is still owed before any live wipe.
- **Mass document ingestion** — he raised it as a possibility, not a commitment. ⚠️ `ingest_paths`
  and `walk_dir` exist but **nothing calls them**; reaching the ~700 PDFs needs a caller written
  first. His call whether that is in scope.
- **The public-repo identity scrub** — authorised, end of a session only. `tasks.md` § Owed at cycle close.
