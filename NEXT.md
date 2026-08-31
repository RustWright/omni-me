# NEXT

**Next action: build a release APK, confirm the splash on the test phone, then Phase 6 (step 9).**
Boot splash + readiness gate written and web-verified (up ~280ms, gone by ~623ms with the app
rendered; 723 tests green; clippy clean both frontend configs). **Nothing native is verified** —
Playwright is Chromium, and cold-boot ordering is the whole point. `scripts/android-build.sh release`
(the APK EMBEDS `frontendDist`), force-stop, cold open from a saved **Finances** tab: expect no white
flash, no Journal frame, animated enso, app already built; repeat from **Journal** for the editor
hold. Then step 9: Phase 6 (branch-gate, v1 tag, doc archive) → updatable app → OTA round-trip.

## Decisions in force — inherit these, don't re-derive

- **Splash scope settled (user, 2026-08-31): Rust splash + charcoal native `windowBackground`.** A
  custom `index.html` and Android 12+'s `SplashScreen` API were **deferred past v1**; revisit only on
  a fresh complaint. "Don't get side tracked; release at its best state as soon as we can" is the
  standing steer for everything below.
- **Everything stays disposable until daily use on the PERSONAL phone**, after OTA is confirmed on
  the test phone; that handover's wipe is **total, no backups kept**. Until the phone has the new
  APK, **dismiss auto-import batches, never commit them** (`commit_txn_id` is client-side).
- **Credentials live on the box** (`:ro`, read at boot); `deploy.yml` builds the overlay against the
  **public** pushed ref, so this repo pushes first, and public CI gates on `cargo fmt`. **Email
  ingest is CUT from v1.** One roadmap push per fresh context. Pre-v1 review **CLOSED**.

## Do NOT re-survey

- **The splash + its two paid-for traps** (bundle preload must stay hoisted to app mount or the gate
  serializes what used to overlap; `PullEvent::Applied` fires *after* the projection, hence the new
  pre-apply `Applying`). Write-up: `tasks.md` 2026-08-31 entry. 96/144/192px compared — 144 won.
- **The import and the seed.** Every figure reproduced; box verified by pulling all 12277 events back
  under a throwaway device id. **`OMNI_VOLUME`** resolves from the running container
  (`deploy/lib-volume.sh`). **Naive cold-open indexes** — disproven; SurrealDB 3.0.4 won't serve it.

## Open threads

- **Auto-import filter attribution still OPEN.** Ticks report `events=0` but `filtered rows already
  in the journal` has never appeared — the API returned nothing, rather than the filter suppressing
  rows. Watch for that line on a tick that returns data.
- The brokerage source needs an OTP Reconnect (the wipe took its saved session file).
- Box runs **unauthenticated** (`[server].auth_token` unset); decide before the personal phone. The
  privacy guard cannot catch "wise" (substring of "otherwise") — hand-grep public commits.
