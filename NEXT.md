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
- **SETTLED (user, 2026-08-31): TOTAL clean slate — "as if there has never been any data or
  server ever running on the box."** NOT wipe-then-re-import. All **12 315** events go, including
  the 12 277 of real imported ledger + journal; the box comes up empty for the initial release
  and real data is entered/imported fresh afterwards. Nothing is retained anywhere. An earlier
  draft of this note guessed wipe-then-re-import — that guess was wrong; do not act on it.
- **Reaching the box: use the tailnet IP `100.90.1.17` (past sessions did).** HTTP on the IP
  answers `200`. **SSH is currently refused for BOTH the MagicDNS name and the IP** — *"tailnet
  policy does not permit you to SSH to this node"* — so that is a Tailscale SSH ACL, not name
  resolution; it needs an ACL change or `db-ops.yml` (mode `wipe`), which runs box-side. A wipe
  needs the `OMNI_VOLUME` override + size check ([[project-hetzner-db-reset-for-testing]]).
- **Box auth stays deferred until AFTER the personal phone is set up (user)** — so a token
  problem can't masquerade as a failure of that phone's backfill. Fully unauthenticated today;
  zero-downtime order is in `tasks.md`.
- **Install guide is in the private overlay README** — the two one-time Android prompts, and why
  **"Install without scanning"** (behind *More details*) is right: *Scan app* uploads the APK,
  and these builds bake the box hostname.

## Do NOT re-survey
- **All four fixes are verified on-device and shipped in v1.0.3** (correct local date, OTA
  install bridge, real enso icon, desktop AppImage that runs at all), plus the branch gate, doc
  archive, and Android baseline. Techniques in `tasks.md`: persistent `Xvfb` + `xdotool`; `adb
  exec-out screencap` + `input tap`; the **`Pacific/Honolulu` override**, the only way to test
  the date bug outside 19:00–24:00. **`XDG_DATA_HOME` must be absolute**; prove isolation by mtime.
## Open threads
- Desktop flashes white ~320ms (WebView surface, not the window; confirm on a real GPU) · CI
  can't tell a launchable release from an unlaunchable one — an `xvfb-run` smoke step catches
  that class, and the user said **after** the round-trip, which is now · brokerage needs an OTP
  Reconnect · privacy guard can't catch "wise", so hand-grep public commits.
