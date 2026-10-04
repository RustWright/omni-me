# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — the CI test-fixture fix, on its own branch (ruled 2026-10-03)
`tasks.md` § "The test fixture leaks a SurrealKV instance per test": one shared instance, a namespace
per test. ⛔ First MEASURE that isolation holds across namespaces (`init_schema`, the projection
version table); then consolidate the ~27 `fn test_db` copies into one. Plan before building.
Branch from `dev/role-split-model-seats` and merge back into it (main lacks this branch's tests).
Pending from 2026-10-04: a dev image with the bank-reconnect diagnostics (`3fab90d`, overlay
`4e5a3b9`) was building; deploy it to dev if not already there (`docker ps` on the box).
Dev otherwise: agent `dev-2dc97a4`, APK 1.1.24-dev.
Owed and not blocking: the Rust 1.99 upgrade (CI pinned to 1.98.1); the triage scoring query.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Belief recency** (he rejected the first belief: old entries outvoted a recent change): his call.
- **One more bank-feed reconnect try** once the diagnostics build is on dev; the server log then
  shows the bank's refusal reason (`tasks.md` § "Bank feed reconnect fails").
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **Should a wipe clear the vector index synchronously?** `tasks.md` § DoR item 1, fourth finding.
- **The official finances import** (now a closing step, not the steady-state source).
  ⛔ Gate 1, the LIVE server backup, comes first. Verify by size, never by exit code.
- **Mass document ingestion**: `archive::ingest_paths` has no caller yet (G9's add is one file).
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
