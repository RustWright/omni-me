# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — get an assistant agent running on dev
The chat timeout and the untested belief workflow have one cause: no agent runs anywhere.
`tasks.md` § On-device pass → "First real matching test" and the `omni-me-agent` item (§ Open).
Asked him 2026-10-03 where it should run; act on his answer.
Then, in his order: the Reconcile noise findings in that same entry (transfer halves first by size).
`5a986eb` (bank helper re-proposal) needs CI green, then a dev deploy.
Owed and not blocking: the Rust 1.99 upgrade (CI pinned to 1.98.1); the triage scoring query.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **Where the agent runs** (asked 2026-10-03).
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
