# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — stage 1 of the ruled release process, item 1: belief recency
`tasks.md` § "THE RELEASE PROCESS" (ruled 2026-10-04): clear the list → phone test loop → code
review gate → merge to live. Work the stage-1 list in its order; ask each decision when reached.
⚠️ The CI hang fix (`1c8a559`) is one green run old: watch the next several before calling it gone.
Dev now: server `dev-4e5a3b9-3fab90d` + ws-api 0.39.3 driver from `/opt/omni-ws-dev`, agent `dev-2dc97a4`, APK 1.1.24-dev.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.

## ⛔ Waiting on him
- Nothing on the bank feed: fixed on dev; live waits for the finances tab, by his ruling
  (`tasks.md` § "Bank feed reconnect fails"). The app's 150 s reauth wait (`d549552`) is in no APK yet.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The counter → destination → action inventory**: proposed, not agreed.
- **The official finances import** (now a closing step, not the steady-state source).
  ⛔ Gate 1, the LIVE server backup, comes first. Verify by size, never by exit code.
- **Mass document ingestion**: `archive::ingest_paths` has no caller yet (G9's add is one file).
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
