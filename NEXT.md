# NEXT

✅ **Grouping built, browser-verified, safe to ship** (public `3747f46`). Gate green: 1267 core /
84 app / 163 frontend, clippy ×4, fmt. ⛔ **Unverified on a device** — the dev phone is stuck
mid-rebuild. Evidence in overlay `DEV_TESTING.md` § 8. Dev on `dev-cdf8059-228f7e9`; rollback
`/var/omni-snapshots/dev-pre-grouping-20260924-044915.tgz`.

## ▶ NEXT ACTION — wipe the dev phone, install `1.1.8-dev`, check the Walmart pair
1. `adb shell pm clear com.omni_me.app` — it has been rebuilding projections for 40+ minutes and
   its `projection_versions` row records 2, so a wipe is cheaper than letting it finish.
2. Install `~/omni-dev/apk/omni-me-1.1.8-dev.apk` (run `35964433851`), launch, let it cold-sync.
3. 🔴 **The question:** do the two Walmart mails (`Thank you for shopping` + `order was
   delivered`, one order) show as **one** review item carrying a supersession, or two?
   ⚠️ Grouping runs **per device** — the server never builds this table
   (`build_projections_server` registers only `documents`).

## ⛔ Waiting on the user
- **Deploy `dev-fc66490-ac130e2`** (adds `instacart`), then **re-forward the two Instacart
  mails** — their UIDs are past the IMAP cursor and nothing can rewind it.
- **A dev-deploy job in the overlay CI?** Each swap is a hand-run SSH command auto mode blocks.
  ⛔ Not built unilaterally: a deploy workflow with the wrong target hits live.

## ⛔ Inherit — settled, do not re-derive
- **No vendor reference → no grouping**, per class. **A revision opens a new review item**;
  committed books are never amended. ⚠️ Revisit trigger is his own friction.
- ⚠️ **Dismissed treated like committed** is MY call, not his. Flag it if it annoys him.
- ⛔ **"One Walmart order → six batches" is NOT a duplication factor** — two separate orders.
- ⛔ **Never bump a projection version** until the rebuild defects are fixed (`tasks.md`): a bump
  wipes and replays **all ten**; on the S9 that did not finish in 40 minutes.

## 🔴 Open
- 🔴 **Three rebuild defects** (⛔ above) · **projection writes swallow statement errors**
  (`.await?` returns Ok on a rejected one; four have ZERO `.check()`). Mechanism in `tasks.md`.
- 🔴 **Suite deadlock**: the recorded fix was **disproven by measurement** — +1 thread per
  `connect()` survives dropping handle and TempDir; one shared instance stays flat. [S, not M]
  ⚠️ **Playwright MCP wants a `chrome` channel** needing sudo; bundled chromium works instead.

## ⛔ Standing
- 🔴 **CI's ONE invocation** naming core+server+agent compiles core with `auto-import`.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⚠️ `omni-me-private` is NOT hook-committed.
  ⚠️ Strip ANSI in docker logs. Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
