# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — the CI test-fixture fix, on its own branch (ruled 2026-10-03)
`tasks.md` § "The test fixture leaks a SurrealKV instance per test": one shared instance, a namespace
per test. ⛔ First prove isolation holds across namespaces (`init_schema`, the projection version
table); then consolidate the ~27 `fn test_db` copies into one. Plan before building, fresh session.
Dev runs everything else ruled 2026-10-03: server `dev-208e689-2dc97a4`, agent `dev-2dc97a4`, APK
1.1.24-dev. After the fix: the agent's index-build memory growth; the due-belief review.
Owed and not blocking: the Rust 1.99 upgrade (CI pinned to 1.98.1); the triage scoring query.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Approve or reject the belief proposal** in the phone's inbox (first live `belief.record`).
- **Bank feed OTP**: the second feed's session expired; reconnect from the phone, which also gives
  the transfer-join its first real run.
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
