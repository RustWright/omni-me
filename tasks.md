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
     **Final go-live wipe = TOTAL, no retention (user, 2026-08-30).** Everything stays
     disposable and resettable until the app is installed and in active daily use on the
     user's **personal** phone — which comes *after* the OTA/update path is confirmed on the
     test phone. At that handover, wipe the box to a genuine fresh slate and **delete the
     accumulated backups rather than keeping them**: `/var/omni-snapshots/*` on the box (incl.
     the three 87-byte no-op tarballs and the stray empty `omni_data` volume) and the
     `~/.local/share/com.omni-me.app/{local.db,budget.journal}.PREWIPE-*` / `.bak-*` copies
     plus the `_backup_pre_*` dirs. The old data is not useful and is only noise.

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
- [x] **6.3** v1 semver stamp + git tag — **DONE 2026-08-31.** `1.0.0` stamped in
  `Cargo.toml` (`[workspace.package]`), `tauri-app/frontend/Cargo.toml`, and
  `tauri-app/src-tauri/tauri.conf.json`; both lockfiles refreshed; CI green on the stamp
  commit; annotated tag `v1.0.0` pushed and now immutable under the `release-tags` ruleset. [XS]
- [x] **6.4** Archive + reset the trackers — **DONE 2026-08-31, cut at the v1.0.0 tag.**
  `project.md` 502 → 73 lines (current state + pointers only); `tasks.md` 1508 → ~250 (open
  work only). History moved to `.archive/v1.0.0/`: `project-history.md` (the full session log
  and Cycle 1–3 records) and `tasks-completed.md` (the whole pre-cut `tasks.md` verbatim —
  archiving only the finished half would have made a worse record). The split was scripted
  with an assertion that every line was accounted for and that the 79 top-level done blocks +
  36 open blocks matched an independent `grep` census; the assertion caught a real
  discrepancy first pass (tasks 1.8a/1.8b are *indented* sub-items carried by their parent,
  so 81 `[x]` lines are only 79 blocks). All 36 open items survived into the live file. [S]

---

## Running friction log *(fill during dogfooding; triage into the live phase)*

### 2026-08-31 — found by the OTA round-trip (step 9)

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
  only the gitignored Android tree changes; confirmed zero tracked churn. [BUG, high]

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
  no way to re-test against an older build. [BUG, high — code fixed, device-verify pending]

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
  [BUG, high — code fixed, device-verify pending]

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
  `http://omni-box-hetzner:3000`** on a device with no persisted value, proving the compile-time
  default injected by private CI reaches a genuinely fresh install.
  **Android update-CHECK path proven too:** Settings → App Updates → *Check for updates* against
  a box serving 1.0.0 returns **"You're on the latest version (1.0.0)"** — so manifest fetch,
  parse, and the `is_newer` comparison all work over the tailnet. The download+install half
  awaits 1.0.1. **Data note:** the phone held 30 own events vs 28 on the box; the 2 unsynced
  were journal edits / batch dismissals and the user chose to accept the loss (test-phone data
  is disposable by standing decision).

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
  `App initialized server_url=http://omni-box-hetzner:3000` — which incidentally also confirms
  the CI-baked `OMNI_DEFAULT_SERVER_URL` reaches a fresh install. The user's real
  `~/.local/share/com.omni-me.app` was confirmed untouched by mtime before and after. [BUG, high]
- [ ] **Desktop DOES flash white for ~320ms — but `backgroundColor` is NOT the culprit and the
  fix is a different layer.** Filmed at last (user installed `Xvfb` 2026-08-31; `grim` fails
  because Mutter lacks `wlr-screencopy`, and GNOME's `org.gnome.Shell.Screenshot` DBus method
  returns `AccessDenied` — portal-callers only). Recorded a cold boot on a virtual display and
  measured the window area frame-by-frame at 25fps:
  **window appears already charcoal → pure white `#fffcff` from t=2.84s to t=3.12s (~320ms) →
  charcoal + the enso splash renders correctly.**
  **So `app.windows[0].backgroundColor` WORKS** — the native window paints `#1e1e1e` from the
  first frame, which is exactly what it was added for. The white is the **WebView**, a separate
  surface: in `tauri-runtime-wry-2.10.1/src/lib.rs:920` the config colour is applied to the
  *tao window* builder only, while the webview's own `set_background_color` (`:3747`) defaults
  to **`(255,255,255,255)`** when unset. The config doc-comment claims "window and webview", but
  on GTK only the window layer is wired from config at build time. **This is the identical bug
  Android had**, fixed there natively in `MainActivity.onWebViewCreate` — two surfaces, and only
  one was covered.
  **Likely fix:** call `set_background_color` on the `WebviewWindow` in `lib.rs` setup (desktop
  cfg), so the webview surface matches before first paint. Would make `#1e1e1e` a FIFTH place
  that must stay in sync — better to derive all of them from one constant while touching this.
  **Caveat before building:** this was filmed under Xvfb **software** rendering (`libEGL
  warning: DRI3 error` in the log), so confirm the flash on the real display first — the
  compositing path may differ on a GPU. **Deferred past v1 per the settled splash scope**; the
  Android flash (the one that actually bit) is fixed. [S, deferred]
  **Trap worth keeping — `XDG_DATA_HOME` MUST be absolute.** The capture script set it from
  `SP="$(dirname "$0")"` while being invoked as `./film-splash.sh`, so `$SP` was `.` and the
  variable held a **relative** path. The XDG spec says relative values are invalid and must be
  ignored, so the app silently fell back to the real `~/.local/share/com.omni-me.app` — the run
  was NOT isolated, despite the script reading as though it were. Nothing was damaged (it read
  the DB, cleared a stale `LOCK`, touched `workspace.json`, authored no events) and it made the
  film a *warm* boot against real data rather than a fresh one, which is arguably the more
  representative test. But the failure was **silent**: no warning, no error, and the isolation
  claim looked true. Tells that caught it: a "stale SurrealKV LOCK" in a supposedly fresh dir,
  and a `device_id` older than one from an earlier run. Always pass an absolute path, and
  verify isolation by the dir's mtime rather than by reading the script.
- [ ] **The pipeline cannot tell a launchable release from an unlaunchable one.** The bug above
  survived two months because "published + correct sha + valid signature" was the whole
  acceptance test. Worth a headless smoke step in `app-release.yml` that runs the built
  AppImage under `xvfb-run` and fails the job if it exits non-zero within a few seconds. Would
  have caught this exact class on the first release. (Android can't be smoke-run in CI the same
  way — an emulator boot is a much bigger lift — so the APK stays device-verified.) [S, CI]

### 2026-08-30 — noticed during the on-device splash verification

- [x] **`unused_mut` warning on every Android build — DONE 2026-08-31.**
  `tauri-app/src-tauri/src/lib.rs` (`let mut builder = tauri::Builder::default()`) only needs
  `mut` on desktop, where the `#[cfg(desktop)]` updater-plugin block reassigns it; on Android
  that block is compiled out, so every APK build printed the warning. Fixed with
  `#[cfg_attr(not(desktop), allow(unused_mut))]` on the binding. The Android cfg can't be
  exercised by a desktop clippy run, so the attribute shape was proven separately with a
  standalone `rustc` probe under `#![deny(unused_mut)]` (a `cfg_attr`-gated `allow` on a `let`
  statement does apply). Desktop clippy `-D warnings` clean, 73 app tests green. [XS]
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

### 2026-08-31 — first dogfooding on the clean seed

### 2026-08-29 — found while cutting email from v1

### 2026-07-04 — cross-platform dogfooding (phone + laptop, both on the Hetzner box) — NEW, list incomplete ("...and it goes on")

**CRITICAL — sync / data integrity:**
- [ ] **Auto-sync never fires.** Edits save locally but don't propagate until the **Sync** button is pressed manually, despite both devices connected to the Hetzner server. Applies to journal, notes, tasks — everything. (dogfooding 2026-07-04) [?]
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
- [ ] The objection is **open-ended gate config**, not automation and not email as a source. Any replacement has to make "which mail is relevant" self-maintaining. Directions worth *only* a note until v1 ships — do not design these now: forward-to-a-dedicated-address push instead of polling (the user's filter action becomes the signal, no label to maintain); Gmail API query search instead of a maintained label; or drop email entirely and add API sources. Whatever replaces it inherits the constraint at `receipts.rs:216-231` — untrusted sender input reaching an LLM, with the pending-review queue as the only control. [→ own planning session, [[feedback-defer-major-phases-to-fresh-session]]]

**FEATURE — journal line timestamps, redesign (design-first → own session, user 2026-07-05):**
- [ ] **On-device sync feels laggy + the open view doesn't always live-refresh** (on-device test, 2026-08-14). Auto-sync **works both directions without the manual button** — validated live: a fresh phone backfilled all **12,643** events (device audit `total=12643, ever_synced=true`), and edits propagate desktop↔phone on their own. Two UX gaps surfaced: **(1) latency** — inbound edits take up to ~20s: the server has no push channel so receivers POLL (`core/src/sync/puller.rs:27` `DEFAULT_PULL_INTERVAL=20s` + 4s warmup; a network-online accelerator nudge exists but steady-state is the 20s poll). Tunable (shorter interval = more requests) or add a push/SSE channel (bigger build). **(2) live-refresh gap** — after an auto-pull applies, the backend emits `sync:applied` (`tauri-app/src-tauri/src/lib.rs:389-402`, only when `pulled>0`) → frontend bumps `sync_epoch` → subscribed views (journal/notes/routines/finances all read it) refetch; BUT the currently-open view **sometimes** stays stale until you navigate away+back (forcing a remount refetch). Suspects: the `sync:applied` nudge not reliably reaching the specific open component, or the editor **dirty-protect** (from `aa41789`, prevents live-clobber-while-typing) over-suppressing the body refresh even when not actually dirty. Needs frontend diagnosis. **Both are polish, NOT blockers — core sync + the 306/308 fixes are validated on-device.** [M, frontend]
### On-device test findings — batch 2 (2026-08-14, finances/UX pass on a Samsung S9 + desktop)
_Positives confirmed: **ledger snappy after first load** (stale-while-revalidate read-cache working), **finances UI reads much nicer** (design system). The below are the issues surfaced._

**On-device confirmation pass 2026-08-23 (user, S9, debug APK w/ `OMNI_DEFAULT_SERVER_URL`=box):** ✅ date entry (calendar popover) · ✅ nav back (Overview→Institution→Back→Overview) · ✅ Ask/Afford cards gone · ✅ short month labels · ✅ routines 7-day grid readable (user accepts as interim; frequency-aware redesign still open). 🟡 top-bar auto-hide worked but jittered → **mitigated, user-accepted** (goes away once keyboard is up; ↓ #top-bar). 🟡 trend tooltip taps but doesn't scrub on touch → **deferred** (↓ #income-spending). 🔴 account entry still broken on fresh device → **root-caused + refix + on-device verified** (↓ #account-entry). Remaining open from this batch: #off-switch, #desktop-cold-open, #android-back, #trend-touch-scrub. (#recurring-drilldown DONE + Dashboard-extended + overlap-fixed + **user-confirmed on-device 2026-08-23**.)
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
### 2026-07-06 — journaling personalization

- [ ] **Journal template hardcodes the user's personal journaling framework** — `frontend/src/journal_template.rs::render` bakes in the user's own choices: the three reflection property keys (`homework_for_life`, `grateful_for`, `learnt_today`), the `## What happened today?` section heading, and the `daily_note` tag. Those same three keys are also hardcoded in the day-complete `is_complete` check (`core/src/events/notes_projection.rs`), and the `tags: [daily_note]` inline-list form is itself a workaround for that parser — so the template, the auto-close logic, and the typed properties panel (`journal.rs::JournalPropertiesPanel`, 3 fixed reflection fields) are all coupled to this one personal schema. Generalizing (user-configurable reflection prompts + template) means reworking `is_complete` to not key off fixed names + adding a config surface for the prompt set. Also a mild personalization-in-open-core smell (personal journaling prompts sit in the public repo, though not identity/financial data). [M] — flagged by user 2026-07-06, **deferred ("resolve later")**; pairs with 5.4 typing-feel + the properties-panel work.

### 2026-07-21 — finances perf + overall UI/UX coherence (HEADLINE — own session)

- [ ] **Finances section feels slow to load / unresponsive, and the overall UI/UX lacks coherence (mobile + desktop).** User (2026-07-21): "better ways to present the data and expose interfaces for me as a user to interact with it." Two intertwined threads: **(1) perf** — the finances views feel laggy on load (seed already noted: balance-cache landed 2026-07-04, but load/interaction responsiveness in the finances section specifically still feels slow — profile the real path: command latency, projection reads, frontend render/hydration, mobile vs desktop); **(2) UX/IA redesign** — the app doesn't feel like one coherent system; rethink how finance data is presented and how the user interacts with it, on both form factors. **Cross-cutting → its own planning-first session** per the defer-major-phases rule, opened with **rendered design candidates** (per the design-render-candidates habit; design for full future scope, go wide before narrowing). Do NOT start as a tail-of-session. [L, → own session] — **IN PROGRESS — Stages A/B/C landed 2026-08-10** (plan `could-you-start-reviewing-curious-dahl.md`; approved IA = **Overview · Ledger · Analyze**). **A (perf):** read-path `tracing` instrumentation (`972cdfb`); measured real data (10,209 txns) → naive indexes insufficient (SurrealDB 3.0.4 won't skip the ORDER BY sort), so the win is frontend caching, done in C3. **B (design foundation):** CSS-var token layer + shared primitives (`Card`/`Button`/`PageHeader`/`Banner`/`StatTile`/`SegmentedNav`/`TextInput`/`Icon`) (`39a6021`); user picked Overview look **C · Balanced**. **C (IA build, 6 commits `dcd37d0`→`e87a5fb`):** C1 real net-worth-history backend (`core::dashboard::net_worth_series`, endpoint == hero; 3 core tests); C2 persistent sub-nav replacing the flat 18-variant hub (all flows preserved, surface persisted in `NavState`); C3 stale-while-revalidate frontend read-cache + skeletons (the top felt-latency lever); C4 the C·Balanced Overview (net-worth hero + range-switchable SVG area chart 1M/3M/6M/1Y/YTD/All + 2×2 card grid); C5 Ledger master-detail (desktop side-by-side / mobile slide-over, row highlight); C6 Analyze landing (cash-flow trend + budgets snapshot + reserved LLM entry). Review gate: core tests + both wasm clippy configs green, Playwright-verified 390+1280 with 0 console errors, inline-edit mutation confirmed. **REMAINING:** ~~Stage D~~ **DONE 2026-08-24** (full primitive refactor across all 5 pages + input-class fold; see #594 in the roadmap above for commit list). Still: on-device/real-data end-to-end pass (mock can't exercise the backend or real perf; rides the queued DB reset). [L, → own session]

---

## Carried backlog (slot into a phase or pull from the friction log)

**Post-launch fix cycle (from Phase 4 GUI validation, 2026-06-22):**
- [ ] Wire the private overlay's **per-source account maps** so the account-map-based bank pollers actually emit drafts (they import 0 until wired — the private half of 3.9; receipt/email sources already work). **Deferred to polish (user, 2026-06-28).** [M]

**Phase-5 reconciliation/import deferrals (from Cycle 3):**
- [ ] Inline-edit per detected recurring pattern before confirm (today: dismiss + rescan). [S]
- [ ] Balancing-posting affordance for hidden-fee resolution on merge (wire/FX fees). [S]
- [ ] Credit-card CSV variant + real-export format verification (synthetic-tested only). [S]
- [ ] Reconciliation candidate engine: FX-spanning (cross-currency) matches. [M]

**Deferred stretch (from Cycle 2/3):**
- [ ] Daily Flow consistency visualizer redesign — frequency-aware (was 7-day hard-coded). [M]
- [ ] `BufferEvent::FlushFailed` → `StatusReporter` "stuck buffer" indicator. [S]
- [ ] Configurable `FORCE_GENERIC_DIRS` (hardcoded to `Work/`). [S]
- [ ] `auto_close_scheduler::AppState.event_store` → `Arc<dyn EventStore>` parity. [XS]
- [ ] Seconds duration unit on routine items (breaking event-schema change, 16 touch points). [M]
- [ ] `cargo:rerun-if-env-changed=TAURI_DEV_HOST` upstream contribution to `tauri-build`. [XS]

**Deferred from Cycle 5**
- [ ] Create mdbooks docs, copy the pattern from ../mylearnbase in how it is created and deployed
- [ ] There will be a lot of features added to omni-me, not everyone might want to use every feature, add ability to toggle them off so there is no trace of them
- [ ] The next major thing would be adding chat functionality, so I can chat with an LLM and it can execute commands to do things instead of me needing to go find a way to do it myself

**Post-v1 / when-demanded:**
- [ ] PWA fallback (deferred Cycles 1-3).
- [ ] Veryfi `DocumentExtractor` impl (trait + routing scaffold already in place).
- [ ] ExchangeRate-API auto-rates for the manual-FX currency (replaces manual per-statement entry).
- [ ] LLM-translated NL queries for R2 (evaluate; ship only if real usage demands).
- [ ] PaddleOCR sidecar (escape hatch from Cycle-3 7.11).
- [ ] C1 email auto-fetch (vs paste); R3 self-employment dashboards; R4 tax-form validation.
- [ ] Generic IMAP config source — wire the existing public `ImapSource` into the config builder. Indefinitely deferred 2026-06-20 (needs `build_one` to thread `db`+`extractor`+async into both call sites, *and* a handler-policy design call — a config IMAP source = receipt importer by sender-pattern?). Not personally needed: the user's email sources (statements + receipts) run through the private overlay's `build_imap_sources`.
- [ ] SurrealDB bump past 3.0.4 — **lockstep across both repos** (public + private overlay each pin their own lock; out-of-sync re-floats the overlay to 3.1 + `diskann`, which fails to compile on the current toolchain, rust#100013). No vector-search usage today, so no pull; revisit when vector search is wanted or the toolchain resolves #100013. Patch 3.0.x bumps are safe meanwhile. [S]

---

## Cycle 5+ filed

- Inbox management feature (user's "far future dream").
- Open Banking Canada evaluation (when bank adoption matures).
