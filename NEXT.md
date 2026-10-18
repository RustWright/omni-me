# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — B4 done; ask him, one question at a time
B4 verdict and findings: `tasks.md` § release, "B4 REHEARSAL" (log `~/omni-dev-tools/state/b4-log.md`).
Dev now serves the live clone. Questions, in my order:
1. **Cost**: with the server `mem_limit`, staying on the CX22 now fits (`tasks.md` § the cost
   comparison, "SUPERSEDED IN PART"). Stay, resize for margin, or separate agent host?
2. **B5 and the bank-source ordering** (`tasks.md` § B5 and the B4 entry): live needs mail and
   enrichment switched on, his credentials sections, and the bank sources must not start at C3.
3. **Fast-forward `main` again** for `e8cfd89`, `0f40a73` (draft PR #4 runs CI meanwhile).
⛔ Live is the pause: every Phase C to F step on live waits for his go, one at a time.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`,
`set_config.py`, `set_retention_timers.py`. Overlay workflows: dispatch from the overlay branch that
pairs with the public ref, and before any overlay build run overlay `ci.yml` with
`-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **September statements** for the paisa importers (go-live E2). Nothing in Phase E starts without them.
- **`/etc/omni-me/credentials-agent.toml`** on the box, `[llm]` only (+ `auth_token` if set), before F1.
- **Retention timers on live** before the backfill is switched on (his 2026-10-12 values, set on dev only).
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): they are still served
  by SHA. `tasks.md` § release item 4 has what to name.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
