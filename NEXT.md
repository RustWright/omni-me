# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — work the autonomous queue, then switch to his tasks
His instruction 2026-10-17: do everything that needs no input from him first; switch to his tasks
only when the queue is exhausted. Rulings: `tasks.md` § release, "Stage 4: the go-live sequence".
⛔ Do not re-ask them. B-a and B1–B3 are built and green. The queue, in this order (my proposal):
1. **B-b old-mail backfill**: design, then build with the per-category timers as config. Proposed
   values go in as defaults that he confirms later; never block the build on them.
2. **The recurring-cost comparison** owed to him (larger box vs separate agent host vs object
   storage, beside the LLM spend). Measure and source prices; write it up for his decision.
3. **Phase A**: merge both dev branches to `main`, stamp the version, overlay CI on the SHA.
4. **B4 rehearsal** on a fresh clone of live (read-only snapshot, before Phase C), Phases C–F on
   dev except E2, which needs his statements.
⛔ Live is the pause: every Phase C to F step on live waits for his go, one at a time.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Before any overlay build, dispatch overlay `ci.yml`
with `-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **September statements** for the paisa importers (go-live E2). Nothing in Phase E starts without them.
- **`/etc/omni-me/credentials-agent.toml`** on the box, `[llm]` only (+ `auth_token` if set), before F1.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): they are still served
  by SHA. `tasks.md` § release item 4 has what to name.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
