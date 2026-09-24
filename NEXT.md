# NEXT

✅ **`bfb551a`+ — CI GREEN** (1271 core / 163 frontend / 84 app). ✅ Dev server
`dev-1929863-2cc6d6a`; ⛔ live untouched (`sha-9a0b1dd`, 2w). ✅ **`1.1.10-dev` running on the
phone** — installed in place, token kept, boot 1.1s. ✅ **Grouping verified ON THE DEVICE.**

## ▶ NEXT ACTION — one mail. Everything else is in place.
1. **Ask the user to re-forward the Food Basics confirmation** — the only mail sharing an
   `order_group` with a batch already queued.
2. 🔴 **The question:** does `order_ref` come back bare `21088946880437216` (→ merges into one
   review item with a supersession) or `090621088946880437216` (→ stays separate)?
   ⛔ Judge against that prediction; do not re-derive it afterwards.
3. Read it off the phone: `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>`, then
   `list_pending_batches` over CDP. ⛔ Never `pm clear` — it wipes the server token.

## ⛔ Inherit — settled, do not re-derive
- **No vendor reference → no grouping**, per class. **A revision opens a new review item.**
- ⛔ **Grouping runs PER DEVICE** — `build_projections_server` registers only `documents`.
- ✅ **On the phone's own projection:** 10 batches — 3 order-scoped, rest `receipts-uid-N`, with
  `order_group` + `group_member` set and `superseded: []`. Only the **collapse** is unproven.
- ⛔ **"One Walmart order → six batches" is NOT duplication** — two separate orders. ⛔ **Never
  bump a projection version** until the rebuild defects are fixed (`tasks.md`).
- ✅ **Dev deploys are NOT blocked from here** (box SSH + `docker compose`); the old note was wrong.

## 🔴 Open — the phone wedge
- 🔴 `1.1.8-dev` sat 8h on `Restoring 16052 events…`: all tokio + surrealdb threads `state=S`,
  every Tauri command pending forever, `RenderThread` burning 3.2 CPU-h. ✅ A force-stop +
  relaunch recovers it. Measurements: overlay `DEV_TESTING.md` § "THE DEV PHONE IS WEDGED".
- ⛔ **Volume is disproven, twice.** `core/tests/backfill_wedge.rs` (ignored; run by name) does
  16,052 events through 10 projections in ~130s flat — and **the S9 itself replayed 17,148 in
  104s**. Neither hardware nor event count is the cause; it was a hang.
- ⛔ **Stall location UNKNOWN** — `advance_bookmark` runs once after the whole batch, so a wedge
  leaves the bookmark at the start. ⚠️ An earlier "it's the tail" note was wrong. The new chunked
  apply bookmarks per chunk and logs each; reproducing needs `pm clear` **plus** the token.
- 🔴 Three rebuild defects · projection writes swallow statement errors (four have ZERO
  `.check()`) · suite deadlock (recorded fix disproven) — all in `tasks.md`.

## ⛔ Standing
- ⛔ **NEVER run the full suite on this box** — it OOMs, and `CARGO_BUILD_JOBS=1` does not save it.
  Local = fmt/clippy/check only. Draft PR #1 is open, so a plain push runs the full gate in ~8min.
- ⚠️ `build-dev-apk.yml`'s `public_ref` must be a **BRANCH**, not a SHA. ⚠️ Check an APK's build
  time against the commit before believing a silent log. ⚠️ `omni-me-private` is NOT hook-committed.
  Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
