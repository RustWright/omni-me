# Archived tasks — post-v1.0.0 daily use

Resolved and superseded work from `tasks.md`, cut during the **2026-09-05 reconciliation**.
Verbatim, with each item's original verification notes intact — with one exception: the real
box hostname is replaced by `omni-box-example` in two places, caught by the pre-commit privacy
guard. This is a public repo; the real host is in the overlay.

Companion to `.archive/v1.0.0/tasks-completed.md`, which holds everything up to the v1.0.0
tag. This file starts where that one stops: the period from the go-live wipe onward, with
the app in real daily use on the phone and `surface`.

**Why these were archived.** `tasks.md` claims to carry open work only, and had drifted far
from that: items fixed and verified on-device weeks earlier were still sitting unchecked,
so a session reading the file inherited a wrong blocker list. Each block below was closed
against evidence in the repo — a commit, a code path, or a user confirmation on real
hardware — never against the item's own prose.

Dispositions: **DONE** verified in code/git or confirmed on-device · **PARTIAL** the
resolved narrative of an item still awaiting on-device confirmation, whose live one-line
entry stays in `tasks.md` · **SUPERSEDED** overtaken by events, with the reason recorded.

---


## [DONE] Phase 6.2 branch-gate

*Archived from `tasks.md` lines 138-157.*

- [x] **6.2** Branch-gate workflow — **DONE 2026-08-31, scoped to safety rails (user).** Not
  PR-gating: `~/.dotfiles/scripts/session-end.sh` pushes straight to the current branch, and a
  require-PR rule would fail that push every session, stranding work locally and never
  advancing the parent submodule pointer. What landed instead protects the two irreversible
  cases. **Public repo:** GitHub rulesets `main-protection` (id 21894658 — `deletion` +
  `non_fast_forward` on `~DEFAULT_BRANCH`) and `release-tags` (id 21894665 — `deletion` +
  `non_fast_forward` + `update` on `refs/tags/v*`, making every release tag immutable).
  **Private overlay:** GitHub offers it *nothing* — it is private on a free plan, so both
  `/branches/main/protection` and `/rulesets` answer `403 Upgrade to GitHub Pro`. Same two
  guarantees enforced client-side by a tracked `scripts/git-hooks/pre-push` (symlinked into
  the per-device hooks dir; install line in the overlay README's "Branching" section).
  **Empirically verified, not assumed:** a real `git push origin :refs/tags/v1.0.0` was
  rejected with "Cannot delete this tag", an ordinary fast-forward push to `main` succeeded,
  and the overlay hook was driven through all four cases (delete-main → block, ff-main →
  allow, rewind-main → block, force-push a side branch → allow).
  **Finding worth keeping:** a `required_status_checks` rule **does** gate *direct pushes*,
  not merges only — GitHub's docs only mention merges, but adding `Formatting` as required
  rejected a plain `git push` to `main` with "Required status check \"Formatting\" is
  expected". It was removed for exactly that reason. Do not re-add it without fixing the
  session hook first. [S]



## [DONE] Phase 6.3 v1 semver + tag

*Archived from `tasks.md` lines 158-161.*

- [x] **6.3** v1 semver stamp + git tag — **DONE 2026-08-31.** `1.0.0` stamped in
  `Cargo.toml` (`[workspace.package]`), `tauri-app/frontend/Cargo.toml`, and
  `tauri-app/src-tauri/tauri.conf.json`; both lockfiles refreshed; CI green on the stamp
  commit; annotated tag `v1.0.0` pushed and now immutable under the `release-tags` ruleset. [XS]



## [DONE] Phase 6.4 archive + reset trackers

*Archived from `tasks.md` lines 162-171.*

- [x] **6.4** Archive + reset the trackers — **DONE 2026-08-31, cut at the v1.0.0 tag.**
  `project.md` 502 → 73 lines (current state + pointers only); `tasks.md` 1508 → ~250 (open
  work only). History moved to `.archive/v1.0.0/`: `project-history.md` (the full session log
  and Cycle 1–3 records) and `tasks-completed.md` (the whole pre-cut `tasks.md` verbatim —
  archiving only the finished half would have made a worse record). The split was scripted
  with an assertion that every line was accounted for and that the 79 top-level done blocks +
  36 open blocks matched an independent `grep` census; the assertion caught a real
  discrepancy first pass (tasks 1.8a/1.8b are *indented* sub-items carried by their parent,
  so 81 `[x]` lines are only 79 blocks). All 36 open items survived into the live file. [S]



## [DONE] CI release APK shipped stock Tauri logo

*Archived from `tasks.md` lines 178-206.*

- [x] **Every CI release APK shipped the STOCK TAURI LOGO instead of the enso — fixed in 1.0.2,
  on-device verify pending.** Spotted by the user on the installed v1.0.1. **Root cause:**
  `cargo tauri icon` writes the Android launcher set into
  `gen/android/app/src/main/res/mipmap-*/`, and **that whole tree is gitignored**. CI runs
  `cargo tauri android init` from scratch on every release, which lays down Tauri's *default*
  icons; the enso only ever existed on whichever machine had last run the icon command locally.
  Desktop was never affected — its icons live in the tracked `src-tauri/icons/` and are wired
  through `bundle.icon`, which is why the AppImage always looked right and nobody caught it.
  Note `android-overrides/` could not have covered this: it copies single named files
  (manifest, MainActivity, themes), and this is ~60 generated PNGs across every density.
  **Fix:** `scripts/android-build.sh` regenerates the icons from
  `branding/logo-manifest.json` before building. The branding SVGs and manifest are tracked, so
  the icons stay **derived, not stored** — no binaries in git and no way for them to drift from
  the source of truth. It lives in the build script rather than the CI workflow because CI
  invokes that script *after* `android init`, so one change fixes the local and CI paths at once.
  A guard fails the build if the adaptive foreground is missing afterwards, rather than silently
  shipping the robot a third time.
  **Verified locally without a full build:** deleted the mipmaps to simulate a fresh `init`, ran
  the regeneration, and the result is **byte-identical** to the known-good enso.
  **Second-order problem caught while testing:** `cargo tauri icon` also rewrites the *tracked*
  desktop `icons/icon.icns`, and that file does not pack deterministically — same input, same
  size, different bytes every run. Left alone it dirties a tracked file on every build, the same
  trap `gen/schemas/` set. The script now snapshots and restores `icons/` around the call, so
  only the gitignored Android tree changes; confirmed zero tracked churn.
  **VERIFIED ON-DEVICE 2026-08-31:** App info for the installed 1.0.2 renders the enso — blue
  open brush ring with the core dot on charcoal — instead of the stock Tauri mark. (APK resource
  names are AAPT2-mangled, so byte-comparing inside the archive is unreliable; what the system
  actually draws is the check that counts.) [DONE]



## [DONE] Desktop OTA round-trip proven end-to-end

*Archived from `tasks.md` lines 207-225.*

- [x] **DESKTOP OTA ROUND-TRIP: PROVEN END-TO-END.** Driven headlessly on a persistent `Xvfb :99`
  with `xdotool` (user installed it on request), against an **absolutely**-pathed isolated
  `XDG_DATA_HOME`; the real `~/.local/share/com.omni-me.app` mtime was identical before and
  after, so nothing touched user data. Sequence, all verified by screenshot:
  1. Fresh 1.0.0 install → backfilled **12315 events** and rendered real synced journal content;
     the **"Restoring N events…"** chip works on desktop.
  2. Settings → App Updates shows **"Desktop · automatic self-update"**; *Check for updates*
     offers **`1.0.0 → 1.0.1`** with the release notes.
  3. *Update & restart* → "Downloading & installing…" → the AppImage **replaced itself in place**:
     sha `68871ebf…` → `56371c2a…`, which is **byte-identical to the published
     `omni-me_1.0.1_amd64.AppImage`** on the box (full sha compared, not a prefix). Minisign
     verification is mandatory in the plugin, so a valid replacement is also proof the signature
     checked out.
  4. App **relaunched by itself** (new pid) and *Check for updates* now returns
     **"You're on the latest version (1.0.1)."**
  Note this also re-confirms the `dangerousInsecureTransportProtocol` fix in a way the earlier
  launch test could not: the updater plugin didn't merely initialize, it actually *fetched and
  installed* over the plain-http tailnet endpoint. Desktop is DONE. [DONE]



## [DONE] Android OTA install broken - Rust/Kotlin dir mismatch

*Archived from `tasks.md` lines 226-292.*

- [ ] **ANDROID OTA INSTALL IS BROKEN — Rust and Kotlin write/read DIFFERENT directories.**
  This is the defect step 9 existed to find ("fix it if the OTA path is broken"). Everything up
  to the handoff works: the manifest is fetched, `1.0.0 → 1.0.1` is offered with notes, the APK
  downloads, the sha256 verifies, and the UI advances to *"Follow the system prompt to finish
  installing."* Then **nothing happens** — no installer dialog, no exception, no log line.
  **Root cause (from source, not inferred):**
  - Rust `request_android_install` writes the side-file to `app_local_data_dir()`, which Tauri
    resolves on Android via `getDataDir` → **`activity.dataDir`** = `/data/user/0/<pkg>`
    (`tauri-2.10.3/mobile/android/src/main/java/app/tauri/PathPlugin.kt:64-69`).
  - Kotlin `checkInstallRequest` reads **`File(filesDir, "install_request")`** =
    `/data/user/0/<pkg>/files/install_request`.
  - `getFilesDir()` is `getDataDir()/files` — a *different* directory. The poller runs correctly
    every 1500ms, finds no file, and `return`s silently. Hence zero diagnostics.
  **Ruled out along the way:** the `REQUEST_INSTALL_PACKAGES` appop was at `default` and
  granting it (`adb shell appops set ... allow`) changed nothing — a red herring. The
  android-overrides ARE in the CI APK (the manifest-declared permission proves `build.rs`
  applied them). The app was foregrounded/resumed, so `onResume` had started the poller.
  **⚠️ THE SHARE-TARGET CHANNEL SHARES THIS ASSUMPTION AND MAY BE BROKEN TOO.**
  `commands/share_intent.rs:33-37` carries a comment asserting "on Android this resolves to the
  same `filesDir` that `MainActivity.kt` writes to" — that assertion is **false** for this Tauri
  version. Kotlin writes share bytes to `filesDir`; Rust reads `dataDir`. Share-target was
  verified on-device in Cycle 3, so either it regressed with a Tauri upgrade or the earlier
  `getDataDir` returned `filesDir`. **Do not assume either way — retest share-target on-device.**
  **FIXED in 1.0.2 (code) — on-device verification pending 1.0.3.** Took option (a): Kotlin gains
  a `bridgeDir()` helper returning **`dataDir`**, used for BOTH channels, so Rust keeps the
  documented Tauri API and the two sides agree. The share-target write moved with it, fixing that
  channel in the same stroke. The false comment in `share_intent.rs` is corrected to state what
  `app_local_data_dir()` actually resolves to on Android, with a note that changing one side
  requires changing the other. Added `Log.i` on both *request found* and *intent fired* so this
  can never fail silently again — the absence of any log was the hardest part of diagnosing it.
  **A first draft of the fix also carried a `filesDir` "legacy fallback"; it was removed after
  checking the history** — Rust has written this file to `app_local_data_dir()` since the OTA
  feature landed and never once wrote to `filesDir`, so that branch was unreachable and its
  comment described a history that never happened. Exactly the failure mode being fixed.
  **Verification needs TWO releases and that is unavoidable:** 1.0.2 carries the fix, so it
  cannot be delivered by the path it repairs — it goes on by cable/browser, and 1.0.3 exists for
  it to update *to*. `is_newer` is strictly-greater and fails closed on downgrades, so there is
  no way to re-test against an older build.
  **BRIDGE FIX VERIFIED ON-DEVICE 2026-08-31 — by the best evidence available.** On its first
  boot, 1.0.2's fixed poller picked up the install request **1.0.0 had stranded** hours earlier
  (written by Rust to `dataDir`, invisible to the old Kotlin reading `filesDir`) and fired the
  system installer unprompted. The log lines added with the fix name the exact path:
  `install request found at /data/user/0/com.omni_me.app/install_request`, then
  `install intent fired for .../cache/omni-me-update.apk`. That is precisely the file the broken
  reader could not see, found where Rust has always written it — and it confirms the removed
  `filesDir` fallback was unreachable. The offered APK was the stale cached 1.0.1 (a downgrade),
  so it was cancelled rather than installed.
  **Still owed: the clean 1.0.2 → 1.0.3 round-trip, run with `REQUEST_INSTALL_PACKAGES` reset to
  `default`.** During diagnosis that appop was granted via `adb appops set … allow`; it made no
  difference then (the path bug was the real cause), but it is a workaround the user must not
  need on their personal phone, so the final proof has to run without it. Reset to `default`
  before the round-trip.
  **ROUND-TRIP COMPLETE UNDER STOCK PERMISSIONS — ANDROID OTA IS PROVEN (2026-08-31).** Appop
  reset to `default` first, then `1.0.2 → 1.0.3` entirely in-app: manifest fetched, update
  offered with notes, APK downloaded + sha-verified, request found at `dataDir`, intent fired,
  and the phone ended on **versionCode 1000003 / versionName 1.0.3** with device id, sync and
  settings all intact.
  **What the user must tap on a fresh phone — two one-time Android prompts, no adb, no
  workaround:** (1) *"your phone is not allowed to install unknown apps from this source"* →
  **Settings** → **Allow from this source** → back; Android returns straight to the install
  dialog. (2) **Google Play Protect** *"App scan recommended"*, which offers only *Scan app* /
  *Don't install app* up front — **"Install without scanning" is hidden behind the "More
  details" expander.** That matters for privacy: *Scan app* uploads the APK to Google, and these
  APKs bake the box hostname, so **"Install without scanning" is the right choice** and is worth
  writing into the install guide. Neither prompt can be pre-granted by the app; both are normal
  Android sideload behaviour and both are one-time per source. [DONE]



## [DONE] Android showed the UTC date, not the local one

*Archived from `tasks.md` lines 293-344.*

- [ ] **ANDROID SHOWS THE WRONG DAY: "Today" resolves to the UTC date, not the local one.**
  Caught by comparing the two platforms side by side at 23:42 CDT on 2026-08-30 — both devices
  report `America/Winnipeg`, both agree at the OS level (`adb shell date` → `Sun Aug 30 23:42
  CDT 2026`), but:
  **phone app "Today" = `2026-08-31`** (the UTC date) vs **desktop app "Today" = `2026-08-30`**
  (correct). So from ~19:00 local onward, every journal entry written on the phone is filed
  under *tomorrow* — wrong `date:` in frontmatter, wrong calendar cell, wrong day-complete, and
  it diverges from the same day's entry on desktop.
  **This is a known-and-supposedly-fixed failure mode whose fix is not holding on Android.**
  `frontend/src/pages/journal.rs:175-200` carries a long comment describing precisely this:
  `UserDate::today` runs on the first render while `tz_signal` still holds the `Tz::UTC` default
  (`main.rs:269` — `invoke_get_timezone` is async and cannot have resolved yet), `use_signal`
  freezes that seed, and a `use_effect` is meant to re-anchor once the real zone arrives. The
  comment even predicts the compounding: the bad date is written straight into nav by the
  write-through effect, so the next launch restores it and "the error outlived the fix."
  **Not yet root-caused — do not assume which half fails.** Candidates, in order: (1) the
  frontend `tz_signal` never resolves on Android, so the re-anchor has nothing to fire on — note
  the BACKEND does know the zone (`App initialized ... timezone=America/Winnipeg` in logcat), so
  the gap would be the invoke, not detection; (2) the re-anchor fires but the
  `selected_date.peek() == anchor` guard declines to move it; (3) nav restore re-seeds the bad
  date before the effect runs. Diagnose live with WebView DevTools over adb
  ([[reference-android-webview-devtools-over-adb]]) and read `tz_signal` directly rather than
  reasoning from the code.
  **Priority: this is a daily-use blocker for evening journaling on the PERSONAL phone**, which
  is the next milestone. The user journals at night — this session is itself at 23:42.
  **ROOT-CAUSED + FIXED in 1.0.2; on-device verify pending.** It is the **same dropped-invoke
  race** that hung the fresh-install boot in July, in a second place. `invoke_get_timezone` used
  the plain `invoke(...)`, and on Android's cold open an invoke issued before the native IPC
  handler is ready is silently dropped — the promise never resolves *and* never rejects. The
  `use_future` therefore parked forever, `tz_signal` stayed at its `Tz::UTC` seed for the whole
  session, and every `UserDate::today` returned the UTC date. Candidate (2), the re-anchor
  guard, was wrong: the effect never had anything to fire on. Desktop was immune because the
  race is Android-only.
  **Fix:** since this is the *second* instance of the class, the timed-invoke logic was
  generalized into `bridge::invoke_timed` (racing the invoke against a `setTimeout`);
  `invoke_get_workspace_timed` now delegates to it with an unchanged signature so the proven
  boot path is untouched, and a new `invoke_get_timezone_timed` uses it behind the same
  bounded-retry loop `continuity.rs` uses (500 ms/attempt, 100 ms gap, 15 s fail-open cap).
  **Use `invoke_timed` for anything fired during boot** — that guidance is on the helper.
  **VERIFIED ON-DEVICE 2026-08-31 (1.0.0 vs 1.0.2, same phone, same data).** The natural window
  had passed — after midnight CDT local and UTC agree, so a buggy build looks correct — so the
  divergence was forced through the app's OWN timezone override rather than by touching system
  settings. No root required, and it reproduces at **any** hour: set Timezone to
  `Pacific/Honolulu` (UTC-10), restart, compare.
  **1.0.0:** backend logged `timezone=Pacific/Honolulu`, app showed **"Today 2026-08-31"** (the
  UTC date) — reproduced on demand. That also *disproved* the re-anchor-guard hypothesis: the
  backend had the right zone all along; the frontend simply never received it.
  **1.0.2:** same override, same phone (`adb install -r`, so data and override carried over) →
  **"Today 2026-08-30"**, with `file_path: 2026/August/W36/2026-08-30-note` confirming the entry
  files under the correct day. Keep this override trick — it is the only way to test this bug
  outside the ~19:00–24:00 local window. [DONE]



## [DONE] Android baseline installed from CI, verified on S9

*Archived from `tasks.md` lines 345-364.*

- [x] **Android baseline installed from CI and fully verified on-device (S9).** `adb uninstall`
  → `adb install` of the release-signed 1.0.0 APK (versionCode 1000 → 1000000). **The cert
  mismatch was proven, not assumed:** a plain `adb install -r` over the debug build first
  returned `INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match`, leaving the phone
  untouched — so the uninstall really was required.
  **Fresh-install results:** new `device_id`, `ever_synced=false, total=0` at boot, then
  **`auto-pull applied; pulled=12314, failed=0`** — the entire box contents backfilled with zero
  failures. UI confirmed by screenshot: journal renders, header shows **"Synced"** green, the
  daily-note template and tags are intact, no crash or panic in logcat.
  **This closes the long-pending `OMNI_DEFAULT_SERVER_URL` item** (open since 2026-07-21 as
  "[USER] on-device fresh-install confirm"): Settings shows **Sync Server Address =
  `http://omni-box-example:3000`** on a device with no persisted value, proving the compile-time
  default injected by private CI reaches a genuinely fresh install.
  **Android update-CHECK path proven too:** Settings → App Updates → *Check for updates* against
  a box serving 1.0.0 returns **"You're on the latest version (1.0.0)"** — so manifest fetch,
  parse, and the `is_newer` comparison all work over the tailnet. The download+install half
  awaits 1.0.1. **Data note:** the phone held 30 own events vs 28 on the box; the 2 unsynced
  were journal edits / batch dismissals and the user chose to accept the loss (test-phone data
  is disposable by standing decision).



## [DONE] Every CI desktop AppImage panicked on startup

*Archived from `tasks.md` lines 365-400.*

- [x] **Every CI desktop AppImage since 0.2.0 PANICKED ON STARTUP — the desktop release has
  never once run.** Found the moment the 1.0.0 AppImage was actually launched (isolated
  `XDG_DATA_HOME`, user data untouched):
  `PluginInitialization("updater", "The configured updater endpoint must use a secure protocol
  like `https`.")` — a panic before any window appears. **Reproduced on 0.2.0 too**, so it is
  pre-existing from 2026-06-29, not introduced by the v1.0.0 work.
  **Root cause, from the plugin source (`tauri-plugin-updater-2.10.1/src/config.rs`,
  `validate_endpoints`):** a non-https updater endpoint is `#[cfg(debug_assertions)]` → *warn*,
  `#[cfg(not(debug_assertions))]` → *hard error*. The private CI injects
  `http://<box>:3000/updates/desktop/latest.json`. So the defect is **invisible to
  `cargo tauri dev` and fatal in exactly the artifact CI produces** — which is why months of
  desktop development never hit it.
  **Why the 2026-06-29 "verified over the tailnet" check missed it:** that check confirmed the
  AppImage was *served* (HTTP 200 + valid sig). Nothing in the pipeline ever *launched* it.
  The lesson is [[feedback-smoke-run-binaries-before-handoff]] applied to CI artifacts, not
  just local builds — a release that has never been executed is not verified.
  **Fix (private `app-release.yml`, one key):** `dangerousInsecureTransportProtocol:true` in
  the `--config` updater block. **Verified at the library level before rebuilding** with a
  standalone probe pinned to `=2.10.1` and `debug-assertions = false`: http without the flag →
  REJECTED with the identical error, http with the flag → ACCEPTED, and a *typo'd* key →
  REJECTED (proving the name is load-bearing rather than silently ignored — the failure mode
  [[verify-syntax-before-writing]] warns about).
  **Security note (user-approved 2026-08-31):** the flag relaxes the *transport* requirement
  only. Both properties https would supply are already provided independently — the tailnet is
  a WireGuard tunnel (encrypted + authenticated, no middleman position), and the artifact is
  minisign-verified against a mandatory pubkey whose private half lives only in Actions
  secrets. **Revisit if `/updates` ever leaves the tailnet**; on a public endpoint the flag
  becomes genuinely unsafe. Rationale is recorded in full at the injection site.
  **FIX VERIFIED on the rebuilt artifact (2026-08-31).** Desktop-only rebuild of 1.0.0 (the
  public source at `v1.0.0` was never wrong — the defect was entirely in the private CI's
  injected config — so the version stays matched to the source tag rather than burning a 1.0.1
  on it). Downloaded the republished AppImage and ran it against an isolated `XDG_DATA_HOME`:
  **no panic, process stayed alive**, and the log shows a clean
  `App initialized server_url=http://omni-box-example:3000` — which incidentally also confirms
  the CI-baked `OMNI_DEFAULT_SERVER_URL` reaches a fresh install. The user's real
  `~/.local/share/com.omni-me.app` was confirmed untouched by mtime before and after. [BUG, high]



## [DONE] unused_mut warning on every Android build

*Archived from `tasks.md` lines 443-450.*

- [x] **`unused_mut` warning on every Android build — DONE 2026-08-31.**
  `tauri-app/src-tauri/src/lib.rs` (`let mut builder = tauri::Builder::default()`) only needs
  `mut` on desktop, where the `#[cfg(desktop)]` updater-plugin block reassigns it; on Android
  that block is compiled out, so every APK build printed the warning. Fixed with
  `#[cfg_attr(not(desktop), allow(unused_mut))]` on the binding. The Android cfg can't be
  exercised by a desktop clippy run, so the attribute shape was proven separately with a
  standalone `rustc` probe under `#![deny(unused_mut)]` (a `cfg_attr`-gated `allow` on a `let`
  statement does apply). Desktop clippy `-D warnings` clean, 73 app tests green. [XS]



## [DONE] Android build dirtied a tracked generated file

*Archived from `tasks.md` lines 451-461.*

- [x] **An Android build dirtied a TRACKED generated file — DONE 2026-08-31, gitignored.**
  `gen/schemas/acl-manifests.json` is regenerated per-target; an Android build drops the
  **`updater`** key (the plugin is `#[cfg(desktop)]`), so committing after an APK build would
  have quietly removed the desktop updater's manifest entry. The whole
  `tauri-app/src-tauri/gen/schemas/` directory is now gitignored and `git rm --cached`'d —
  it's build output, and tracking per-target build output is what created the hazard.
  **Verified regeneration before untracking** rather than assuming it: deleted the directory,
  touched `build.rs`, ran `cargo check -p omni-me-app` → the four desktop-target files came
  back byte-identical to the originals (and only the desktop subset, which is precisely the
  churn). Same `cfg(desktop)` root as the warning above. [XS]



## [DONE] CI AppImages unrunnable on go-live desktop (glibc floor)

*Archived from `tasks.md` lines 464-487.*

- [x] **Every CI desktop AppImage was unrunnable on the go-live desktop — glibc floor set by the
  build host. FIXED 2026-08-31.** `surface` (Ubuntu 22.04, glibc 2.35) rejected the 1.0.3 AppImage
  with `GLIBC_2.3x not found` for several libraries. An AppImage bundles the app's own libraries
  but **never glibc**, so the build host's version becomes the artifact's minimum — and
  `build-desktop` ran on `ubuntu-latest` (24.04, glibc 2.39). **Invisible from the dev laptop,
  which is also 24.04**, so every local launch test passed; this is the same acceptance-test gap
  as the "pipeline cannot tell a launchable release from an unlaunchable one" item below.
  **Fix: pin `build-desktop` to `ubuntu-22.04`** (Android is unaffected — an APK has no glibc
  dependency, and that job still uses `ubuntu-latest`).
  **Two follow-on defects surfaced while fixing it, both worth keeping:**
  (1) `dx` publishes **only `-gnu` archives** built against a newer glibc and has no musl variant,
  so binstall's `dx` itself died with `GLIBC_2.38 not found` on the pinned runner — it is now
  compiled from source there. `cargo-tauri`'s prebuilt binary is fine.
  (2) That fix was then **silently defeated by the cache**: `Swatinem/rust-cache` caches
  `~/.cargo/bin` and keys on `Linux-x64` **without the Ubuntu version**, so the job restored a
  24.04-built `dx` and `cargo install` reported "already installed" and no-op'd in ~1s. Fixed with
  `--force` plus a cache key namespaced to the runner image, and a `dx --version` guard so a
  non-running CLI fails at the install step instead of surfacing later as an opaque
  `beforeBuildCommand` error.
  **Verified at the binary level, not by CI going green** — precisely the acceptance test that let
  this through: the published 1.0.3 AppImage's highest required symbol is `GLIBC_2.35` (bundled
  libs top out at 2.34), and it then installed and ran on `surface`. Cost accepted: `--force`
  makes every desktop release recompile `dx` (~10 min); droppable now the guard exists. [S, CI]



## [DONE] Wise proposed 4 already-ledgered rows (wipe sequencing)

*Archived from `tasks.md` lines 488-513.*

- [x] **Wise proposed 4 rows already in the ledger — NOT a dedup bug. A wipe-sequencing artifact,
  and the runbook lesson is the keeper.** Reported from the review inbox: one correct new row
  (2026-08-31 transfer) alongside four already-recorded ones from 2026-08-28 and earlier.
  **What the timestamps prove:** the poller ticked at **06:41:11**, seconds after the container
  restarted on the freshly wiped volume, when the box held **zero** events. The imported ledger
  did not land until **06:55:36** (the seed push). So all three of the source's dedup filters were
  blind by construction — no `wise-id:` tags yet, no `autoimport-id:` provenance, no prior
  proposals (the wipe erased those). The **08:41:18 tick appended 0 events**, confirming dedup
  works now that the 150 `wise-id`-tagged transactions are present.
  **RUNBOOK — for any future wipe: pause the auto-import sources before wiping, or seed the box
  before the first poll.** The gap between "container healthy" and "seed pushed" was ~14 minutes,
  and a source on a 30-min interval fires inside it. The health gate passing is not the same as
  the box being ready for a poller.
  **Two dead ends recorded so they are not re-walked.** (1) `budget.journal` shows only 5
  `wise-id` strings against 155 in the ledger, which looks exactly like the importer dropping
  header tags — it is not: the rendered projection simply does not emit top-level tags, and the DB
  is the artifact that matters. Verified in the event payloads: `tags:
  ["wise-id:BALANCE-5957939544"]`, **150** events carrying a `wise-id` top tag, matching the count
  `wise.rs` documents. (2) The repeated `BALANCE-…`/`CARD-…` external ids in one batch are not
  duplicates but the **two legs** of currency-crossing transactions (`-404.83 USD` / `+559.47 CAD`;
  a card charge funded from both balances). Each leg is a real balance movement.
  **Latent risk worth keeping in view:** dedup keys on the *reference number*, which multi-leg
  transactions share. If one leg becomes "known" while the other has not been recorded, the second
  leg is silently filtered out. Not observed yet — flagged because the failure mode is a missing
  transaction, not a visible duplicate. [watch]



## [DONE] Auto-sync never fires

*Archived from `tasks.md` lines 584-584.*

- [ ] **Auto-sync never fires.** Edits save locally but don't propagate until the **Sync** button is pressed manually, despite both devices connected to the Hetzner server. Applies to journal, notes, tasks — everything. (dogfooding 2026-07-04) [?]



## [PARTIAL] Content/body + ledger edits across sync - code fix + root cause

*Archived from `tasks.md` lines 585-601.*

- [ ] **Content/body edits don't materialize on the receiving device.** Manual sync of a journal entry shows **"1 up"** on the sender and **"1 down"** on the receiver, but the received entry stays **BLANK**. Same for **note bodies** — a note's *creation* syncs (the empty note appears on the other device) but typed **text** does not. So *creation* events and *task create/complete* propagate, but **text-content updates don't apply**. Likely a projection/event-apply gap for body-content events (journal continuity / note body), not the transport (counts move). **Headline bug — breaks the multi-device premise.** [L?] — **CODE-FIXED for notes + journal (commit `d049f1e`, Session 6):** `on_journal_updated` / `on_generic_updated` now **UPSERT** so a body edit materializes even when this device never saw the create (lost to an old batch-abort). **Pending on-device confirm** (pairs with 305 + the on-device sync pass). *Not yet checked off — awaiting real-device verification.*
- [ ] **Ledger transaction edits don't update on sync / across devices.** This session's in-app txn edit isn't reflected after sync. (may share the content-apply root cause) [?] — **ROOT-CAUSED + CODE-FIXED 2026-08-10 (commit `c5e55a6`, whole-app review):** the `transactions` projection was left out of the Session-6 sync-resilience pass (`d049f1e` fixed notes/journal only) — every mutation handler (`categorized`/`tagged`/`updated`/`cleared`) used a bare `UPDATE`, which **silently no-ops** in SurrealDB when the row is absent, so an edit applied without its (lost/unsynced) `TransactionRecorded` vanished. Fix = those four handlers now **UPSERT-materialize** with SCHEMAFULL-required `?? default` backfills; `on_transaction_recorded` became an idempotent order-safe UPSERT (fills a partial row without clobbering an already-applied edit); `deleted`/`merged` deliberately stay no-op-if-absent (documented). +2 core tests (`mutation_without_create_materializes_row`, `create_after_mutation_fills_without_clobbering_edit`); rebuild-equivalence tests still green. **Pending on-device confirm** (real multi-device replay, on the sync pass). *Not yet checked off — awaiting real-device verification.*

**ROOT CAUSE (triaged 2026-07-04, code-confirmed) — three intertwined event-sourcing bugs. SYNC FIX IN PROGRESS — Session 6 (2026-07-05).**
1. **Journal aggregate-identity mismatch.** `journal_entries` rows are keyed by **date** (`type::record('journal_entries', $date)`), but `journal_id` is a **per-device ULID** stored as a field. `on_journal_updated` updates `WHERE journal_id = $journal_id`. Two devices each mint their own `journal_id` for the same day → an incoming `journal_entry_updated` from device A matches **no row** on device B (whose row carries B's id) → the entry stays **BLANK**. And an incoming `journal_entry_created` for a date that already exists locally → SurrealDB `CREATE` **errors** (record exists). (`core/src/events/notes_projection.rs`) — **✅ FIXED (d049f1e):** the journal aggregate identity IS now the **date** (`create_journal_entry` mints `journal_id = date`, deterministic + device-independent). All journal handlers route by the date-keyed record id (`aggregate_id`); `on_journal_created` is an idempotent **UPSERT** (two devices' creates converge, no collision, rebuild-safe); llm routes by record id. Needs the **date-as-ID reset cutover** (old random-ULID events regenerated by the queued re-import — user chose this, option 1). — **✅ CUTOVER DONE 2026-07-05:** box wiped + clean re-import + seeded; all journal events on the box are now **date-keyed** `journal_entry_created` (the importer date-keys too). Old random-ULID journal aggregates are gone.
2. **Fail-fast apply + pre-advanced sync cursor = silent permanent data loss.** `pull_only` (`core/src/sync/client.rs`) appends pulled events **and advances `last_sync_timestamp`** *before* projections run; then `trigger_sync` calls `apply_events`, which is **fail-fast** (`projection.rs:85` `?`). So one failing/colliding event (e.g. the bug-1 `CREATE` collision) **aborts the rest of the pulled batch** — and because the cursor already advanced, those events are **never re-pulled or re-projected** (only a full `rebuild()` recovers them). Explains "imported data absent on mobile" + inconsistent "some things sync, some don't". — **✅ FIXED (d049f1e):** new `ProjectionRunner::apply_events_resilient` (skip-and-log a failing event, never abort the batch; returns a failure count) is used on **both** sync-pull apply paths (`trigger_sync`, `sync_back_after_llm`). Local single-event commands keep strict `apply_events` (surface the user's own error). Events are durably appended before apply, so nothing is lost — a `rebuild()` recovers, and with the now-idempotent projections a replay is a no-op for applied events.
3. **Non-upserting body updates.** `on_generic_updated` / `on_journal_updated` are `UPDATE … SET` — they **no-op** if the create hasn't landed yet (ordering, or a prior batch-abort). Should be idempotent upserts. — **✅ FIXED (d049f1e):** journal + generic-note update/rename/create handlers are all **UPSERT** now (materialize a full valid row if the create was missed, preserving closed/tags/summary; `title ?? ''` / `date ?? aggregate_id` supply SCHEMAFULL-required fields). +3 new core tests (two-device converge, orphan-update materializes, resilient skip). 477 core tests green, clippy clean.
- **Auto-sync never fires** — **✅ FIXED (backend, 9cdf69d).** Root cause confirmed: `append_and_apply` (`commands/shared.rs`) **never fed the `SyncBuffer`**, so the `PushDebouncer` never woke on an edit; **and there was no pull scheduler at all**. Fix: (push) `append_and_apply` now calls `push_debouncer.trigger()` after each edit → auto-push; (pull) new `core::sync::PullScheduler` (startup backfill + 20s interval + network-online nudge) pulls remote events and applies them via `apply_events_resilient`, wired in `lib.rs`. On a pull that lands new events it emits a `sync:applied` window event. +2 puller unit tests; core+app clippy clean.
  - **DEFERRED (explicit, its own focused pass): frontend live-refresh.** The backend now propagates data to both devices' DBs automatically and emits `sync:applied`, but **no page consumes it yet** — the frontend has zero Tauri event-listen infra, and each page's fetch is a bespoke `use_future` with careful in-flight-edit-preservation. Making open pages live-update means an event-listen bridge + a root `SyncTick` + per-page refetch that respects dirty state — cross-cutting frontend reactivity work that must not be rushed (risk: refetch loops / clobbering unsaved edits). Until then: pulled data shows after any navigation or a manual Sync. [frontend, M — own session]
- **NEW BUG found + fixed 2026-07-05: `413 Payload Too Large` on bulk push.** The client chunked pushes at a **fixed 100 events**, but the server's `/sync` body limit is **256 KiB** (`server/src/lib.rs`) — 100 transaction events overflow it, and the seeding push died mid-stream (10 200/11 844). This is a **real product bug** (the deployed app shares the path). **✅ FIXED (public core, `sync/client.rs`):** push now chunks by **bytes AND count** (`chunk_for_push`, ≤100 events & ≤200 KiB/req; an oversized single event is still sent alone). Extracted as a pure, unit-tested helper (+3 tests: count cap, byte split, oversized-single). Core tests + clippy green. *(Headroom follow-up: the 256 KiB server limit is tight for a batch — bump it when the server is next rebuilt.)*
- **DURABILITY guardrails (user Option 1, 2026-07-05) — the LLM-primary-interface push will keep the grammar churning, so these are load-bearing.** ✅ done: byte-chunk unit test; the canonical-source reconcile is proven manually. **✅ ALL THREE BUILT 2026-07-06 (own planning-first session, plan `binary-puzzling-lerdorf.md`):** (1) **CI golden-reconcile** — `core/tests/golden_reconcile.rs` + synthetic fixture `core/tests/fixtures/golden/main.ledger` (fictional Northwind/Globepay/Summit/Meridian; elided leg, multi-commodity FX, `@@ TOTAL`-cost buy+sell, posting+header tags). Two independent paths (full pipeline via `ledger::balances` + direct draft-sum) both asserted against a frozen per-account/per-commodity table; cross-checked once vs the paisa-bundled `ledger 3.3.2` (all 8 accounts exact). Rides the existing `cargo test -p omni-me-core` step — no CI-workflow change, no `ledger` binary in CI. (2) **single canonical event-builder** — `TransactionRecordedPayload::new`/`with_*` (core `events/types.rs`) + `NewEvent::{transaction_recorded,journal_created,generic_note_created}` factories (`events/store.rs`) that **derive `aggregate_id` from the payload id** (the journal-key drift is now structurally impossible). Rewired all 4 public sites (budget `record_transaction` + statement-CSV import, journal_import, auto_import, notes create-journal/create-note via a new `append_new_and_apply` tail) **and** the private `headless_import.rs` (separate overlay commit). (3) **sync orphan-device-id self-check** — new `core/src/sync/diagnostics.rs` (`DeviceIdAudit` + `orphan_signature` + `audit_device_ids`; GROUP BY device_id + `sync_state` ever-synced), wired non-fatal into `lib.rs` startup (info! distribution, warn! on orphan). Verified: 494 core lib tests + golden-reconcile green; core/app/server clippy `-D warnings` clean; private overlay 26 tests + example clippy clean; leak-check PASS (denylist scan of added lines clean, fictional cast only). **Uncommitted — git push is user-triggered.** Post-v1: event schema-versioning + upcasting / migration-as-events (eliminate wipe+reimport when grammar changes).
- **REMAINING — Step 5: Fresh-device backfill + cold-open hang** (the SEVERE-UX item below). Backfill is now handled by the PullScheduler's startup pull (4s warm-up so it doesn't contend with first-open reads). The **cold-open hang** root cause is still UNVERIFIED (the "backfill locks the DB" note can't have been the *original* cause — nothing auto-pulled on the build where it was reproduced). Do NOT fix blind. [M, → next device cycle]
  - **✅ INSTRUMENTATION BUILT (5c1f2ec): `startup_probe`** — timestamped checkpoints to `<app_data>/startup-timing.log` (+ mirrored to `tracing`). Chose a **timing file over a logcat bridge** (no Android-NDK link risk; the file survives the hang and is pullable). Checkpoints: `setup:{begin,db_connected,projections_init_all,config_loaded,engines_spawned,managed}` + first `cmd:get_workspace:{begin,end}` + first `cmd:get_journal_by_date:{begin,end}`, then the probe self-deactivates (no noise on normal navigation). Each process launch writes a `==== boot pid=… ====` header so a fresh-install run and a reopen run sit side-by-side.
  - **How to read it next device cycle:** reproduce via uninstall→reinstall→cold first-open (the *only* repro). The last checkpoint before the ~2min gap localizes the hang (setup `block_on` vs first workspace read vs first journal DB read). Retrieve on Android (debug APK is debuggable, so `run-as` works without root): `adb shell run-as com.omni-me.app cat files/startup-timing.log` (confirm the `files/` path on device; `app_data_dir` is private storage — plain `adb pull` won't reach it). On desktop it's at `~/.local/share/com.omni-me.app/startup-timing.log`. **Remove the probe once the hang is root-caused.**
  - **✅ DESKTOP TEST (2026-07-05) — probe validated + hang NARROWED to Android-only.** Built the debug desktop binary + ran it twice on `:0`: (1) warm (existing DB) and (2) **simulated fresh install** (moved the real app-data dir aside → empty DB + cold webkit cache + default `localhost:3000`; restored after). BOTH boots completed in **~4.5s total** with the journal read at **~5ms** — **no hang on desktop, even fresh.** Timeline: `setup:begin +0` → `db_connected +1.1–1.5s` → `init_all/config/engines/managed +1.7s` → (frontend WASM load ~2.9s) → `get_workspace +4.5s` (instant) → `get_journal_by_date +4.5s` (~5ms). **Consequence:** the shared **backend** (setup, DB connect, projections init, journal read) is NOT the cause — it runs identically on both platforms and is fast here. The hang is **Android-specific**: the Android WebView cold-load / WASM init, the tauri asset-protocol serving the embedded frontend, or Android's SurrealKV filesystem perf. **What the Android log will disambiguate:** if `cmd:get_workspace`/`cmd:get_journal_by_date` FIRE (then hang) → it's a backend read on Android's FS; if only `setup:*` appear and then nothing → the frontend/WebView never finishes loading (hang is *before* any backend call, in the WebView asset load). Note: fresh install has NO workspace.json → app defaults to the journal route (matches "journal hangs"); a warm run restores the last tab (opened on Settings this session — intentional continuity, decision #4).

**SEVERE UX:**



## [SUPERSEDED] '...more to come' + orphaned tail of a DONE item

*Archived from `tasks.md` lines 610-635.*

- [ ] **...more to come** — user's list was still going ("and it goes on"); collect the rest before finalizing priorities.

 (dogfooding 2026-07-04, user flagged
  — "we'll need to tackle that eventually"; scope confirmed = **ledger/budget transactions**, not
  daily notes). **DONE 2026-07-04.** The friction note was stale: a `TransactionUpdated` event +
  `TransactionUpdatedPayload` (schema-flexible `changes` bag) + `on_transaction_updated` projection
  fold + a registered `update_transaction` command **already existed** (used internally by the 5.7
  reconciliation `resolve_unmatched` path) — the SurrealDB projection already amended
  date/description/postings. The real gaps were (1) the **journal file went stale on modification** —
  `JournalFile` only rendered Recorded/AccountAdded/ExchangeRate, so after any edit/delete
  journal-derived balances (Accounts + dashboard, which read `journal_artifacts()`) were wrong (even
  a full `rebuild()` replayed only the originals); and (2) **no frontend edit UI**. Fix, journal
  side: `journal_file.rs` now edits the entry **in place by its existing `; txn_id:<id>` anchor**
  (reuses the account-dedup block-splice primitive) — `TransactionUpdated`/`TransactionsMerged`
  re-render the one entry from the already-updated projection row; `TransactionDeleted` splices it
  out; prices + account directives are untouched and no whole-file regenerate/DB-scan is paid
  (chosen for on-device speed after weighing month-sharding — see the sequencing note above).
  Categorized/Tagged/Cleared stay no-ops (not part of the rendered entry). Fix, UI: new
  `TransactionEditForm` on the detail view (Edit button) for date/description/postings
  (account/amount/commodity, add/remove rows; each posting carries its original JSON so fx-rate/tags
  survive), Save → `update_transaction`; plus a two-step **Delete** affordance → `delete_transaction`
  → back to list. Bridge: `invoke_update_transaction` / `invoke_delete_transaction` (mock + real).
  Verified: 474 core lib tests (8 new — 6 pure block helpers incl. exact-id-boundary + directive
  preservation, 2 DB-backed edit/delete integration) + core clippy `-D warnings`; frontend clippy
  clean mock+default (wasm); app crate compiles; Playwright mock at 1280 + 390 — Edit seeds all
  fields, Cancel/Save/Delete-confirm wired, mobile layout wraps cleanly, 0 console errors. [M]



## [SUPERSEDED] Wire the overlay's per-source account maps

*Archived from `tasks.md` lines 649-651.*

- [ ] Wire the private overlay's **per-source account maps** so the account-map-based bank pollers actually emit drafts (they import 0 until wired — the private half of 3.9; receipt/email sources already work). **Deferred to polish (user, 2026-06-28).** [M]

**Phase-5 reconciliation/import deferrals (from Cycle 3):**

## [PARTIAL] Android predictive-text suggestions never auto-accepted
*Archived from `tasks.md` lines 547-578.* Live one-liner stays in tasks.md under 'Awaiting on-device confirmation'. The CodeMirror-6 mechanism finding below is the part worth keeping.
- [ ] **Android: the keyboard's predictive-text suggestions are never auto-accepted while typing,
  so writing in the app is slower than in Obsidian.** Reported by the user within minutes of the
  personal-phone install, while closing out the 2026-08-30 journal entry in omni-me instead of
  Obsidian — i.e. the very first real writing session on the go-live data, which makes it a
  daily-use friction item rather than a nice-to-have. In Obsidian, typing a word and hitting
  space commits the keyboard's suggestion; in omni-me it does not, so every word has to be
  typed out or tapped manually.
  **MECHANISM FOUND, from the installed source rather than inferred.** CodeMirror 6 sets these on
  its content element by default (`node_modules/@codemirror/view/dist/index.js`):
  `autocapitalize: "off"`, `autocorrect: "off"`, `spellcheck: "false"`, `translate: "no"`. Those
  are the exact attributes an Android keyboard reads to decide whether to offer and auto-commit a
  suggestion, so the behaviour is CM6 being a **code** editor by default — sensible for source,
  wrong for a journal. `editor.js` sets none of them, so nothing in this app overrode the default;
  the app's other inputs that disable autocomplete (`settings.rs`, `account_input.rs`) are
  unrelated fields.
  **FIXED + SHIPPED IN 1.0.4 (2026-08-31); on-device verification still owed.** `editor.js` gained
  a `proseInputAttributes` extension —
  `EditorView.contentAttributes.of({autocorrect: "on", autocapitalize: "sentences", spellcheck:
  "true"})` — added to the single `createEditor` extensions array (checked: there is only one
  `new EditorView` site, so this is not a one-of-N fix). `translate: "no"` is deliberately left at
  CodeMirror's default; a note should not be machine-translated in place.
  **Verified in a real browser before shipping**, not assumed: the rebuilt bundle was served to a
  harness page and the live DOM read back `autocorrect="on" autocapitalize="sentences"
  spellcheck="true"` on `.cm-content`, so the override does beat CodeMirror's defaults.
  **What is NOT yet established:** that flipping the attributes is *sufficient*. CM6 also has a
  long history of Android IME **composition** edge cases, so the keyboard may still fail to commit
  even with the attributes on. Verify on the device — drive the live WebView over adb with CDP
  ([[reference-android-webview-devtools-over-adb]]) and watch `compositionstart` /
  `compositionupdate` / `compositionend` / `beforeinput` while typing a word and pressing space —
  rather than declaring it fixed because the attribute changed. Shipping it to the phone needs a
  version bump (`is_newer` is strictly-greater), so it rides a 1.0.4. [editor, S–M]

## [SUPERSEDED] Cycle 4 header, v1 release roadmap, and the Session-4 design decisions
*Archived from `tasks.md` lines 1-137.* The whole pre-reconciliation header. The v1 roadmap and Phase 6 sequencing narrative are now finished events, and design decisions 3-7 are all shipped (custom editor kept, state continuity reworked, properties panel, IME inset bridge, dual-affordance drawer). Decisions 1-2 (extensibility mechanism, stable VPS) live in project memory and `architecture.md`. Carried forward into the new tasks.md: the size-tag legend, the post-release gate, the operating model, and the build strategy.
# Tasks — Cycle 4: Polish → Stable v1

**Target:** Take the three shipped core features (notes, routines, budget) to a stable,
daily-usable v1. Completion bar is deliberately subjective: **"polish until the app is
comfortable to use daily."**

**Status: v1.0.0 tagged 2026-08-31.** Phases 1–5 are complete and Phase 6 is at its last
item. What remains before the personal-phone handover is the **OTA round-trip** — proving a
released build can pull the next one down and install over itself on both mobile and desktop.

> **Completed work is archived.** Every finished phase task and every resolved friction entry
> up to the v1.0.0 cut lives in
> [`.archive/v1.0.0/tasks-completed.md`](.archive/v1.0.0/tasks-completed.md), verbatim and
> with its original verification notes. This file carries **open work only**. Session history
> is in [`.archive/v1.0.0/project-history.md`](.archive/v1.0.0/project-history.md).

**✅ v1 close-out gates — ALL CLEARED.** Sync-integrity fix; the final DB reset + re-import
(box holds ~12.3k events, reconcile byte-faithful to `ledger bal`); user dogfooding
confirmation on both devices; and the full pre-v1 code review (closed 2026-08-28 — four
perspective documents over Cycles 3+4, 13 Criticals, all triaged and fixed). The review was
the un-rush gate per [[feedback-rushing-caused-bugs-review-gate]].

**🗺️ Release roadmap — steps 1–8 done; step 9 is the last.** The agreed ordered march to v1
(user, 2026-08-24) ran: feature-completion → the review gate → email ingest (**cut from v1**)
→ data catch-up → box wipe + clean re-import → Phase 6. Steps 1–8 and their findings are in
the archive. Step 9 is below.

  **F. Phase 6 full release — IN PROGRESS.**
  9. Phase 6 polish (6.2 branch-gate ✅, 6.3 v1 semver + tag ✅, 6.4 archive the bloated docs ✅)
     → ship an **updatable app on mobile + desktop** → make a **trivial change and test the
     update path end-to-end**; fix it if the OTA path is broken.
     **Final go-live wipe = TOTAL, no retention (user, 2026-08-30) — EXECUTED 2026-08-31.**
     Everything stayed disposable until the OTA path was confirmed on the test phone; with that
     cleared, the box was wiped to a genuine fresh slate and every accumulated backup deleted
     rather than kept.
     **What ran, in order, and how each step was verified by state rather than by an exit line:**
     (1) **Dry-run first, before anything was destroyed** — `headless_import` into a throwaway DB,
     then `reconcile.py` against the `ledger bal` oracle: 111 rows, **0 diffs**. That gate is what
     made the deletions safe, since the only thing standing behind them is the reproducibility of
     the import from `main.ledger` + the vault.
     (2) **Box wipe** with `OMNI_DEPLOY_DIR=/home/deploy/omni-deploy` (root has no `~/omni-deploy`,
     so the script's default resolves nothing) — `/data` **21M → 64K**, with the image's skeleton
     dotfiles reappearing, the documented signature of a real emptying. All **4** snapshots then
     deleted (the 3 old ones plus the safety snapshot the wipe itself took, held only until the
     wipe was verified); `/var/omni-snapshots` is empty. `/var/omni-updates`,
     `/etc/omni-me/credentials.toml` and `/opt/omni-ws` kept — data went, config didn't.
     (3) **Local reset** — live `local.db` / `budget.journal` / `workspace.json` plus all 10 backup
     items deleted: **213M → 776K**. `device_id`, `server_url`, `base_currency`, `timezone` kept.
     (4) **Fresh import** under the real `OMNI_DEVICE_ID`, reproducing the 2026-08-30 corpus
     exactly: 10582 txns / 0 parse errors / 0 balance failures, 1352 journal + 343 generic + 4
     collisions + 19 scan errors. Reconcile **0 diffs** again on the real DB; `probe_realdb` all
     anchors OK, net worth **83 290.52 CAD** over 17 accounts, the same 2 known drops, 0 partials.
     (5) **Seed** — `push_local` sent **12 277**; an independent `/sync/pull` probe reads **12 278**
     on the box under a single device id. The extra is genuine: the Wise poller proposed a real
     draft seconds after the container restarted. No test-phone events, no stale poller events.
     The brokerage source correctly reports `needs_reauth` — `ws-session.json` lived on the volume,
     so it needs an in-app **Reconnect** with the OTP once the phone is up.
     **Remaining:** install on `surface` (Linux desktop) and the personal phone — both user-run,
     assistant-guided — then the brokerage Reconnect, then box auth.

  **POST-RELEASE (explicitly gated — only after the core app is solid on journal + routines + finances):**
  - LLM Chat (the main way to get data/insights out).
  - Task + project tracking.
  - Overall inbox monitoring + personal-assistant capabilities.
  - *Do NOT add any new top-level category until the existing three are working end-to-end.*

**Operating model — dogfooding is the test harness.** The user will use the app heavily;
real daily friction is the primary bug-finder. The plan front-loaded "make it livable enough
to live in" (Phase 1), and daily use now feeds the **Running friction log** below, which is
triaged into whichever phase is live. Scope creep is expected and has a home here.

**Strategy:** Sequential. No parallel worktrees (`feedback_parallel_agents_cost.md`).
Subagent default model = `opus` (`feedback_subagent_default_model.md`). Throttle workspace
cargo with `CARGO_BUILD_JOBS=2` (`feedback_throttle_workspace_cargo.md`).

## Design decisions (settled Session 4, 2026-06-06)

1. **Extensibility = one mechanism, two shapes.** (a) Subprocess plugins for data sources
   (generalizes the private overlay's user-provided-driver pattern). (b) Config-selection for
   provider-swaps — bring-your-own-LLM first (one OpenAI-compatible `LlmClient` impl + config:
   base URL/model/key; covers local Ollama/llama.cpp + commercial APIs); `DocumentExtractor`
   on the same rail. Behavior/automation extension deferred (mechanism won't preclude it).
   **Add-mechanism:** scripts placed on the server manually (kept low-friction), all wiring
   in-app; config is in-app data, scripts are *not* uploaded-and-executed (no RCE surface).
2. **Stable VPS** is a committed deliverable (prereq for laptop-free sync + extensibility).
3. **Keep the custom CodeMirror editor**; close the daily-use gap rather than pivot to Obsidian.
4. **State continuity = full rework.** Lift per-page state into a root-held store (survives
   navigation) AND persist workspace *position* to disk (survives Android app-kill / restart).
   Content safety comes from hardened auto-save, not hot-exit.
5. **Properties UI = full inline panel** (Obsidian-style), stays on top of the note; typed
   widgets for the small known vocabulary; scroll-up to reach on mobile (acceptable — fields
   auto-fill / reflections are end-of-day).
6. **Keyboard occlusion fix** = extend the existing Android inset bridge with the IME inset.
7. **Navigation = drawer opened by BOTH edge-swipe and a button.** Edge-swipe verified viable
   on Android (`setSystemGestureExclusionRects`); button is the never-misfires fallback. No
   upfront validation lock-in — on-device friction will surface it.

Size tags: [XS] ≤30min · [S] ~1h · [M] ~2-3h · [L] ~4-6h · [USER] user action

---

## Phases 1–4 — COMPLETE

Daily-use foundation · server go-live · the open-core split + extensibility · the real-data
go-live import. All four are finished; their tasks, the step-by-step STATUS narratives, and
the verification notes are in
[`.archive/v1.0.0/tasks-completed.md`](.archive/v1.0.0/tasks-completed.md).

## Phase 5 — Editor feel + properties *(partly dogfooding-driven)* `(logbook)`

- [ ] **5.4** Typing-feel polish — open bucket, populated from the friction log as daily use surfaces it. [—]

## Phase 6 — Release polish

> **Sequencing (user, 2026-07-04):** 6.2 / 6.3 / 6.4 are the closing "bow" — do them
> **last**, only after *every other task tied to the currently-available features* (the
> rest of Phase 6, the carried backlog, and the Android / Linux-desktop / Windows
> build-and-test track) is complete. Don't branch-gate, tag v1, or archive the tracker
> until the actual feature work is finished.
>
> **Path to the gate — don't pre-lock the enhancement list (user, 2026-07-04).** The pre-gate
> polish/enhancement list is deliberately **not committed**: the user won't lock a curated set
> of middle-tier items until dogfooding the *updated* app on real devices — "I'll still demand
> more once the friction hits, and I don't want to be releasing patches immediately after I've
> locked everything in with the release." So the order is: **(1)** finish the obviously-
> incomplete current-feature code (the **edit-a-committed-ledger-transaction** [M] friction item
> below); **(2)** data/private track — wire per-source account maps + the **one final DB reset &
> re-import** (real data on-device; also verifies the drill-down / balance-cache / dedup built
> against synthetic tests); **(3)** cross-platform **build & test** to get the updated app onto
> mobile + desktop — the pivotal event; then **on-device dogfooding drives** which items get done,
> by real daily impact, until complaint-free; **(4)** standard **code review**; **(5)** 6.2/6.3/6.4.
> A "curate by daily impact" pass was done 2026-07-04 (keep: recurring inline-edit, credit-card
> CSV variant, balancing-posting affordance, Daily Flow redesign, FlushFailed indicator; defer:
> FX-spanning reconciliation, seconds-on-routines unless wanted at the reset, FORCE_GENERIC_DIRS,
> event_store Arc parity, tauri-build PR) — but it's a **hypothesis to re-rank against friction**,
> not a locked plan. Firm post-reset deferrals = the "Post-v1 / when-demanded" + Cycle-5 buckets.


## [DONE] On-device confirmation record — batch 2 (S9, 2026-08-14 / 2026-08-23)
*Archived from `tasks.md` lines 606-609.* Positives and per-item confirmations from two on-device passes. Kept because a confirmation is evidence, and the items it confirms are now closed elsewhere in this archive.

### On-device test findings — batch 2 (2026-08-14, finances/UX pass on a Samsung S9 + desktop)
_Positives confirmed: **ledger snappy after first load** (stale-while-revalidate read-cache working), **finances UI reads much nicer** (design system). The below are the issues surfaced._

**On-device confirmation pass 2026-08-23 (user, S9, debug APK w/ `OMNI_DEFAULT_SERVER_URL`=box):** ✅ date entry (calendar popover) · ✅ nav back (Overview→Institution→Back→Overview) · ✅ Ask/Afford cards gone · ✅ short month labels · ✅ routines 7-day grid readable (user accepts as interim; frequency-aware redesign still open). 🟡 top-bar auto-hide worked but jittered → **mitigated, user-accepted** (goes away once keyboard is up; ↓ #top-bar). 🟡 trend tooltip taps but doesn't scrub on touch → **deferred** (↓ #income-spending). 🔴 account entry still broken on fresh device → **root-caused + refix + on-device verified** (↓ #account-entry). Remaining open from this batch: #off-switch, #desktop-cold-open, #android-back, #trend-touch-scrub. (#recurring-drilldown DONE + Dashboard-extended + overlap-fixed + **user-confirmed on-device 2026-08-23**.)
