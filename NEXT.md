# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — none queued; the DoR item 1 chain is clear and waiting on him
Wipe, re-import and cursor rewind are all **demonstrated on dev** and green in CI; `tasks.md`
§ DoR item 1 carries every number. Dev runs `dev-ad0e828-bb6b866`, live is untouched on
`sha-9a0b1dd`. Pick up from **Waiting on him** below — the two decisions there are what gate the
next build, and neither is mine to make.

⚠️ **`gmail_personal` is PAUSED mid-drain**, at roughly UID 400 of 14,878, cursor persisted so a
resume continues exactly there. `gmail_work` and `yahoo` run normally. ⛔ Do not resume it without
the spend decision below — every message it fetches is a model call.

⛔ **Never `cargo build`/`test` on this box** — three VM kills on 2026-09-28. `check`/`fmt`/`clippy`
only; anything that links goes to CI (`build-import-tools.yml` returns runnable binaries). Recipe
and the measured reason are in `project-dev-machines-and-build-limits`.
Tooling: `~/omni-dev-tools/devsrv.py` and `import-tools/`.

## ⛔ Waiting on him
- **Finish the dev re-fetch, or stop it?** ~14,478 messages remain, each one a receipt-extraction
  model call — order **$30** on DeepSeek-V4-Pro. Disk is not the constraint (~11 KB/message, 23 G
  free). ⛔ Paused pending his call; nothing accrues meanwhile.
- **Should a wipe clear the vector index synchronously?** The sweep now prunes orphans
  (`62edf7e`), which closes the window at the *next* sweep but does not make the wipe itself
  complete. `tasks.md` § DoR item 1, fourth finding.
- **The official finances import**: after September closes, when every statement is ready.
  ⚠️ It now has a rehearsal behind it — the same corpus re-imported clean on dev.
- **Mass document ingestion** — raised as a possibility, not a commitment. `archive::ingest_paths`
  and `walk_dir` exist but **nothing calls them**; a caller has to be written first.
- **R27's account side** — his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **Gate 1 — the LIVE server backup**, still owed before any live wipe. Verify by size, never by
  exit code. ✅ Done for dev 2026-09-28 (`~/omni-dev-backups/`).
- **The public-repo identity scrub** — authorised, end of a session only. `tasks.md` § Owed at cycle close.
