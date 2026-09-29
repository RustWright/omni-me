# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — the four UX fixes need a device pass; only one is verified
Pushed through `f7a1f74`; CI green on every job, and the layout sweep now measures 16 screens
including `archive › detail (wide CSV)` at both widths. ⛔ **That verifies the overflow fix and
nothing else** — the unverified-filter, the two nav intents and the attachment panel are
compile-and-lint-clean only and have run nowhere. `tasks.md` § Open — from daily use has the detail.

⚠️ **The sweep is not a gate yet.** Its first two runs were broken while reporting green — see
`tasks.md`. ⛔ Do not drop `continue-on-error` until it fails for a regression deliberately
introduced; a check that has never failed for a real reason is not evidence.

⚠️ **Tap-target numbers came from measurement, not the earlier grep, and the grep was wrong.**
Settings toggles are 358×20 and Journal's "Raw properties" is 104×16 — worse than anything
`px-2 py-1` flagged. `routines.rs`'s destructive confirms never rendered in mock, so they remain
unmeasured.

⚠️ **`gmail_personal` is PAUSED mid-drain**, at roughly UID 400 of 14,878, cursor persisted so a
resume continues exactly there. `gmail_work` and `yahoo` run normally. ⛔ Do not resume it without
the spend decision below — every message it fetches is a model call.

⛔ **Never `cargo build`/`test` on this box** — three VM kills on 2026-09-28. `check`/`fmt`/`clippy`
only; anything that links goes to CI (`build-import-tools.yml` returns runnable binaries). Recipe
and the measured reason are in `project-dev-machines-and-build-limits`.
Tooling: `~/omni-dev-tools/devsrv.py` and `import-tools/`.

## ⛔ Waiting on him
- **On-device re-check of the four UX fixes** once a build reaches the dev phone — archive
  overflow, the Unchecked-only filter + Load more, both "items waiting" rows, and attachments in
  the finance review. ⚠️ None has run anywhere; they are compile-and-lint-clean only.
- **Tap targets** — 14 controls near 26px against Android's 48dp, several the destructive confirms
  in `routines.rs`. Raising them is a visual redesign across screens, so it is his call, not mine.
- **The counter → destination → action inventory** — proposed, not agreed. It is the artifact that
  catches the class behind three of the four; no tool does.
- **Finish the dev re-fetch, or stop it?** ~14,478 messages remain, each one a receipt-extraction
  model call — order **$30** on DeepSeek-V4-Pro. Disk is not the constraint (~11 KB/message, 23 G
  free). ⛔ Paused pending his call; nothing accrues meanwhile.
- **Should a wipe clear the vector index synchronously?** `tasks.md` § DoR item 1, fourth finding.
- **The official finances import**: after September closes, when every statement is ready.
- **Mass document ingestion** — `archive::ingest_paths` and `walk_dir` exist but **nothing calls
  them**; a caller has to be written first.
- **R27's account side** — his to check; ⛔ no list of affected runs exists, by his decision.
- **The three spends**: the C1/C2/C3/D re-rank, gate 9's 90-call re-run, the C2 slate for gate 6.
- **Gate 1 — the LIVE server backup**, still owed before any live wipe. Verify by size, never by
  exit code. ✅ Done for dev 2026-09-28 (`~/omni-dev-backups/`).
- **The public-repo identity scrub** — authorised, end of a session only. `tasks.md` § Owed at cycle close.
