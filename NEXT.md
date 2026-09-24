# NEXT

🔴 **The dev phone wedges on cold backfill — this outranks everything.** `1.1.8-dev` sat on
`Restoring 16052 events…` for 8h: every tokio worker and surrealdb thread `state=S`, `RenderThread`
burning 3.2 CPU-h on the banner, and **every** Tauri command (even in-memory `get_sync_status`)
pending forever. Measurements in overlay `DEV_TESTING.md` § "THE DEV PHONE IS WEDGED".
✅ Dev server now on `dev-1929863-2cc6d6a` (healthy). ⛔ Live untouched: `sha-9a0b1dd`, 2w uptime.

## ▶ NEXT ACTION — build `1.1.10-dev`, wipe the phone, watch the countdown
1. Commit + push the chunked-apply instrument (`core/src/sync/puller.rs`), then
   `gh workflow run 359972206 --repo RustWright/omni-me-private --ref dev/untested-overlay
   -f public_ref=dev/role-split-model-seats -f version=1.1.10-dev -f box=hetzner -f dev_port=3001`.
2. `adb shell pm clear com.omni_me.app` ⚠️ **then re-enter the server token** — a clear wipes it and
   the app then fails silently. Install, launch, watch `auto-pull projecting done=N total=16052`.
3. 🔴 **The question:** does it stop at a chunk boundary (→ a specific event wedges it) or keep
   counting to 0 (→ it was transient, and the countdown is now the standing instrument)?

## ⛔ Waiting on the user
- **Re-forward the Food Basics confirmation** — the ONLY outstanding ask. The unpriced-charge fix
  is deployed, so it will now produce a batch, and it is the one mail that shares an `order_group`
  with an existing one. That single forward is the whole collapse test.
- ✅ **Dev deploys are NOT blocked** — the box SSH + `docker compose` swap runs fine from here; the
  old "auto mode blocks it" note was wrong. A CI job is convenience, not a blocker. (Overlay
  `DEV_TESTING.md` § 4 has the command.)

## ⛔ Inherit — settled, do not re-derive
- **No vendor reference → no grouping**, per class. **A revision opens a new review item.**
- ⛔ **Grouping runs PER DEVICE** — `build_projections_server` registers only `documents`.
- ⛔ **"One Walmart order → six batches" is NOT duplication** — two separate orders.
- ⛔ **Never bump a projection version** until the rebuild defects are fixed (`tasks.md`).
- ✅ **Grouping is verified ON THE DEVICE** (10 batches: 3 order-scoped, rest uid-scoped, with
  `order_group` + `group_member` populated). Only the **collapse** is unproven. Don't re-derive it.
- ✅ **A force-stop + relaunch recovers the wedge**, data intact — so it is survivable, not fatal.

## 🔴 Open
- 🔴 **Volume is NOT the cause.** `core/tests/backfill_wedge.rs` (ignored; run by name) applies
  16,052 events through all 10 projections in **~130s flat**, clean, with and without 38 KB
  `document_archived` payloads. So the wedge is Android-specific or real-data-specific.
- 🔴 **No native stacks**: the S9 is unrooted, so `debuggerd -b` and `run-as` both refuse.
- 🔴 Three rebuild defects · **projection writes swallow statement errors** (four have ZERO
  `.check()`) · **suite deadlock** (recorded fix disproven by measurement) — all in `tasks.md`.

## ⛔ Standing
- ⚠️ **`build-dev-apk.yml`'s `public_ref` must be a BRANCH, not a SHA** (checkout fetches
  `refs/heads/<ref>*`). `build-dev-image` accepts a SHA.
- ⚠️ **Check an APK's build time against the commit** before believing a silent log: `1.1.8-dev`
  was built 25 min BEFORE its own logging fix, which is why 8h produced nothing.
- 🔴 **CI's ONE invocation** naming core+server+agent compiles core with `auto-import`.
  ⚠️ `omni-me-private` is NOT hook-committed. Branches: `dev/role-split-model-seats` /
  `dev/untested-overlay`.
