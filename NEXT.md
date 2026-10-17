# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — his turn: the autonomous queue is exhausted (2026-10-17)
Done and green: B-b older-mail backfill, the recurring-cost comparison, the v1.2.0 stamp. All in
`tasks.md` § release, "Stage 4: the go-live sequence" (Phase B list and the entries under it).
Ask him, one at a time, in this order (my proposal):
1. **Merge to `main`.** The permission check refused my push. Public is a fast-forward to
   `feed4fb`; overlay merge is ready on local branch `merge-main`, still needing
   `.github/public-ref` = `main`. Then overlay `ci.yml` with `-f public_ref=<main SHA>`.
2. **May the box pull `alpine`?** B4's read-only clone of live runs `snapshot.sh`, which runs in
   `alpine`, and the box has none (`tasks.md` § the alpine finding). Then B4 itself, on dev.
3. **The cost decision**: resize to CX33 + R2 as off-box blob backup (mine) vs a separate agent
   host. Write-up in `tasks.md`; the LLM spend figure is his billing page.
⛔ Live is the pause: every Phase C to F step on live waits for his go, one at a time.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Before any overlay build, dispatch overlay `ci.yml`
with `-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

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
