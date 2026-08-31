# NEXT

**Next action: the OTA round-trip.** `app-release` for **1.0.0** is dispatched (run
`33352705256`, both targets). When it lands: **install the baseline** (Android needs an UNINSTALL
first — see below), then ship **1.0.1** and prove the in-app update path end-to-end on both
platforms. Fix it if it breaks — that is the whole point of this step.

**Done this session:** 6.2 branch-gate, 6.3 `v1.0.0` tagged, 6.4 trackers archived, plus two [XS]
Android friction items (`unused_mut`, `gen/schemas` untracked).

## Decisions in force — inherit these, don't re-derive

- **The branch gate is safety-rails, deliberately NOT PR-gating (user, 2026-08-31).** Public repo
  has rulesets `main-protection` + `release-tags` (`v*` immutable). **A `required_status_checks`
  rule gates DIRECT pushes too** — verified, it rejected a plain push to `main` — so it was
  removed; re-adding it breaks `session-end.sh` every session. The private overlay can't be gated
  at all (free plan → `403 Upgrade to GitHub Pro`); it uses a tracked `pre-push` hook instead.
- **Phone app is DEBUG-keystore signed, CI signs with the RELEASE key, and Android refuses
  install-over across differing certs.** Baseline needs an **uninstall** (total phone wipe) → CI
  APK → ~12k-event re-sync (also exercises the "Restoring N…" chip). Local APK = **throwaway**.
- **Everything stays disposable until daily use on the PERSONAL phone**, after OTA is confirmed on
  the test phone; that wipe is **total, no backups kept**. Until the phone has a CI APK, **dismiss
  auto-import batches, never commit them** (`commit_txn_id` is client-side).
- **Credentials live on the box** (`:ro`); public CI gates on `cargo fmt`. **Email ingest is CUT
  from v1.** One roadmap push per fresh context. Pre-v1 review **CLOSED**. Public Actions minutes
  are free; only the overlay's are metered (~37min/release) — the cost is wall-clock, not money.

## Do NOT re-survey

- **The branch gate and the doc archive.** Verified by real rejected/accepted pushes and an
  asserted line-census. History is in `.archive/v1.0.0/`; `tasks.md` is open work only.
- **The splash** (Android-verified) and **the import/seed** (every figure reproduced; 12277 events
  pulled back; `OMNI_VOLUME` resolves from the running container). Traps in `tasks.md`: the
  editor-bundle preload stays hoisted to app mount; `PullEvent::Applied` fires *after* the
  projection; **`#1e1e1e` is in FOUR files that must stay equal** (conf, themes, MainActivity, css).

## Open threads

- **Desktop splash is STILL UNVERIFIED** — settle it on the 1.0.0 AppImage, the first desktop
  artifact since `backgroundColor` was added. `grim` can't film this Wayland session. The 6s cap is
  likewise untested. The ~700ms black gap before the logo is the deferred `index.html`, not a bug.
- **Auto-import filter attribution OPEN** — ticks report `events=0`, but `filtered rows already in
  the journal` has never appeared. Watch for that line on a tick that returns data.
- Brokerage source needs an OTP Reconnect. Box runs **unauthenticated** — decide before the
  personal phone. Privacy guard can't catch "wise"; hand-grep public commits.
