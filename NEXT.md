# NEXT

**Next action: step 9 — Phase 6 (6.2 branch-gate, 6.3 v1 semver + tag, 6.4 doc archive) → updatable
app on mobile + desktop → trivial-change OTA round-trip end to end.**
The splash is **done and device-verified** (Samsung SM-G960W, Android 10): no white flash, correct
tab on first paint, splash 300–900ms. Three defects were found on-device and two fixed — write-up in
`tasks.md`'s 2026-08-31 splash entry. **3 commits are unpushed**; the public repo must be pushed
*before* dispatching `app-release`, because CI builds the client from the public ref.

## Decisions in force — inherit these, don't re-derive

- **The phone's app is signed with the DEBUG keystore; CI signs with the RELEASE keystore. Android
  refuses install-over across certs.** So the OTA baseline needs an **uninstall** — a total wipe of
  phone data — then the CI-signed APK, then a re-sync of ~12k events from the box. That trip is
  unavoidable and belongs to step 9; doing it also exercises the still-unverified "Restoring N
  events…" indicator. The locally-built APK now on the phone is a **throwaway** for splash testing.
- **Splash scope stays settled (user, 2026-08-31):** Rust splash + charcoal native background. The
  custom `index.html` and Android 12+'s `SplashScreen` API remain **deferred past v1** — the white
  flash was fixed *without* them, via the WebView canvas colour. "Don't get side tracked; release at
  its best state as soon as we can" is still the standing steer.
- **Everything stays disposable until daily use on the PERSONAL phone**, after OTA is confirmed on
  the test phone; that handover's wipe is **total, no backups kept**. Until the phone has a CI APK,
  **dismiss auto-import batches, never commit them** (`commit_txn_id` is client-side).
- **Credentials live on the box** (`:ro`, read at boot); public CI gates on `cargo fmt`. **Email
  ingest is CUT from v1.** One roadmap push per fresh context. Pre-v1 review **CLOSED**.

## Do NOT re-survey

- **The splash.** Device-verified end to end; both fixes committed (`7d92e8f` boot-hold release on
  unmount, `1d53cc6` charcoal WebView canvas). Its three paid-for traps are written down in
  `tasks.md`: the bundle preload must stay hoisted to app mount; `PullEvent::Applied` fires *after*
  the projection; and **`#1e1e1e` now lives in three files that must stay equal** (`themes.xml`,
  `MainActivity`, `--color-bg`).
- **The import and the seed.** Every figure reproduced; box verified by pulling all 12277 events
  back under a throwaway device id. **`OMNI_VOLUME`** resolves from the running container
  (`deploy/lib-volume.sh`). **Naive cold-open indexes** — disproven; SurrealDB 3.0.4 won't serve it.

## Open threads

- **Unjudged by machine:** splash animation smoothness in the hand, and whether 144px reads right on
  a real phone. Frame grabs cannot answer either — needs the user's eyes on a cold open.
- **Whether 6s is the right splash cap is now untested** — nothing reaches it on this device.
- **Auto-import filter attribution still OPEN.** Ticks report `events=0` but `filtered rows already
  in the journal` has never appeared. Watch for that line on a tick that returns data.
- The brokerage source needs an OTP Reconnect (the wipe took its saved session file).
- Box runs **unauthenticated** (`[server].auth_token` unset); decide before the personal phone. The
  privacy guard cannot catch "wise" (substring of "otherwise") — hand-grep public commits. It *did*
  catch a real institution name in handoff prose this session, so it is earning its keep.
