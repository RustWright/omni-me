# NEXT

✅ **Paginated pull is deployed and verified on hardware.** A fresh device pulls 15,960 events
in 2m46s across ~32 pages (was: 25 MB in one body, four 30s timeouts, then one lucky win).
✅ Archive, Finances, Notes, Routines, Assistant all render on device against real data.

▶ **NEXT ACTION: the user decides three things.** All three are written up with options and a
recommendation in the private overlay's `DEV_TESTING.md` § PARKED. ⛔ Do not decide them alone.
1. 🔴 **The enrichment pass cannot work as deployed** — it queries `documents`, a projection
   table, and the server registers **zero** projections by design (`server/src/lib.rs:257`).
   Recommendation: register only that projection on the server. This changes a stated
   invariant, so it is his call.
2. **Ledger catch-up** — incremental vs full wipe + re-import. Deciding after importing is worse.
3. **The IMAP leg** — dev has no `[imap.*]` on purpose; a poller marks his REAL mail processed.

## ▶ SCOPE — agreed 2026-09-16
**Goal: a fully functional app, ending with the FINANCES TAB re-enabled** — the finish line.
🔴 **I drive the first pass; HE drives the final pass.** Early obvious bugs would sour his
judgement before the feedback that matters. ⛔ Don't hand him a build until the crashes are out.
⛔ `tasks.md` § DEFINITION OF READY is the chain. 🔴 Documents are on the critical path.
✅ Dev data is disposable; rollback snapshot is `dev-pre-autonomous-20260917-042334.tgz`.

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live.** Live is `sha-9a0b1dd`, up 9 days,
  OTA store serving **1.1.2** from Sep 7. ⛔ Never deploy there; `/var/omni-updates` is its store.
- ✅ **`feature.finances = true` on DEV ONLY** — the user set it on the dev phone. It unblocks
  testing finance propose. ⛔ It is NOT the chain's finish line; live still reads `false`.
- ✅ **Dev APKs carry `OMNI_WEBVIEW_DEBUG=1`** — drive the page over CDP. ⛔ Never in
  `app-release.yml`. ⚠️ The socket is named by PID; re-forward after every app launch.
- ⚠️ **Strip ANSI before grepping docker logs** — colour codes sit *inside* tokens
  (`[3muri[0m[2m=[0m/sync/pull`), so `uri=/sync/pull` never matches. This faked a crash once.
- ⚠️ **The sync badge is push-only** — a fresh install reads "Synced" over an empty app for the
  whole catch-up. Pre-existing; paging lengthened the window. ⛔ Don't patch it from `pull_only`.
- 🔴 ⛔ **Every cargo run in a systemd unit with `CARGO_PROFILE_DEV_DEBUG=0`** — its absence,
  not the artifacts, is what filled the disk (17G → 1.1G with it set).
  ⛔ **Never reclaim `target/` by mtime** — cargo reuses unchanged deps without touching it.
- ⛔ **Branches:** public `dev/role-split-model-seats`, private `dev/untested-overlay`.
