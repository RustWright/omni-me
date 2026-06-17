# Tasks — Cycle 4: Polish → Stable v1

**Target:** Take the three shipped core features (notes, routines, budget) to a stable,
daily-usable v1. Completion bar is deliberately subjective: **"polish until the app is
comfortable to use daily."**

**Status:** Cycle 4 Session 4 (Planning) complete 2026-06-06. Phase 1 is the first
implementation target (Session 5). Plan file: `~/.claude/plans/vivid-cooking-kurzweil.md`.

**Operating model — dogfooding is the test harness.** The user will use the app heavily;
real daily friction is the primary bug-finder. The plan front-loads "make it livable enough
to live in" (Phase 1), then daily use feeds the **Running friction log** below, which is
triaged into whichever phase is live. Scope creep is expected and has a home here.

**Scope:** open-core public/private split (one-way door) · extensibility mechanism
(subprocess plugins + config-selection) · stable VPS + deploy · real-data go-live import
(Cycle-3 6.5) · editor daily-use foundation + properties panel · release polish (logo,
branch-gate, v1 stamp).

**Strategy:** Sequential. No parallel worktrees (`feedback_parallel_agents_cost.md`).
Subagent default model = `opus` (`feedback_subagent_default_model.md`). Throttle workspace
cargo with `CARGO_BUILD_JOBS=2` (`feedback_throttle_workspace_cargo.md`).

## Design decisions (settled Session 4, 2026-06-06)

1. **Extensibility = one mechanism, two shapes.** (a) Subprocess plugins for data sources
   (generalizes the WealthSimple user-provided-driver pattern). (b) Config-selection for
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

## Phase 1 — Daily-use foundation *(unblocks dogfooding; do first)* `(logbook)`

- [x] **1.1** Root continuity store: `use_context_provider` at app root (`tauri-app/frontend/src/main.rs`, joins existing tz/pending-share contexts), state keyed by identity (note path / capture-id / list-instance). [M]
- [x] **1.2** Migrate journal editor (`pages/journal.rs`) to the store; drop page ownership of `content`/`last_saved_content`/save loop. [M]
- [x] **1.3** Migrate generic notes editor (`pages/notes.rs`) to the store. [M]
- [x] **1.4** Migrate in-flight finances captures (Phase-3 gap) into the store keyed by capture-id; "in-flight capture" affordance on Home. [M]
- [x] **1.5** Migrate transaction-list pagination state (`transactions`/`offset` signals) into the store keyed by list-instance — fixes nav-to-detail-and-back reset (`project.md` carry). [S]
- [x] **1.6** ~~Relocate the auto-save loop into the store~~ — **resolved via lean path (B), 2026-06-06.** Post-1.2–1.5 the store already retains unsaved content and re-fires the save on remount (covers nav-away-and-back); no root save daemon (avoids the id-writeback coordination). The `edit → leave → never-return → app-kill` gap folds into the extended 1.8 (persist the store to disk). Decision rationale in memory `project-autosave-robustness-b`. [M]
- [x] **1.7** Auto-save resilience: retry/backoff on failure + a glanceable save-state indicator (saved / saving / unsaved / failed). [S] — shared `autosave.rs` (`SaveState` + `save_with_retry` + `SaveIndicator`); backoff policy = exp 500/1000/2000/4000ms × 4 then fail; wired into journal + notes auto-save & manual save.
- [x] **1.8** Workspace persistence (Level 2-A) — **split a/b:**
  - [x] **1.8a** Store persistence (closes the 1.6 kill-gap): serde on store value types + `PersistedWorkspace` (maps as `Vec<(key,val)>` — serde_json rejects enum map keys); backend `get_workspace`/`save_workspace` (JSON file in `app_data_dir`, mirrors `commands/settings.rs`) + bridge fns + mock stubs; boot-load (writer-gated on `loaded`) + debounced write-back in `use_continuity_provider`. Frontend clippy clean + backend `omni-me-app` check clean. **Not yet run against real disk** (mock has no backend — verify in next full-app/on-device run).
  - [x] **1.8b** Position restoration — **done 2026-06-06, two slices.** *Slice 1 (cursor/scroll):* `EditSession.cursor` (`#[serde(default)]`) + CodeMirror interop (`editor.js`: `onCursor`/`initialCursor` in options, selection-aware update listener, `clampCursor` = `Math.min(pos,len)` so a shrunk doc drops the caret at end, `getEditorCursor` unmount fallback) + `js_get_editor_cursor` extern + `Editor` `initial_cursor`/`on_cursor` props; journal + notes capture-via-`on_cursor`/hydrate/mirror; restore is selection-only so it never flips dirty. *Slice 2 (nav):* `NavState` (string-typed, dependency-free) in the store + `nav_peek`/`update_nav`/`is_loaded`; `Tab::as_key`/`from_key`; boot tab-restore future (gated on `loaded`, pending-share still wins); journal date+subtab and notes view(Edit/List)+subtab restore-on-load/mirror-back (New→List on restore — draft content preserved by slice 1). *Boot race:* page hydration now waits on `store.loaded_peek()` so the initially-open page sees the disk snapshot. **Playwright-verified (mock, in-memory tier):** journal caret 3 + notes caret 21 restored across remount w/o dirtying; notes open-note + journal day (June 12) restored across tab round-trip; clean reload → Journal/Today defaults; 0 console errors. **Disk tier (boot-after-kill restore of caret/nav) shares these code paths but needs a real backend — same on-device bucket as 1.8a.** Frontend clippy clean (mock + default). [M]
- [x] **1.9** Keyboard occlusion — **impl 2026-06-06; on-device bug found + fixed 2026-06-07.** `InsetBridge.kt`: read `WindowInsetsCompat.Type.ime()` inside the *existing* content-root listener (chained), cache `ime.bottom` in CSS px, inject `--keyboard-inset-bottom`. `:root` default `0px` in `input.css`. **On-device (Android 10 / API 29):** `--keyboard-inset-bottom` stayed `0px` with keyboard up (systemBars inset worked = bridge fires; `Type.ime()` specifically read 0). Root cause (confirmed via Android docs): `Type.ime()` is a first-class inset only since API 30; on API 29 the androidx backport needs the window in `adjustResize`. **Fix:** added `android:windowSoftInputMode="adjustResize"` to the activity in `android-overrides/AndroidManifest.xml` (pairs with the existing `enableEdgeToEdge()` = `setDecorFitsSystemWindows(false)`). Re-verifying on device. [M]
- [x] **1.10** Web caret-above-keyboard — **done 2026-06-06 (impl; on-device verify rides 1.13).** `.pb-mobile-nav` padding adds `var(--keyboard-inset-bottom)` so the scroll column gains room when the IME is up. `editor.js`: `visualViewport` resize/scroll listener + `keepCaretAboveKeyboard()` (rAF-coalesced) compares `coordsAtPos(head)` against `visualViewport.height` and nudges the nearest `overflow-y:auto` ancestor (`findScrollParent`, gate-free so it targets the main column reliably despite the padding/event race; `body` is `overflow:hidden`); also called on doc/selection changes. **Browser-checked:** var resolves `0px`, `visualViewport` present, scroll-parent lookup resolves to the `pb-mobile-nav` column, 0 console errors, editor functional. Real occlusion needs a device. [S]
- [x] **1.11** Nav drawer — **done 2026-06-06 (model A: mobile drawer replaces bottom nav, desktop keeps SideNav).** New `NavDrawer` (scrim + slide-in panel, both always-rendered + class-toggled so the transform animates) replaces `BottomNav` (component removed); header gains an `md:hidden` hamburger (`mr-auto` keeps the sync chip right; `justify-end` keeps it right at md+). `.pb-mobile-nav` dropped its 4rem bottom-bar reserve → `1.5rem + safe-area + keyboard`. **Playwright-verified (390px):** hamburger opens drawer, scrim dims content, destination tap navigates + auto-closes, scrim tap closes; (1280px) persistent SideNav, no hamburger. Screenshots in `logbook/_assets/phase1-nav-drawer/`. Clippy clean. [M]
- [x] **1.12** Edge-swipe open — **done 2026-06-06 (web verified; native rides 1.13).** *Web:* root-shell `ontouchstart`/`move`/`end` track a touch that *starts* in the left `EDGE_SWIPE_START_PX`(24) strip while the drawer is closed and opens it once it travels `EDGE_SWIPE_OPEN_PX`(48) right; no `preventDefault`, so scroll/typing untouched. **Synthetic-touch-verified:** edge swipe (x8→x80) opens; mid-screen swipe (x200→x280) is a no-op; 0 console errors. *Native:* `InsetBridge.applyGestureExclusion` sets `systemGestureExclusionRects` on the content root for a 24dp left strip (API 29+, re-applied on the boot schedule since `root.height` is 0 early; OS clamps the height, so best-effort — hamburger stays the guaranteed opener). Not compiled (no Android build this session). [M]
- [x] **1.13** On-device APK verification (Samsung, Android 10 / API 29) — **COMPLETE 2026-06-08:** 1.9 inset + 1.10 caret-above-keyboard both verified on-device (caret clears the keyboard by the exact 24px margin). The real blocker — found after a long debug — was the build pipeline embedding a **stale debug frontend** (`frontendDist`=debug baked into the `.so`), so no JS change reached the device; fixed via `scripts/android-build.sh release` (see friction log + `BUILD.md`). 1.10 native-event fix: `InsetBridge` dispatches `omni:keyboardinset` → `editor.js` re-runs `keepCaretAboveKeyboard`. Pre-fix investigation notes:** Build pipeline issues found + fixed first: (1) release build OOM-froze the 7GB laptop → memory-safe env overrides (`CARGO_PROFILE_RELEASE_LTO=false`/`CODEGEN_UNITS=16`/`OPT_LEVEL=1`, `CARGO_BUILD_JOBS=1`) + a MemAvailable watchdog; (2) APK shipped **mock** data because `frontendDist`=debug dir held a stale `dx serve --features mock` build (latent: `beforeBuildCommand` builds release but `frontendDist`=debug → android always bundles the debug dir) — rebuilt debug dir non-mock. APK signed with debug keystore, sideloaded. **Verified on real backend:** ✅ tab restore across app-kill (reopened on Finances — NavState lives only in the workspace file, not the DB); ✅ content persistence across kill (marker survived); ✅ caret restore across kill (`getEditorCursor`=156, non-zero, at edit point); ✅ drawer open + navigate; ✅ InsetBridge injects real `--safe-area-inset-bottom` (48px). **Found + fixing:** ❌ keyboard inset (1.9) — `adjustResize` fix building. **Still to check:** caret-above-keyboard (1.10) once 1.9 lands; edge-swipe is a no-op to test here (device is 3-button nav → no back-gesture conflict). [S]

## Phase 2 — Server go-live *(deploy — runs AFTER the Phase 3 split; kept lean until then)*

**Execution order (revised 2026-06-13):** **Phase 3 (split) runs before Phase 2 (deploy)** —
not parallel as originally framed. The deploy pipeline's *home and shape* depend on the split's
repo topology + the Model-A-vs-B image decision, and the current repo is already **public**, so
a personal deploy pipeline can't live there. Detailed deploy design is deferred until the split
settles, to avoid designing against a topology that will change (`feedback-sequence-by-dependency`).

**Decisions that survive the reorder** (`project-deploy-runtime`, `feedback-ci-cd-over-sysadmin`):
Docker container (not systemd); dual-provider (Hetzner + DO 60-day trial); one provider-neutral
image; Tailscale reachability (phone reaches the server by tailnet hostname); heavy CI/CD with
health-gated deploy + auto-rollback, SHA-tagged images, pre-deploy snapshots, one box at a time,
nothing exposed publicly. **The "deploy to my box" pipeline is homed on the private side** (private
overlay under Model A); only "build/test/publish the public image" stays public.

- [ ] **2.1** Containerize *runtime* config — split-agnostic, safe to prep anytime: `docker-compose` volume layout (surreal_data + blobs → one volume via `BLOB_DIR`), `/health` healthcheck, runtime secret mount (`credentials.toml` + `GEMINI_API_KEY`), `restart: unless-stopped`. [S]
- [ ] **2.2** Provision Hetzner + DO boxes (Ubuntu LTS, ~2GB RAM — CI builds the image, box only runs it) + Docker + Tailscale bootstrap. [USER] — **defer DO until deploy is ready so the 60-day trial clock doesn't burn idle; no box is needed for the split itself.**
- [ ] **2.3** Deploy pipeline — **DETAIL AFTER THE SPLIT** (homed per topology / Model A-vs-B): build the real image → push to the right registry → deploy over the tailnet → health-gate + auto-rollback → pre-deploy snapshot → dual-target rollout (DO then Hetzner). [—]
- [ ] **2.4** Verify go-live: **phone syncs against the VPS with the laptop OFF**; kill one box, the phone still syncs via the other. [S]
- [ ] **2.5** **Precondition MET 2026-06-17:** app-entered re-auth (3.5a / `SOURCE_REAUTH_DESIGN.md`) has shipped + is real-OTP-verified, so WS auto-import may run on the VPS — the SSH-for-OTP failure mode is gone. The remaining work here is just the deploy itself (Phase 2); WS stays local-only only until the deploy pipeline lands. [—]

## Phase 3 — Open-core split + extensibility *(the one-way door — RUNS BEFORE Phase 2)* `(logbook)` `(demo?)`

**First decision when opening this phase:** Model A (bank adapters compiled into a private image;
deploy pipeline lives in the private overlay repo) vs Model B (public zero-source image everywhere;
bank adapters run as box-local subprocess plugins per 3.5 — deploy stays mostly public). Lean A now
(least work, matches 3.1–3.3), B as the end-state once the subprocess mechanism matures. This choice
sets the Phase 2 deploy topology.

**STATUS 2026-06-14 — STEP 1 (relocate) COMPLETE (3.1–3.3).** Destination resolved = Model B via a
foundation-first path. Private overlay `RustWright/omni-me-private` created + pushed (`main`@`8b07e83`)
+ registered as a `productive_learning` submodule; public engine bank-free + 446 tests/clippy clean;
private clippy clean + 23/23 adapter tests + green smoke test vs real config (graceful WS degradation
verified live). SurrealDB pinned 3.0.4 in both repos (lockstep — see backlog).

**STATUS 2026-06-15 — STEP 2a (subprocess contract + WS pull-helper) COMPLETE.** Froze the engine↔helper
JSON contract in prose (`SUBPROCESS_SOURCE_CONTRACT.md`) + code (`HelperRequest`/`HelperResponse`/
`HelperStatus` public serde types in `core::auto_import::subprocess`). Built the generic public
`SubprocessSource` (the WS adapter's generic tail, generalized; 6 fake-helper tests). Converted WS to a
standalone `ws-helper` binary in the overlay (`fetch_drafts` + `src/bin/ws-helper.rs`) that **reads its
own credentials** — the engine never sees a bank secret (boundary now structural). Helper discovery =
sibling-of-current-exe + `OMNI_WS_HELPER` override (the convention all future plugins reuse). Hardened
the `driver_script` path (engine validates the helper command; helper resolves driver to absolute +
existence-checks). Public 6 new tests + clippy clean (still bank-free); private builds both binaries +
22 tests + clippy clean. **Smoke-verified live:** WS now ticks via `SubprocessSource → ws-helper →
python driver`; real session-expiry → driver exit 5 → `needs_reauth` → graceful backoff, Wise + 3 IMAP
unaffected. **STEP 2b = next session:** app-OTP re-auth full stack (3.5a — `AuthState` + `/reauth` route
+ WS helper `reauth` verb (already frozen in the contract) + Dioxus "Reconnect" UI). Deferred: Wise
helper; SC email-handler wrinkle (out-of-scope per the contract — stays in-process; recommended future
shape = helper decrypts PDF, engine extracts via LLM); real account-map (3.9).

**STATUS 2026-06-15 — STEP 2b (app-OTP re-auth backend slice, 3.5a server half) COMPLETE.** The
`needs_reauth` signal is now a tracked, exposed `AuthState` instead of a buried log line: new
`ImportError::NeedsReauth` + `AuthState`/`ReauthOutcome` public types + defaulted
`reauth`/`reauth_capable` trait methods + registry state-tracking (`record_tick` flips to
`NeedsReauth`; a clean tick or successful reauth clears it; a transient blip leaves it) + registry
`reauth()`. `SubprocessSource` threads the new variant and speaks the frozen `reauth` verb;
`ws-helper` runs `wealthsimple::reauth` (driver fresh-login: exit 0→`reauth_ok`, 4→`invalid_otp`,
else→`error`); new `POST /auto_import/reauth` (OTP in body, not URL); `GET /auto_import/status` now
carries `auth_state` + `reauth_capable`. Public +14 tests (62 auto-import) + clippy clean both feature
configs + server clippy clean; private +4 tests (26) + clippy clean + both bins rebuilt. **Live
round-trip vs real config (GREEN):** status exposes the fields (`reauth_capable` true only for the
SubprocessSource, false for wise + 3 IMAP); a dummy-OTP reauth drove the *real* WS driver →
`invalid_otp` (the driver already supports the otp fresh-login path); unknown source→404; non-capable
`wise`→`not_supported`; the WS scheduler tick surfaced `needs re-auth` and backed off while the other
4 sources ticked clean. **Caught + fixed mid-verify:** the first round-trip ran a stale binary
(`cargo test` rebuilt the lib but didn't relink the bin) → explicit `cargo build`, re-verified.
**Deferred to next session (the rest of 3.5a):** Dioxus "Reconnect {source}" UI + OTP field +
Playwright + the real-OTP happy-path test. Contract docs + `SOURCE_REAUTH_DESIGN.md` updated.

**STATUS 2026-06-17 — STEP 2c (app-OTP re-auth CLIENT, 3.5a DONE) COMPLETE.** The Dioxus client now
drives the proven backend, finishing 3.5a. **Inline-in-row UX:** the existing Auto-Import Sources
settings row (`AutoImportRow`) grows an amber "Reconnect needed" callout + `Reconnect` button when a
source is `needs_reauth` + `reauth_capable`; the button expands an inline 6-digit OTP field whose
`Submit` calls the new `reauth_source` Tauri command → `POST /auto_import/reauth` (OTP in body). The
`ReauthOutcome` drives the row: `active`→success+collapse+parent re-pulls `/status`→healthy;
`invalid_otp`→"code rejected", field stays; `not_supported`/`error`→inline message. **The seam that
was silently swallowing the signal:** each proxy `AutoImportSourceView` (Tauri command layer +
frontend types) deserialized lossily, dropping the server's `auth_state`/`reauth_capable`; declared
both (`#[serde(default)]`) at every hop so it survives to the screen. Orthogonal to `health` (passive
"data flowing" vs imperative "user must act") — a degraded-but-active SC source shows no callout, only
`needs_reauth` does. Clippy clean (Tauri + frontend, both feature configs). **Playwright mock
walkthrough (5 states, 0 console errors):** WS shows the callout, others don't; Reconnect reveals the
field; `000000`→rejected (field stays); valid 6-digit→success→row returns healthy on refresh. PNGs in
`logbook/_assets/source-reauth-reconnect/`. **Real-OTP happy path proven E2E vs the real WS account**
(`cargo tauri dev` desktop → private server on real `credentials.toml`): a live TOTP flipped
`auth_state → active`, then a manual `Fetch now` came back `last_outcome: success` / `health: healthy`
— proving the **session refreshed**, not just the flag cleared (the two-clocks model: `registry.reauth`
only flips `auth_state`, so the successful pull is independent evidence). **This unblocks 2.5.**

- [x] **3.1** Create private overlay crate written against `core`'s `AutoImportSource`. [M] — **done 2026-06-14**; path-deps on the public crates (pinned git-dep deferred to deploy).
- [x] **3.2** Move bank adapters (`wealthsimple`/`wise`/`sc_ngn` + WS python driver + credential structs) into the overlay; generic plumbing (`imap*.rs`, `receipts.rs`, `mime.rs`, trait) stays public. [L] — **done 2026-06-14**; public copies `git rm`'d after private verified.
- [x] **3.3** Invert source instantiation — done via the `run(RunConfig{source_builder})` seam in `server/src/lib.rs` (not literally `main.rs`); public `main.rs` = zero-sources builder. [M] — **done 2026-06-14**.
- [ ] **3.4** Public app degrades gracefully to zero configured sources + zero declared accounts (no crash; manual entry / journal / budget all work). [M]
- [x] **3.5** Subprocess-plugin runner: generic public `SubprocessSource` (command + args; helper owns creds + account-map, so "secret-ref" became "helper reads its own secrets" — a stronger boundary; schedule stays the engine's interval). WS converted to the `ws-helper` binary; contract frozen in `SUBPROCESS_SOURCE_CONTRACT.md` + code types. [L] — **done 2026-06-15 (Step 2a)**; smoke-verified live. Multi-source config *registration* (declare sources via config/UI) is 3.6/3.7; CSV/REST helpers fan out from this runner.
- [x] **3.5a** Interactive source re-auth (**app-entered OTP**) per `SOURCE_REAUTH_DESIGN.md` — generic `AuthState` + status + reauth route in the **public** engine; WS driver login-protocol in the **private** overlay; client "Reconnect {source}" UI. Removes the SSH-to-VPS-for-OTP failure mode. **Was the hard precondition for deploying WS auto-import to the VPS (Phase 2) — now MET.** [M] `(logbook)` — **DONE 2026-06-17.** Server half (Step 2b, 2026-06-15): engine `AuthState`/`ReauthOutcome` + registry state-tracking + `POST /auto_import/reauth` + `auth_state`/`reauth_capable` on `GET /auto_import/status` + `SubprocessSource` reauth verb + `ws-helper` `reauth` handler (`wealthsimple::reauth`). Client half (Step 2c, 2026-06-17): inline "Reconnect {source}" callout + OTP field in `AutoImportRow` + `reauth_source` Tauri command, the lossy-serde seam widened at every proxy hop. Routes under `/auto_import/*` (not `/sources/*` as the design sketched). **Verified:** clippy clean (both feature configs); Playwright mock walkthrough of all 5 states (0 console errors); **real-OTP happy path E2E vs the real WS account** — live TOTP → `auth_state: active`, manual Fetch → `last_outcome: success`/`health: healthy` (session refreshed, not just flag cleared). `(logbook)` capture deferred to a later drafting pass; PNGs preserved in `logbook/_assets/source-reauth-reconnect/`.
- [ ] **3.6** Config-driven generic sources: CSV first (+ REST / IMAP) parameterized by config. [L]
- [ ] **3.7** In-app source-registration UI (Settings): add / edit / remove sources; secrets referenced by name. [M]
- [ ] **3.8** Provider-swap: OpenAI-compatible `LlmClient` impl + Settings picker (base URL / model / key); `DocumentExtractor` on the same config rail. [M]
- [ ] **3.9** Move account roster (`bridge.rs:1392-1441`) into config/data + declared-accounts Settings UI (lift `core::balances::LISTABLE_ACCOUNTS` → `accounts` SurrealDB table; `accounts` table + `AccountAdded` already exist). [M]
- [ ] **3.10** Liquidity-aware `can_i_afford` (per-account `is_liquid` flag drives the verdict; same accounts table; `AffordVerdict.policy_label` → "Liquid assets − next month's recurring"). [S]
- [ ] **3.11** Synthetic-fixtures discipline: adopt before any parser work against real data. [XS]
- [ ] **3.12** mylearnbase follow-up: re-shoot Accounts screenshot generic + soften "CIBC Aventura" + update that image's alt text. [S]
- [ ] **3.13** Verify: clean clone builds + runs zero-config; overlay build pulls real sources; BYO-LLM points at an alternate endpoint and works. [S]

## Phase 4 — Real-data go-live import (Cycle-3 6.5) *(after Phase 3)*

- [ ] **4.1** Import the cleaned hledger journal end-to-end — event emission → SurrealDB + journal-file projection round-trip (the part 6.4 stopped short of). Ends the cheap-breaking-changes window. [M]
- [ ] **4.2** Validate projected balances vs the source journal; dashboard/accounts reflect real data. [S]
- [ ] **4.3** Exercise the deferred Cycle-3 real-DB paths now that real data flows: R2 query (7.2) + base-currency setting (7.3) against the live SurrealDB. [S]

## Phase 5 — Editor feel + properties *(partly dogfooding-driven)* `(logbook)`

- [ ] **5.1** Inline properties panel (decision B) above the body; typed widgets for date / tags / 3 reflection keys; raw escape hatch for legacy props (`legacy_properties` / `has_legacy_properties` already exist). [L]
- [ ] **5.2** YAML↔form model kept in sync with the editor; the form emits parser-safe YAML. [M]
- [ ] **5.3** Harden `is_complete` (`core/src/events/notes_projection.rs:282`) to accept block lists / reordering / blank lines (also helps Obsidian-import compat). [S]
- [ ] **5.4** Typing-feel polish — open bucket, populated from the friction log as daily use surfaces it. [—]

## Phase 6 — Release polish

- [ ] **6.1** App logo (desktop + Android assets; replace default Tauri). [S]
- [ ] **6.2** Branch-gate workflow: feature branches + merge gates to protect stable. [S]
- [ ] **6.3** v1 semver stamp + git tag. [XS]
- [ ] **6.4** Archive + reset `project.md` (session log + status history → `.archive/`, leaving a lean current-state doc) once stable-v1 ships — it's grown unwieldy carrying every session's detail. Consider the same for `tasks.md`. Tie to the v1 tag so the archived snapshot is a clean cut point. [S]

---

## Running friction log *(fill during dogfooding; triage into the live phase)*

- **Android build-pipeline root cause — resolved 2026-06-08.** Symptom: no JS/frontend change
  reached the device all session despite many rebuild+reinstall cycles (only native Kotlin
  changes took effect). **Not a cache.** Cause: `tauri-build` **embeds `frontendDist` into the
  `.so`** and the WebView serves from there; `frontendDist`=debug + `npm run build` only
  refreshing the *release* dir ⇒ the APK baked in a **frozen stale debug frontend**. (An earlier
  version of this entry claiming "Android ships release via `copy:android:release`, ignores
  `frontendDist`" was **wrong** — corrected.) Fix: `scripts/android-build.sh [debug|release]`
  overrides `frontendDist`→release for the build only via `--config`; dev flow untouched.
  Verified: `.so` 51→40.5 MB, served index 512 B hashed, served bundle has the 1.10 listener,
  caret clears keyboard on device. Sweep `clean:release` also added (still valid — release dir
  accretes hashed wasms that all get embedded). Docs corrected in `tauri-app/BUILD.md`.
  **Deferred post-split:** real `devUrl` so `frontendDist`=release everywhere; remove dead
  `copy:android:release`; stop committing `editor.bundle.js`.

---

## Carried backlog (slot into a phase or pull from the friction log)

**Phase-5 reconciliation/import deferrals (from Cycle 3):**
- [ ] Inline-edit per detected recurring pattern before confirm (today: dismiss + rescan). [S]
- [ ] Balancing-posting affordance for hidden-fee resolution on merge (wire/FX fees). [S]
- [ ] Credit-card / CIBC CSV variant + real-export format verification (synthetic-tested only). [S]
- [ ] Reconciliation candidate engine: FX-spanning (cross-currency) matches. [M]

**Deferred stretch (from Cycle 2/3):**
- [ ] Daily Flow consistency visualizer redesign — frequency-aware (was 7-day hard-coded). [M]
- [ ] `BufferEvent::FlushFailed` → `StatusReporter` "stuck buffer" indicator. [S]
- [ ] Configurable `FORCE_GENERIC_DIRS` (hardcoded to `Work/`). [S]
- [ ] `auto_close_scheduler::AppState.event_store` → `Arc<dyn EventStore>` parity. [XS]
- [ ] Seconds duration unit on routine items (breaking event-schema change, 16 touch points). [M]
- [ ] `cargo:rerun-if-env-changed=TAURI_DEV_HOST` upstream contribution to `tauri-build`. [XS]

**Post-v1 / when-demanded:**
- [ ] PWA fallback (deferred Cycles 1-3).
- [ ] Veryfi `DocumentExtractor` impl (trait + routing scaffold already in place).
- [ ] ExchangeRate-API auto-rates for NGN (replaces manual per-statement entry).
- [ ] LLM-translated NL queries for R2 (evaluate; ship only if real usage demands).
- [ ] PaddleOCR sidecar (escape hatch from Cycle-3 7.11).
- [ ] WealthSimple native-Rust port (if the subprocess path proves stable).
- [ ] C1 email auto-fetch (vs paste); R3 self-employment dashboards; R4 tax-form validation.
- [ ] SurrealDB bump past 3.0.4 — **lockstep across both repos** (public + private overlay each pin their own lock; out-of-sync re-floats the overlay to 3.1 + `diskann`, which fails to compile on the current toolchain, rust#100013). No vector-search usage today, so no pull; revisit when vector search is wanted or the toolchain resolves #100013. Patch 3.0.x bumps are safe meanwhile. [S]

---

## Cycle 5+ filed

- Inbox management feature (user's "far future dream").
- Open Banking Canada evaluation (when bank adoption matures).
