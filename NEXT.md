# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — devices show the history, then E3 on his go
E2 is done and verified on live (`tasks.md` § Stage 4, "✅ E2 DONE"). Then:
1. He confirms both devices show the imported transactions (Ledger may need one Apply, filed
   nitpick in § Open).
2. Ask his go for E3: `archive-upload.py --instance production` (1,041 docs), ruled right after E2
   (`tasks.md` § Stage 4, E). Re-verify the script and the count live before asking.
⚠️ E4: the import stops at different dates per account, so re-ask "where it stops" per mailbox.
⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys and
releases are his to dispatch (the permission check refuses mine); hand him the exact command.
⚠️ A fast-forward of public `main` closes the draft PR (now #6); open a new one after each.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`,
`set_config.py`, `set_retention_timers.py`. Overlay workflows: dispatch from the overlay branch that
pairs with the public ref, and before any overlay build run overlay `ci.yml` with
`-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **The device check after E2**, then **his go for E3** (live).
- **The design session's decisions**: `DESIGN_PREREAD.md` § Decisions, Parts 1 and 2 (read first).
- **Live credentials for B5**: `[imap.*]` mailboxes and a reader seat, before E4 (live has no `[llm]` at all).
- **Retention timers on live**, set from the phone, before the backfill is switched on.
- **`/etc/omni-me/credentials-agent.toml`** on the box, `[llm]` only (+ `auth_token` if set), before F1.
- **R2**: his Cloudflare account, a bucket and a scoped API token, for the off-box blob copy.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): `tasks.md` § release item 4.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
