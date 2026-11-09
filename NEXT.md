# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — finish the ledger, then E1 + E2 in one sitting
The historical ledger is extended and reconciled, all but October's WS checking (`tasks.md` § Stage 4,
"RULED 2026-11-09 … E2's extension", and its ✅ EXTENDED entry). Then:
1. His October checking CSV lands in `P:\Backups\paisa-ledger\ingest_documents\ws`: file it, regen,
   InTransit must reach 0 and checking must match its 10-31 close.
2. Copy back to P: (old P: copy kept as a dated folder; his ruling). P: is not mounted in WSL:
   `cmd.exe` + `robocopy` via `\\wsl.localhost\Ubuntu-24.04\…` works.
3. Ask his go for E1 (wipe, `/sync/pull` tally before and after), then E2 (import) right after.
Also owed, both ruled 2026-11-09: build the poller's archived-bytes skip before E4 (`tasks.md` §
"B4 open item 1"), and write the design pre-read for the notification + agent-framework session.
⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys and
releases are his to dispatch (the permission check refuses mine); hand him the exact command.
⚠️ Overlay `feba64a` (agent CPU weight) is on the dev branch only: merge to `main` before F1.
⚠️ A fast-forward of public `main` closes the draft PR (now #6); open a new one after each.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`,
`set_config.py`, `set_retention_timers.py`. Overlay workflows: dispatch from the overlay branch that
pairs with the public ref, and before any overlay build run overlay `ci.yml` with
`-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **The WS General Checking October CSV** (E2; E1 waits with it).
- **Live credentials for B5**: `[imap.*]` mailboxes and a reader seat, before E4 (live has no `[llm]` at all).
- **Retention timers on live**, set from the phone, before the backfill is switched on.
- **`/etc/omni-me/credentials-agent.toml`** on the box, `[llm]` only (+ `auth_token` if set), before F1.
- **R2**: his Cloudflare account, a bucket and a scoped API token, for the off-box blob copy.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): `tasks.md` § release item 4.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
