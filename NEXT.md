# NEXT

✅ **Public `bfb551a` — CI GREEN** (1271 core / 163 frontend / 84 app, fmt + clippy ×5 jobs).
✅ Dev server on `dev-1929863-2cc6d6a` (unpriced-charge fix, healthy). ⛔ Live untouched:
`sha-9a0b1dd`, 2w uptime. ✅ **Grouping verified ON THE DEVICE.** 🔴 Only the **collapse** is left.

## ▶ NEXT ACTION — install `1.1.10-dev` IN PLACE, then get the one mail
1. `adb install -r ~/…/omni-me-1.1.10-dev.apk` (scp from the box's `~/omni-dev/apk/`).
   ⛔ **In place — do NOT `pm clear`.** A clear wipes the server token, the app then fails
   silently, and re-entering it needs the credentials file. The collapse test does not need a
   wipe: a forwarded mail is a small incremental pull.
2. **Ask the user to re-forward the Food Basics confirmation.** It is the only mail that shares an
   `order_group` with a batch already in the queue.
3. 🔴 **The question:** does its `order_ref` come back as the bare `21088946880437216` (→ merges
   into one review item carrying a supersession) or `090621088946880437216` (→ stays separate)?
   ⛔ Judge against that prediction; do not re-derive it after the fact.

## ⛔ Inherit — settled, do not re-derive
- **No vendor reference → no grouping**, per class. **A revision opens a new review item.**
- ⛔ **Grouping runs PER DEVICE** — `build_projections_server` registers only `documents`.
- ✅ **Verified on the phone's own projection:** 10 batches — 3 order-scoped
  (`receipts-order-…21088946880437216`, `…600000111912518`, `…600000116818223`), rest
  `receipts-uid-N`, with `order_group` + `group_member` populated and `superseded: []`.
- ⛔ **"One Walmart order → six batches" is NOT duplication** — two separate orders.
- ⛔ **Never bump a projection version** until the rebuild defects are fixed (`tasks.md`).
- ✅ **Dev deploys are NOT blocked from here** (box SSH + `docker compose`); the old note was wrong.

## 🔴 Open — the phone wedge
- 🔴 `1.1.8-dev` sat 8h on `Restoring 16052 events…`: all tokio + surrealdb threads `state=S`,
  every Tauri command pending forever, `RenderThread` burning 3.2 CPU-h on the banner.
  ✅ **A force-stop + relaunch recovers it**, data intact. Full measurements in overlay
  `DEV_TESTING.md` § "THE DEV PHONE IS WEDGED".
- ⛔ **Volume is disproven**: `core/tests/backfill_wedge.rs` (ignored; run by name) does 16,052
  events through all 10 projections in **~130s flat**, with and without 38 KB payloads.
- 🔴 **Suspect the TAIL** — the restart had almost nothing to replay, so it stalls near the end.
  `advance_bookmark` runs once there. The new chunked apply calls it per chunk, so it will either
  move the wedge earlier (naming it) or stop it. ⚠️ Needs a `pm clear` + token to reproduce.
- 🔴 Three rebuild defects · projection writes swallow statement errors (four have ZERO
  `.check()`) · suite deadlock (recorded fix disproven) — all in `tasks.md`.

## ⛔ Standing
- ⛔ **NEVER run the full test suite on this box** — it OOMs, and `CARGO_BUILD_JOBS=1` does not
  save it. Local = fmt/clippy/check only; **push and let CI run it** (draft PR #1 is already open,
  so a plain push triggers the full gate in ~8 min).
- ⚠️ **`build-dev-apk.yml`'s `public_ref` must be a BRANCH, not a SHA.** ⚠️ Check an APK's build
  time against the commit before believing a silent log. ⚠️ `omni-me-private` is NOT hook-committed.
  Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
