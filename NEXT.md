# NEXT

**Next action: the OTA round-trip — baseline PUBLISHED, both artifacts now good.** Android +
desktop **1.0.0** are on the box. Remaining: **install the baseline** ([USER] — Android needs an
UNINSTALL first), then ship **1.0.1** and prove the update path end-to-end on both platforms.
Done this session: 6.2, 6.3 (`v1.0.0` tagged), 6.4, and the bug below.

## Decisions in force — inherit these, don't re-derive

- **The desktop AppImage had NEVER run — fixed 2026-08-31.** `tauri-plugin-updater` hard-errors on
  a non-https endpoint in release and only *warns* in dev, so every AppImage from 0.2.0 on
  panicked at startup and `cargo tauri dev` could never show it. Fixed with
  `dangerousInsecureTransportProtocol:true` in the private CI's injected updater config; rebuilt
  and confirmed launching. Relaxes transport only — WireGuard + minisign already cover it.
  **Revisit if `/updates` ever leaves the tailnet.**
- **Branch gate = safety-rails, deliberately NOT PR-gating (user).** **Never add
  `required_status_checks`** — verified to gate DIRECT pushes too, which breaks `session-end.sh`.
- **Phone app is DEBUG-keystore signed, CI signs with the RELEASE key, and Android refuses
  install-over across differing certs.** Baseline needs an **uninstall** (total phone wipe) → CI
  APK → ~12k-event re-sync (also exercises the "Restoring N…" chip). Local APK = **throwaway**.
- **Everything stays disposable until daily use on the PERSONAL phone**; that wipe is **total, no
  backups**. Until the phone has a CI APK, **dismiss auto-import batches, never commit them**.

## Do NOT re-survey

- **The branch gate, the doc archive** (history in `.archive/v1.0.0/`; `tasks.md` is open work
  only), **the splash code** (Android-verified), **the import/seed** (12277 events back), and the
  **release-only-config sweep** — the updater was the *only* hard-fail of that class. Traps in
  `tasks.md`: editor-bundle preload stays hoisted to app mount; `PullEvent::Applied` fires *after*
  the projection; **`#1e1e1e` is in FOUR files that must match**.

## Open threads

- **Desktop splash visual is [USER]-blocked** — AppImage runs, but nothing here can film a cold
  boot (`grim` fails, GNOME DBus `AccessDenied`, no `Xvfb`).
- **The pipeline can't tell a launchable release from an unlaunchable one** — how the AppImage bug
  survived two months. An `xvfb-run` smoke step would catch the class; filed, not built.
- **Auto-import filter attribution OPEN** — ticks report `events=0`, but `filtered rows already in
  the journal` has never appeared. Watch for that line on a tick that returns data.
- Brokerage source needs an OTP Reconnect. Box runs **unauthenticated** — decide before the
  personal phone. Privacy guard can't catch "wise"; hand-grep public commits.
