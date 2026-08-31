# NEXT

**Next action: fix the ANDROID OTA install bridge, then re-run the phone round-trip.**
**Desktop OTA is DONE and proven** (1.0.0 → 1.0.1 self-update, byte-identical to the published
artifact, relaunched, reports 1.0.1). Android downloads + sha-verifies, then **silently fails to
hand off to the installer**. v1.0.0 + v1.0.1 tagged and published for both platforms.

## Decisions in force — inherit these, don't re-derive

- **ANDROID OTA BUG, root-caused, fix NOT applied.** Rust `request_android_install` writes the
  side-file to `app_local_data_dir()` → Tauri `getDataDir` → **`activity.dataDir`**
  (`PathPlugin.kt:64`); Kotlin `checkInstallRequest` reads **`File(filesDir, …)`** =
  `dataDir/files`. Different dirs → poller finds nothing → silent return (no dialog, no log).
  **Preferred fix: point Kotlin at `dataDir` for BOTH channels.** `share_intent.rs:33` asserts
  they're the same dir — **that comment is FALSE**, so share-target has the same bug: fix both
  and **retest share-target on-device**. Ruled out: the `REQUEST_INSTALL_PACKAGES` appop
  (granting changed nothing); overrides ARE in the CI APK; the poller was running.
- **The desktop AppImage had NEVER run before today** — `tauri-plugin-updater` hard-errors on a
  non-https endpoint in release, warns in dev. Fixed with
  `dangerousInsecureTransportProtocol:true`. Revisit if `/updates` leaves the tailnet.
- **Branch gate = safety-rails, NOT PR-gating: never add `required_status_checks`** — it gates
  DIRECT pushes too, breaking `session-end.sh`. **Android refuses install-over across differing
  signing certs** (proven), so a cert change means a wipe. **Box auth is deferred until AFTER
  the personal phone is set up (user)**, so a token problem can't look like a backfill failure.

## Do NOT re-survey

- **Desktop OTA, the branch gate, the doc archive, the Android baseline** (12314 events, 0
  failures; Settings shows the CI-baked `server_url`, closing the 2026-07-21 item), and the
  **release-only-config sweep**. Techniques: persistent `Xvfb :99` as a background task +
  `xdotool`; `adb exec-out screencap` + `input tap`. **`XDG_DATA_HOME` MUST be absolute** —
  relative is silently ignored and the app falls back to real user data; prove isolation by mtime.

## Open threads
- **Android "Today" is the UTC date, not local** — at 23:42 CDT the phone said 2026-08-31,
  desktop correctly 2026-08-30, so evening entries file under tomorrow. `journal.rs:175-200`
  describes this failure and its re-anchor; it isn't holding on Android. **Daily-use blocker.**
- **Desktop flashes white ~320ms** (WebView surface, not the window; confirm on a real GPU) ·
  **CI can't tell a launchable release from an unlaunchable one** — `xvfb-run` smoke step, user
  says **after** the round-trip · brokerage source needs an OTP Reconnect.
