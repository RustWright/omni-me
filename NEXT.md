# NEXT

**Next action: install on the two personal devices — `surface` (Linux desktop) and the personal
phone — user at the keyboard, assistant guiding.** The go-live wipe, clean re-import and box seed
are DONE (2026-08-31). Then: in-app **Reconnect** for the brokerage (OTP in-app, never over SSH),
then box auth.

## Decisions in force — inherit these, don't re-derive
- **This laptop is the dev/import machine, NOT a go-live device (user).** Ledger + vault live
  here, so it seeds the box and steps out. The go-live desktop is **`surface`** (already on the
  tailnet); it installs the released AppImage fresh and backfills like the phone.
  **Both installs are [USER]-run, assistant-guided.** Guide = private overlay README §
  *Installing on a NEW device*: tailnet first (`/updates` is tailnet-only), then the two
  one-time Android prompts — *Allow from this source*, and Play Protect where **"Install
  without scanning"** hides behind *More details* (*Scan app* uploads the APK, and these builds
  bake the box hostname). Desktop: keep the AppImage **writable** (it self-replaces); Ubuntu
  needs FUSE 2 or `--appimage-extract-and-run`.
- **The seed is a FRESH import from canonical sources, not a restore** — every pre-wipe event was
  discarded, the test phone's and the old poller's included. **Box keep-list stands:**
  `/var/omni-updates` (outside the volume — deleting it strands every install path),
  `/etc/omni-me/credentials.toml`, `/opt/omni-ws`. Data went; config didn't.
- **SSH the box on its PROVIDER public IP as root, NOT the tailnet** (user) — re-verified. Address
  + invocation are in the overlay `deploy/` docs and [[project-hetzner-db-reset-for-testing]];
  **never here — this repo is public.** Root has no `~/omni-deploy`, so scripts run as root need
  `OMNI_DEPLOY_DIR=/home/deploy/omni-deploy`. **Box auth stays deferred until AFTER the personal
  phone is up (user)** — so a token problem can't look like a backfill failure.

## Do NOT re-survey
- **The wipe→import→seed chain is verified; don't redo or re-reconcile it.** `/data` 21M → **64K**
  (skeleton dotfiles back = proof of real emptying), 4 snapshots deleted, local reset 213M → 776K
  (`device_id`/`server_url`/`base_currency`/`timezone` kept). The import reproduced the
  2026-08-30 corpus exactly — **0 diffs** vs the `ledger bal` oracle (111 rows, checked twice),
  anchors OK, net worth **83 290.52 CAD**. Pushed **12 277**; box holds **12 278**, the extra a
  live Wise draft. Brokerage reads `needs_reauth` by design; Wise healthy; v1.0.3 published.

## Open threads
- **Privacy guard installed on THIS device** (it was missing while the hooks auto-commit + push).
  Run `omni-me-private/privacy-guard/install.sh` on any other device; it can't catch "wise".
- Desktop white flash ~320ms (WebView surface; confirm on a real GPU) · the `xvfb-run` smoke step
  in `app-release.yml` is owed now the round-trip is done · per-source `account_maps` unwired.
