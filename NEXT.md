# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — reset `tasks.md`: the Cycle 4 close-out (his pick, 2026-11-12)
Its own session, design-first: 5,800 lines, 115 open boxes vs 65 closed, never reset since Cycle 4
began ("used as a dumping ground"). `PROJECT_PROCESS.md` § Task File Management: reset each cycle.
1. Rulings still in force move to `project.md` (decision summaries, per the density hierarchy);
   closed history drops to git. Do this FIRST: this file points into `tasks.md` for decisions.
2. Verify each open item against code and git before it carries (`project-tasks-md-drifts-stale`);
   some are done, some superseded by the LLM-first push (`project-llm-primary-interface-next-push`).
3. Distil survivors into a fresh `tasks.md`; create `.omni/tasks.toml` (`TASK_HUB_CONTRACT.md`).
4. A size guard so it cannot recur, like `NEXT.md`'s cap. The SessionStart hook is shared: do
   not edit it (`feedback-dont-edit-synced-shared-scripts`); propose the mechanism to him.
5. Then the rest of the close-out `project.md` owes (curiosities pass, memory prune) and its
   stale Status line (still says v1.1.0).

## ⏳ Dated, independent of the reset
- **11-16:** check the weekly WS buys arrive as two legs, no Unmatched (`tasks.md`, "✅ SHIPPED").
- **Then** the batched release 1.2.4, his go: `tasks.md`'s "📦 Release queue". Fixes until then
  land on the dev branch and onto that list, not on live.

⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys, releases
and writes to live data are his to run (the permission check refuses mine); hand him the command.
⚠️ A fast-forward of public `main` closes the draft PR (now #8); open a new one after each.
⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only (`project-dev-machines-
and-build-limits`). Overlay: dispatch from the overlay branch paired with the public ref, run
overlay `ci.yml` with `-f public_ref=<full SHA>` before any overlay build. Live tools: `~/omni-dev-tools/`.

## ⛔ Waiting on him
- **N-to-1 receipt matching**: build it or not (the flights case; `tasks.md`, "🆕 Gap").
- **The two pre-November drafts** in review: dismiss or keep.
- **The design session's decisions**: `DESIGN_PREREAD.md` § Decisions, Parts 1 and 2 (read first).
- **R2**: his Cloudflare account, a bucket and a scoped API token, for the off-box blob copy.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): `tasks.md` § release item 4.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
