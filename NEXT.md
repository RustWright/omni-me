# NEXT

**Next action: the FINAL go-live wipe, then install on the personal devices.** Its gate — "after
the OTA path is confirmed on the test phone" — **cleared 2026-08-31**: Android and desktop OTA
are both proven. **Do the wipe in a fresh session (user)** — a distinct destructive push.
## Decisions in force — inherit these, don't re-derive
- **Wipe scope is in `tasks.md` step 9** — **delete**, don't keep: `/var/omni-snapshots/*` and
  locally `~/.local/share/com.omni-me.app/{local.db,budget.journal}.PREWIPE-*` / `.bak-*` plus
  `_backup_pre_*` (**178 MB / 10 items**, inventoried 2026-08-31).
- **SETTLED (user): TOTAL clean slate — "as if there has never been any data or server ever
  running on the box."** NOT wipe-then-re-import. All **12 315** events go, incl. the 12 277 of
  real imported ledger + journal; the box comes up empty and real data goes in fresh afterwards.
  An earlier draft guessed wipe-then-re-import — that guess was wrong; do not act on it.
- **SSH the box on its PROVIDER public IP as root, NOT over the tailnet** (user; how past
  sessions did it) — verified. Address + invocation are in the overlay's `deploy/` docs and past
  logs; **they don't belong in this repo, which is public.** Tailnet SSH is the refused path.
- **Box inventory (live, 2026-08-31):** one container `omni-me-private` (healthy), **one** volume
  `omni-deploy_omni_data`, **3** snapshots (~64 MB). `tasks.md`'s list is stale — the 87-byte
  no-op tarballs and the stray empty `omni_data` volume are gone. Needs the `OMNI_VOLUME`
  override + size check ([[project-hetzner-db-reset-for-testing]]).
- **Box auth stays deferred until AFTER the personal phone is set up (user)** — so a token
  problem can't look like a backfill failure. Unauthenticated today; order is in `tasks.md`.
- **Install guide is in the private overlay README** — the two one-time Android prompts, and why
  **"Install without scanning"** (behind *More details*) is right: *Scan app* uploads the APK,
  and these builds bake the box hostname.
## Do NOT re-survey
- **All four fixes are verified on-device and shipped in v1.0.3** (local date, OTA install
  bridge, enso icon, a desktop AppImage that runs at all), plus the branch gate, doc archive and
  Android baseline. Techniques in `tasks.md`: persistent `Xvfb` + `xdotool`; `adb exec-out
  screencap` + `input tap`; the **`Pacific/Honolulu` override** (only way to test the date bug
  outside 19:00–24:00). **`XDG_DATA_HOME` must be absolute**; prove isolation by mtime.
## Open threads
- **On any NEW device run `omni-me-private/privacy-guard/install.sh` before committing here** —
  a fresh clone of this PUBLIC repo has no guard (denylist gitignored, hook not cloned). It now
  covers infrastructure addresses, added after this session leaked the tailnet IP into this file
  (history left as-is by decision — CGNAT, not routable).
- Desktop flashes white ~320ms (WebView surface, not the window; confirm on a real GPU) · CI
  can't tell a launchable release from an unlaunchable one — an `xvfb-run` smoke step catches
  that class, and the user said **after** the round-trip, which is now · brokerage needs an OTP
  Reconnect · the guard still can't catch "wise", so hand-grep public commits.
