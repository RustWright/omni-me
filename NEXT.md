# NEXT

**Next action: step 9 — Phase 6 (6.2 branch-gate, 6.3 v1 tag, 6.4 doc archive) → updatable app →
trivial-change OTA round-trip end to end.** The splash is **done and device-verified** (Samsung
SM-G960W, Android 10): no white flash, correct tab on first paint, splash 300–900ms. Three defects
found on-device, two fixed — write-up in `tasks.md`'s 2026-08-31 splash entry. **3 commits are
unpushed**; push the public repo *before* dispatching `app-release`, which builds from that ref.

## Decisions in force — inherit these, don't re-derive

- **The phone's app is DEBUG-keystore signed, CI signs with the RELEASE keystore, and Android
  refuses install-over across differing certs.** So the OTA baseline needs an **uninstall** (total
  phone-data wipe) → CI APK → ~12k-event re-sync from the box; that belongs to step 9 and also
  exercises the unverified "Restoring N events…" indicator. Today's local APK is a **throwaway**.
- **Splash scope stays settled (user, 2026-08-31):** Rust splash + charcoal native background; a
  custom `index.html` and Android 12+'s `SplashScreen` API stay **deferred past v1** — the white
  flash was fixed without either. "Don't get side tracked" still stands.
- **Everything stays disposable until daily use on the PERSONAL phone**, after OTA is confirmed on
  the test phone; that wipe is **total, no backups kept**. Until the phone has a CI APK, **dismiss
  auto-import batches, never commit them** (`commit_txn_id` is client-side).
- **Credentials live on the box** (`:ro`); public CI gates on `cargo fmt`. **Email ingest is CUT
  from v1.** One roadmap push per fresh context. Pre-v1 review **CLOSED**.

## Do NOT re-survey

- **The splash.** Verified end to end; both fixes committed. Traps in `tasks.md`: the editor-bundle
  preload stays hoisted to app mount; `PullEvent::Applied` fires *after* the projection; and
  **`#1e1e1e` lives in three files that must stay equal** (`themes.xml`, `MainActivity`, `--color-bg`).
- **The import and the seed.** Every figure reproduced; all 12277 events pulled back under a
  throwaway device id. **`OMNI_VOLUME`** resolves from the running container. **Naive cold-open
  indexes** — disproven; SurrealDB 3.0.4 won't serve it.

## Open threads

- **Needs the user's eyes:** splash smoothness in the hand, and whether 144px reads right. The 6s
  cap is now untested — nothing reaches it on this device.
- **Auto-import filter attribution OPEN** — ticks report `events=0`, but `filtered rows already in
  the journal` has never appeared. Watch for that line on a tick that returns data.
- Brokerage source needs an OTP Reconnect. Box runs **unauthenticated** — decide before the
  personal phone. Privacy guard can't catch "wise"; hand-grep public commits.
