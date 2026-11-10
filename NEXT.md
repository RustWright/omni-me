# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — release 1.2.2, verify both devices, then E2
E1 is done; E2 is held for the stale-table bug and its fix (`tasks.md` § Stage 4, "✅ E1 DONE" and
"🐞 FOUND 2026-11-09"; ruling there). Stamped and green: public `4e10d95`, overlay lock `fa85d82`;
the overlay release merge `7fd23c6` is prepared on LOCAL overlay `main`, not pushed. Then:
1. He pushes both mains; overlay CI on `main` (pairs with public `main`) must be green.
2. He dispatches `app-release.yml` (version 1.2.2); both devices update.
3. Desktop (Finances on) must show NO transactions after its first 1.2.2 launch; phone heals when
   Finances is next switched on there.
4. Ask his go for E2: `push_local` from `~/reimport-live-20261109/local.db`, device id
   `reimport-live-20261109`, tools in `~/omni-dev-tools/import-tools-0d22343/`. Verify on live as
   data: +11,048 `transaction_recorded`, all else +0, distinct ids, 2019-05-29 → 2026-10-31.
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
- **Push both mains, dispatch 1.2.2**, update both devices; then **his go for E2** (live).
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
