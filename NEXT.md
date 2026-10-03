# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — G5: bank feeds on dev, a live walkthrough with him
Dev phone on 1.1.22-dev, dev server on `dev-1056255-60fe816` with `/opt/omni-ws` now mounted.
Step 2 (copy live's two disabled sections into dev's credentials) is his: the one-line base64
command was given 2026-10-02 and the classifier refuses it to me. Then: restart dev, check
`/auto_import/status` lists both bank sources, and walk him through the bank OTP.
`tasks.md` § On-device pass on 1.1.20-dev holds every G-item, his rulings and what was verified.
Owed and not blocking: the Rust 1.99 upgrade (CI pinned to 1.98.1); the triage scoring query.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- **His hands on the phone**: the camera (G9), the keyboard (G2), a real pinch (G1).
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
