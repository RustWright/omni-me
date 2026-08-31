# NEXT

**Next action: the FINAL go-live wipe, then install on the personal devices.** Its gate — "after
the OTA/update path is confirmed on the test phone" — **cleared 2026-08-31**: Android and desktop
OTA are both proven end-to-end. **Do the wipe in a fresh session (user)** — a distinct
destructive push.

## Decisions in force — inherit these, don't re-derive
- **Wipe scope is in `tasks.md` step 9** — box to a fresh slate, and **delete** rather than keep:
  `/var/omni-snapshots/*`, the stray empty `omni_data` volume, and locally
  `~/.local/share/com.omni-me.app/{local.db,budget.journal}.PREWIPE-*` / `.bak-*` plus
  `_backup_pre_*` (**178 MB / 10 items**, inventoried 2026-08-31).
- **SETTLE FIRST — ask, do not assume:** the box's 12 277 events under `01KWVHDSCPYBDRAXY7T8YV4M6Y`
  are the **real imported ledger + journal** (of 12 315; rest = 28 test-phone, 10 auto-import).
  "Fresh slate" means either (a) wipe then re-import cleanly from `omni-me-private/examples/`,
  or (b) genuinely start empty. **(a) is the likely intent** — "the old data is not useful"
  refers to the *backups* in that sentence — but it is the user's call and it is irreversible.
- **Use `db-ops.yml` (mode `wipe`), NOT ssh.** SSH from this laptop is refused: *"tailnet policy
  does not permit you to SSH to this node"*. A wipe also needs the `OMNI_VOLUME` override plus a
  size check ([[project-hetzner-db-reset-for-testing]]).
- **Box auth stays deferred until AFTER the personal phone is set up (user)** — so a token
  problem can't masquerade as a failure of that phone's initial backfill. It is fully
  unauthenticated today; zero-downtime order is in `tasks.md`.
- **Install guide is in the private overlay README**, including the two one-time Android prompts
  and why **"Install without scanning"** (behind *More details*) is right — *Scan app* uploads
  the APK, and these builds bake the box hostname.

## Do NOT re-survey
- **All four fixes are verified on-device and shipped in v1.0.3** (correct local date, OTA
  install bridge, real enso icon, desktop AppImage that runs at all), plus the branch gate, doc
  archive, and Android baseline. Techniques recorded in `tasks.md`: persistent `Xvfb` +
  `xdotool`; `adb exec-out screencap` + `input tap`; the **`Pacific/Honolulu` override**, the
  only way to test the date bug outside the 19:00–24:00 window. **`XDG_DATA_HOME` must be
  absolute**; prove isolation by mtime.

## Open threads
- Desktop flashes white ~320ms (WebView surface, not the window; confirm on a real GPU) · CI
  can't tell a launchable release from an unlaunchable one — an `xvfb-run` smoke step catches
  that class, and the user said **after** the round-trip, which is now · brokerage source needs
  an OTP Reconnect · privacy guard can't catch "wise", so hand-grep public commits.
