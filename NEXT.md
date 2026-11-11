# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — heal the live agent (F1), F2's questions in between
The live agent is STOPPED (his yes, 2026-11-11): it indexed transactions only, the `4a0f43a`
regression fixed in `0b0bccf` (`tasks.md` § Stage 4, "🐞 FOUND" under F1). Then:
1. Prove `0b0bccf`'s agent image on dev: dev `.env` `OMNI_AGENT_IMAGE`, a fresh dev agent volume
   (`omni-dev-b4_agent_data`, re-derivable), dev server up; the sweep must show journal, notes and
   documents rows > 0.
2. Live: he dispatches `deploy.yml` with the new `agent_image`, after the live `omni-deploy_agent_data`
   volume is removed (his go); watch memory through the ~4.5 h sweep (`f1_watch.py`).
3. F2: questions 2–7 in `~/omni-dev-tools/f2/questions.md`, one at a time; the note goes on live
   only after a release carrying `ff94381`.
⚠️ Two pre-November drafts sit in his review queue (`tasks.md`, same entry): his to dismiss.
⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys and
releases are his to dispatch (the permission check refuses mine); hand him the exact command.
⚠️ A fast-forward of public `main` closes the draft PR (now #6); open a new one after each.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`,
`livesrv.py`, `set_config.py`, `set_retention_timers.py`. Overlay workflows: dispatch from the overlay
branch that pairs with the public ref, and before any overlay build run overlay `ci.yml` with
`-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **The live agent redeploy** (his dispatch) once dev proves the fix; **F2 questions 2–7**.
- **The two pre-November drafts** in review: dismiss or keep.
- **The design session's decisions**: `DESIGN_PREREAD.md` § Decisions, Parts 1 and 2 (read first).
- **R2**: his Cloudflare account, a bucket and a scoped API token, for the off-box blob copy.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): `tasks.md` § release item 4.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
