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

- [ ] **1.1** Root continuity store: `use_context_provider` at app root (`tauri-app/frontend/src/main.rs`, joins existing tz/pending-share contexts), state keyed by identity (note path / capture-id / list-instance). [M]
- [ ] **1.2** Migrate journal editor (`pages/journal.rs`) to the store; drop page ownership of `content`/`last_saved_content`/save loop. [M]
- [ ] **1.3** Migrate generic notes editor (`pages/notes.rs`) to the store. [M]
- [ ] **1.4** Migrate in-flight finances captures (Phase-3 gap) into the store keyed by capture-id; "in-flight capture" affordance on Home. [M]
- [ ] **1.5** Migrate transaction-list pagination state (`transactions`/`offset` signals) into the store keyed by list-instance — fixes nav-to-detail-and-back reset (`project.md` carry). [S]
- [ ] **1.6** Relocate the auto-save loop into the store (preserve debounce + generation-counter cancel from `pages/journal.rs:151-222`) so unmount can't drop a pending save. [M]
- [ ] **1.7** Auto-save resilience: retry/backoff on failure + a glanceable save-state indicator (saved / saving / unsaved / failed). [S]
- [ ] **1.8** Workspace persistence (Level 2-A): serialize open-note/scroll/cursor/active-area to a settings file (mirror `commands/settings.rs` pattern); rehydrate on boot. [M]
- [ ] **1.9** Keyboard occlusion: extend `android-overrides/.../InsetBridge.kt` to read `WindowInsetsCompat.Type.ime()`, **chain** (not replace) the listener, inject `--keyboard-inset-bottom` CSS var (Android 15 SDK-35 requires the IME inset type). [M]
- [ ] **1.10** Web side: keep the caret scrolled above the keyboard via the injected var / `visualViewport`. [S]
- [ ] **1.11** Nav drawer (Dioxus) + hamburger button; nesting discipline so top-level destinations stay bounded. [M]
- [ ] **1.12** Edge-swipe open via `setSystemGestureExclusionRects` (native seam in MainActivity/InsetBridge). [M]
- [ ] **1.13** On-device APK verification: type→tab-away-mid-debounce→return (no lost text + restored position); kill/reopen (restored position); keyboard-up (caret visible); drawer swipe + button (no system-back misfire). [S]

## Phase 2 — Server go-live *(can overlap Phase 1)*

- [ ] **2.1** Provision Hetzner VPS. [USER] (account/payment)
- [ ] **2.2** Server runtime for the `server/` binary: systemd unit or container + persistent data dir. [M]
- [ ] **2.3** Extend `.github/workflows/ci.yml` with a deploy stage (CI test/build already exists). [M]
- [ ] **2.4** Sync reachability: phone↔VPS over Tailscale (sync auth still deferred per `feedback`/existing decision). [S]
- [ ] **2.5** Verify: phone syncs against the VPS with the laptop off. [XS]

## Phase 3 — Open-core split + extensibility *(the one-way door)* `(logbook)` `(demo?)`

- [ ] **3.1** Create private overlay crate (new workspace member) written against `core`'s `AutoImportSource`. [M]
- [ ] **3.2** Move bank adapters (`core/src/auto_import/{wealthsimple,wise,sc_ngn}.rs` + WS python driver + credential structs) into the overlay; keep generic plumbing (`imap*.rs`, `receipts.rs`, `mime.rs`, trait) public. [L]
- [ ] **3.3** Invert source instantiation in `server/src/main.rs` (`ImapSource::new` / `ScNgnHandler::new`) to pull from the overlay; public build = zero sources. [M]
- [ ] **3.4** Public app degrades gracefully to zero configured sources + zero declared accounts (no crash; manual entry / journal / budget all work). [M]
- [ ] **3.5** Subprocess-plugin runner: generalize the WS subprocess pattern into a config-driven `SubprocessSource` (command / args / schedule / secret-ref / account-map → JSON drafts). [L] `(demo?)`
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

---

## Running friction log *(fill during dogfooding; triage into the live phase)*

_(empty — daily-use friction lands here)_

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

---

## Cycle 5+ filed

- Inbox management feature (user's "far future dream").
- Open Banking Canada evaluation (when bank adoption matures).
