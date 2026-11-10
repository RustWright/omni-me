# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — the backfill on live, then F1
E1–E4 and B5 are done; mail intake runs on live from 2026-11-01 (`tasks.md` § Stage 4, "✅ E4 DONE").
1. Timers are set (`tasks.md`, "✅ RETENTION TIMERS ON LIVE"). On his go, I set
   `archive.backfill_before = 2026-11-01` (ruled: it meets E4's date) and watch the first ticks.
2. F1: `/etc/omni-me/credentials-agent.toml` (his), dev stopped during the first sweep (ruled).
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
- **His go for the backfill date** on live.
- **`/etc/omni-me/credentials-agent.toml`** on the box, `[llm]` only (+ `auth_token` if set), before F1.
- **The two pre-November drafts** in review: dismiss or keep.
- **The design session's decisions**: `DESIGN_PREREAD.md` § Decisions, Parts 1 and 2 (read first).
- **R2**: his Cloudflare account, a bucket and a scoped API token, for the off-box blob copy.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): `tasks.md` § release item 4.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
