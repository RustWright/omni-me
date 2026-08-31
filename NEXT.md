# NEXT

**Next action: step 9 — Phase 6 (6.2 branch-gate, 6.3 v1 tag, 6.4 doc archive) → updatable app →
trivial-change OTA round-trip.** Splash **done + Android-verified**: no white flash, correct tab on
first paint, 300–900ms, user confirms it looks right; three defects found on-device, two fixed (see
`tasks.md`). **Push the public repo before dispatching `app-release`** — it builds from that ref.

## Decisions in force — inherit these, don't re-derive

- **Phone app is DEBUG-keystore signed, CI signs with the RELEASE key, and Android refuses
  install-over across differing certs.** OTA baseline needs an **uninstall** (total phone wipe) → CI
  APK → ~12k-event re-sync (also exercises the "Restoring N…" chip). Local APK = **throwaway**.
- **Splash scope stays settled (user, 2026-08-31):** Rust splash + charcoal native background; a
  custom `index.html` and Android 12+'s `SplashScreen` API stay **deferred past v1** — the white
  flash was fixed without either. "Don't get side tracked" still stands.
- **Everything stays disposable until daily use on the PERSONAL phone**, after OTA is confirmed on
  the test phone; that wipe is **total, no backups kept**. Until the phone has a CI APK, **dismiss
  auto-import batches, never commit them** (`commit_txn_id` is client-side).
- **Credentials live on the box** (`:ro`); public CI gates on `cargo fmt`. **Email ingest is CUT
  from v1.** One roadmap push per fresh context. Pre-v1 review **CLOSED**.

## Do NOT re-survey

- **The splash.** Android-verified; user confirms animation and size. Traps in `tasks.md`: the
  editor-bundle preload stays hoisted to app mount; `PullEvent::Applied` fires *after* the
  projection; **`#1e1e1e` is in FOUR files that must stay equal** (conf, themes, MainActivity, css).
- **The import and the seed.** Every figure reproduced; 12277 events pulled back under a throwaway
  device id. **`OMNI_VOLUME`** resolves from the running container. Naive cold-open indexes: disproven.

## Open threads

- **Desktop splash is UNVERIFIED** — same shared code, plus a new `backgroundColor` config key that
  no one has seen render (webkit2gtk, and `grim` can't film this Wayland session). Check at step 9's
  AppImage. The 6s cap is likewise untested now — nothing reaches it on Android.
- **The black gap before the logo (~700ms) is the deferred `index.html`**, not a bug: the enso is
  wasm-rendered, so nothing can paint it sooner. Fix only if it grates after weeks of use.
- **Auto-import filter attribution OPEN** — ticks report `events=0`, but `filtered rows already in
  the journal` has never appeared. Watch for that line on a tick that returns data.
- Brokerage source needs an OTP Reconnect. Box runs **unauthenticated** — decide before the
  personal phone. Privacy guard can't catch "wise"; hand-grep public commits.
