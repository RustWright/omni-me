# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — Stage 4 is through F3; the first live review
1.2.3, F2's note and F3 are all live (`tasks.md` § Stage 4, "✅ F3 DONE"). Next:
1. His review of the two bank batches in Finances (Wise 7, WS 50 drafts). It is also the first
   real use of the conventions note: watch that proposals name ledger accounts, not invented ones.
2. Stage G (`tasks.md` § Stage 4, "G. After"): Support purge (his, optional), dev junk.
⚠️ Two pre-November drafts sit in his review queue (`tasks.md`, "✅ E4 DONE"): his to dismiss.
⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys, releases
and writes to live data are his to run (the permission check refuses mine); hand him the command.
⚠️ A fast-forward of public `main` closes the draft PR (now #8); open a new one after each.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`,
`livesrv.py`, `set_config.py`, `refile_live.py`, `post_conventions_live.py`, `f3_creds_edit.py`.
Overlay workflows: dispatch from the overlay branch that pairs with the public ref, and before any
overlay build run overlay `ci.yml` with `-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **The two bank batches** in Finances: his review.
- **The two pre-November drafts** in review: dismiss or keep.
- **The design session's decisions**: `DESIGN_PREREAD.md` § Decisions, Parts 1 and 2 (read first).
- **R2**: his Cloudflare account, a bucket and a scoped API token, for the off-box blob copy.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): `tasks.md` § release item 4.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
