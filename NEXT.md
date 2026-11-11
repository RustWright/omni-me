# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — he ships 1.2.3, then F2's note goes live
1.2.3 is staged and CI-green (`tasks.md` § Stage 4, "1.2.3 🔴 GO"). Then:
1. Server is live on 1.2.3. His: the App release; then verify both devices update.
2. The note on live as "Ledger conventions", his go (`tasks.md`, "⛔ Ordering trap").
3. Ask whether dev's agent may run beside live's (the box cannot hold both sweeping; the owed
   cost comparison feeds this, `tasks.md`, "✅ F1 DONE").
4. F3 after: ws-api into `/opt/omni-ws`, live's two `_disabled` bank sections renamed, bank sources
   on. The Finances tab is ALREADY on (his, since the 1.2.2 check, 2026-11-10); no tab step remains.
⚠️ Two pre-November drafts sit in his review queue (`tasks.md`, "✅ E4 DONE"): his to dismiss.
⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys, releases
and writes to live data are his to run (the permission check refuses mine); hand him the command.
⚠️ A fast-forward of public `main` closes the draft PR (now #7); open a new one after each.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`,
`livesrv.py`, `set_config.py`, `refile_live.py`, `f1_watch.py`. Overlay workflows: dispatch from the
overlay branch that pairs with the public ref, and before any overlay build run overlay `ci.yml`
with `-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **The two pre-November drafts** in review: dismiss or keep.
- **The design session's decisions**: `DESIGN_PREREAD.md` § Decisions, Parts 1 and 2 (read first).
- **R2**: his Cloudflare account, a bucket and a scoped API token, for the off-box blob copy.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): `tasks.md` § release item 4.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
