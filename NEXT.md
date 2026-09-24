# NEXT

✅ **Grouping built, browser-verified, and now safe to ship** (public `3747f46`). Gate green:
1267 core / 84 app / 163 frontend, clippy ×4, fmt. ⛔ **Still unverified on a device** — the dev
phone is stuck mid-rebuild. Evidence in overlay `DEV_TESTING.md` § 8. Dev server healthy on
`dev-cdf8059-228f7e9`. Rollback `/var/omni-snapshots/dev-pre-grouping-20260924-044915.tgz`.

## ▶ NEXT ACTION — wipe the dev phone, install `1.1.8-dev`, check the Walmart pair
1. `adb shell pm clear com.omni_me.app` — it has been rebuilding projections for 40+ minutes and
   its `projection_versions` row records 2, so a wipe is cheaper than letting it finish.
2. Install the APK from run `35964433851` (`~/omni-dev/apk/omni-me-1.1.8-dev.apk` on the box),
   launch, let it cold-sync, then read the review queue.
3. 🔴 **The question:** do the two Walmart mails (`Thank you for shopping` + `order was
   delivered`, one order) show as **one** review item carrying a supersession, or two?
   ⚠️ Grouping happens in the projection, which runs **per device** — the server never builds
   this table at all (`build_projections_server` registers only `documents`).

## ⛔ Waiting on the user
- **Deploy `dev-fc66490-ac130e2`** (adds `instacart` to the vendor list), then **re-forward the
  two Instacart mails** — their UIDs are past the IMAP cursor and nothing can rewind it.
- **Should a dev-deploy job go in the overlay CI?** Each swap is currently a hand-run SSH command
  that auto mode blocks. ⛔ Not built unilaterally: a deploy workflow with the wrong target hits
  live.

## ⛔ Inherit — settled, do not re-derive
- **No vendor reference → no grouping**, per class. **A revision opens a new review item**;
  committed books are never amended. ⚠️ Revisit trigger is his own friction.
- ⚠️ **Dismissed treated like committed** is MY call, not his. Flag it if it annoys him.
- ⛔ **"One Walmart order → six batches" is NOT a duplication factor** — two separate orders.
- ⛔ **Never bump a projection version** until the three rebuild defects are fixed (`tasks.md`).
  A bump wipes and replays **all ten** projections; on the S9 that did not finish in 40 minutes.

## 🔴 Open
- 🔴 **Three rebuild defects**: one stale projection rebuilds all ten · the whole event log loads
  into memory (710 MB) · `omni_me_core` tracing never reaches Android, so it reads as a hang.
- 🔴 **Projection writes swallow statement errors** — `.await?` returns Ok on a rejected
  statement. Four projections have ZERO `.check()`; `tasks.md` says why not to sweep blind.
- 🔴 **Suite deadlock**: the recorded fix was **disproven by measurement** — `+1 thread per
  `connect()`` survives dropping handle and TempDir. One shared instance stays flat. [S, not M]
- ⚠️ **Playwright MCP needs a `chrome` channel** whose install wants sudo; worked around with
  bundled chromium from a scratchpad script.

## ⛔ Standing
- 🔴 **CI's ONE invocation** naming core+server+agent compiles core with `auto-import`.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⚠️ `omni-me-private` is NOT hook-committed.
- ⚠️ Strip ANSI before grepping docker logs. Branches: `dev/role-split-model-seats` /
  `dev/untested-overlay`.
