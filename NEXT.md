# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — push, so CI runs what this box cannot
The 2026-09-29 UX fixes are on the branch, unpushed. Local gates are all green (both frontend
clippy configs, 169 frontend tests, `-p core -p server -p agent` clippy, `-p omni-me-app` clippy).
⛔ **The core/server/agent test suite has NOT run** — no local `cargo test`, by the rule below.
Draft PR #1 is open, so a plain `git push` runs the full gate in ~8 min. `tasks.md` § Open — from
daily use carries every finding.

⚠️ **The layout sweep has never executed.** `tests/viewport-check.mjs` + its `ci.yml` step are
written but this box cannot link a wasm build, so CI is its first run. It is `continue-on-error`
on purpose; ⛔ do not promote it to blocking until it has passed and caught a deliberate regression.

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
