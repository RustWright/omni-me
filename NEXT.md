# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — build his 2026-10-02 on-device pass, G1–G10 + F4
`tasks.md` § On-device pass on 1.1.20-dev holds each cause and the four decisions he took.
Order is MY proposal, following his earlier "bugs first": the six bugs (G1 zoom, G2 keyboard,
G3 thread read, G4 settings overflow, G8 preheader filler, G9 camera provider), then the built
decisions (G6, G7, G9 archive add, G3 archive/delete), then G5 feeds on dev, then F4.
Owed and not blocking: the Rust 1.99 upgrade (CI pinned to 1.98.1); the triage scoring query.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **G9 confirmation**: one tap on "take photo" once he is done committing, so logcat can confirm
  the FileProvider cause before the fix ships.
- **G5**: a bank-feed OTP when the dev feeds are switched on.
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
