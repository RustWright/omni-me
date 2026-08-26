# Tasks — Cycle 4: Polish → Stable v1

**Target:** Take the three shipped core features (notes, routines, budget) to a stable,
daily-usable v1. Completion bar is deliberately subjective: **"polish until the app is
comfortable to use daily."**

**Status:** Cycle 4 Session 4 (Planning) complete 2026-06-06. Phase 1 is the first
implementation target (Session 5). Plan file: `~/.claude/plans/vivid-cooking-kurzweil.md`.

**⛔ v1 CLOSE-OUT GATES (must all clear before stamping v1):**
1. **Sync-integrity fix** (Session 6, in progress) — apply-layer ✅ done (d049f1e); auto-sync + backfill/cold-open remain.
2. **One final DB reset + re-import** (already queued) — also carries the date-as-ID cutover.
3. **User dogfooding confirmation pass** — verifies observable behavior on both devices.
4. **Full code review** (user, 2026-07-05) — an *additional, non-negotiable* requirement: a read-the-code
   pass for latent bugs, distinct from the behavior pass. Its own session (Session 6 = Code Review in the
   process); use `reviews/` per-perspective + optionally `/code-review ultra` (user-triggered/billed).
   Rationale: recent cycles were rushed, so bugs were missed/deferred — the review is the un-rush gate.
   See memory `feedback-rushing-caused-bugs-review-gate`.

**🗺️ AGREED RELEASE ROADMAP (user, 2026-08-24) — the ordered march to v1.**
*Each numbered push is done in FRESH context: the user compacts before starting each one.
Do the current step, stop, let the user compact. Do NOT run ahead.*

  **A. Feature-completion (one push each):**
  1. **#594 Stage D** — finish the design-primitive rollout (A/B/C done; D = roll the shared
     `Card/Button/PageHeader/...` primitives across journal / notes / settings / routines).
     *User thought this was already done — verify what's actually left before building; may be partial.*
     **AUDIT (2026-08-24): NOT done. Finances is on the new elevated-card system (`obsidian-surface`
     #262626 / `rounded-card` 12px / `shadow-card` / `obsidian-border` token, 17× each); the other 4
     pages are still the old flat look (`obsidian-sidebar`/`white/5`, `rounded-lg` 8px, no shadow,
     `white/10` literal — 0× the new tokens). Primitives barely adopted even in finances (inline cards ×6).
     Note: `obsidian-border`==white so border swap is light-theme-only (invisible in dark); surface/radius/
     shadow are the visible deltas. `PageHeader` primitive lacks `tracking-tight` the pages all use → fix it.
     **SCOPE = Option 3 (user, 2026-08-24): FULL primitive refactor across ALL 5 pages** (incl. finances) +
     fold `date_field`/`account_input` input copies into `INPUT_CLASS`. Rationale: primitives as single
     source of truth to facilitate NEXT-phase design changes; accepts higher breakage risk (tests +
     Playwright + on-device catch as the net). Verify loop: `cargo check --target wasm32-unknown-unknown
     --features mock` (~8s) + a non-mock check per page. One reviewable commit per page.*
     **✅ DONE (2026-08-24). Six commits, one reviewable per page + the fold:** `eb05afe` notes,
     `6b758e3` routines, `ddc142d` journal, `d56abca` settings, `51d92bf` finances, `2842bda` fold
     `date_field`/`account_input` → `INPUT_CLASS`. Extended primitives: `Card` gained `onclick`,
     `PageHeader` gained `class` + `tracking-tight`, added new `IconButton`. Adopted `PageHeader`/`Card`/
     `Button`/`IconButton`/`Banner`/`SegmentedNav`/`TextInput`/`INPUT_CLASS` across all 5 pages; left
     bespoke by design: interactive **button-cards** in finances (keeping native `<button>` a11y over a
     `div`-Card), the p-5 net-worth hero (custom padding), divided-row wells / grouped panels, uppercase
     micro-pills, warn/tinted chips, and the one finances header with a dynamic subtitle. Each page:
     `cargo check` mock + non-mock + clippy all green, Playwright-verified against `dx serve` (mock).
     **Not yet done: on-device/real-data pass** (mock can't exercise the backend; rides the queued DB reset).*
  2. **#370** — desktop freezes on FIRST open after a full reset (not on subsequent opens); editor
     stuck on "Initializing editor environment…". **🅿️ SHELVED 2026-08-24 (user, not reliably
     reproducible; workaround = close+reopen).** Investigation (verify-first, per
     [[feedback-verify-hypothesis-before-build]]) — the hang means `editor_ready` never flips →
     `window.createEditor` never defined within the 20s poll cap (`editor.rs`) → `editor.bundle.js`
     failed to load/stalled. Existing hardening already covers the obvious causes and POST-DATES all
     of it (onload→poll `9e1119a` 2026-07-04; 5s→20s cap; mobile dropped-invoke fix `1357151`
     2026-07-05; freeze seen 2026-08-23). **Eliminated this session (debug build, isolated
     `XDG_DATA_HOME` scratch dir = non-destructive true cold first-open):** (a) empty cold-open →
     opens fine; (b) cold-open pointed at the real box (`omni-box-hetzner:3000`, health 200) → opens
     fine, BUT the `PullScheduler` logged **zero** pull/backfill in 60s (box returned no backlog for a
     fresh device) → the **backfill-contention hypothesis is UNTESTED, not disproven**. **Leading
     remaining hypotheses:** (1) RELEASE build timing — faster init makes the editor request the
     bundle *before* the wry custom asset-protocol handler is ready → request silently dropped (the
     desktop analogue of the mobile dropped-IPC bug; `editor.rs` history notes the release path
     behaves differently from debug); (2) a genuinely large first-open backfill starving the
     asset-protocol handler / main thread (`puller.rs:30` warns of the "large backfill write batch"
     ~4s in). **To resume when reproducible:** re-add the on-screen self-report diag (init-status +
     `load`/`error` listeners on the injected `<script>` — the current path polls ONLY, so it can't
     tell "dropped" from "slow"; reverted this session to avoid committing scaffolding), reproduce in
     a **release** build and/or against a box WITH a real backlog, then confirm before fixing.
     **Proposed fix (unverified — do NOT ship blind):** self-heal on give-up — re-inject the script /
     retry the bundle load after the timeout instead of stranding, mirroring the mobile timed-retry;
     that automates the user's close+reopen recovery. Repro harness that works: build binary, run with
     `XDG_DATA_HOME`/`XDG_CACHE_HOME`/`XDG_CONFIG_HOME` → empty scratch dirs (seed only `server_url`).
     `grim` can't screenshot this GNOME/Mutter Wayland session (no wlr-screencopy) — use the on-screen
     diag readout + the user's eyes, or the WebKit inspector.
  3. **#372** — ✅ DONE 2026-08-25 (web-verified; on-device rides next APK). Android
     hardware/gesture back now pops in-app nav first. Frontend owns a `BackNav` context
     (`main.rs`): each page reports its drill-down depth + pops one level via `use_page_back`;
     the root publishes `window.__omniCanGoBack` and, on an `omni:back` DOM event, applies the
     precedence drawer → page drill-down → non-home tab → (home root) let the OS background the
     app. Kotlin `MainActivity.onBackPressed` reads the flag and dispatches `omni:back` or
     `moveTaskToBack(true)` (backgrounds instead of destroying — keeps the app warm, dodges the
     #370 cold-open). Wired Finances (multi-level, via new `finances_back_target` map + 3 unit
     tests), Notes, Routines (2-level), Journal (calendar drawer). See friction entry below. →
     **next push is #367.**
  4. **#367** — ✅ DONE 2026-08-25 (web-verified; on-device rides next server deploy). Runtime
     off-switch: a Pause/Resume toggle on EVERY "Running now" row (incl. compiled overlay bank
     sources — they key on their registry name, no `sources.toml` entry needed). `SourceRegistry`
     grew `pause` (live-abort the task, keep the entry + config, flag `paused`) and `resume`
     (re-spawn + immediate pull). The pause is **persisted** (`core/.../auto_import/paused.rs` →
     `paused_sources.toml`) and re-applied at boot by threading the paused set into
     `spawn_sources`, which **registers-without-spawning** a paused source — so it doesn't tick
     even once at startup (the whole point for a runaway bank source). `reauth`/`rearm_if_dormant`
     deliberately do NOT un-pause. Full detail in the friction entry (#off-switch) below.
     → **next push is the full code review (#5).**

  **B. Review gate:**
  5. **Full end-to-end code review of everything** (per the v1 close-out gate #4 above).
     **🔄 IN PROGRESS — Phase A COMPLETE + calibration DONE 2026-08-25; Phase C: logic doc TRIAGED + FIXED 2026-08-26 (3 docs left).** Third review of the project; scope is the
     never-reviewed Cycles 3+4 range `22395f8..HEAD` (230 commits, +49,501/−2,329). Four perspective
     docs at `reviews/2026-08-25-*.md` (gitignored — durable summaries in `project.md`'s Session-6 row;
     private-overlay findings in `omni-me-private/reviews/`). Cycle-2 model split reused (Opus:
     security+logic, Sonnet: perf+bloat) via 4 parallel Write-less reviewer subagents.
     **5 Criticals.** Logic: (1) `journal_file.rs:247` renders descriptions unsanitized → one `*`-prefixed
     payee aborts the whole-file ledger parse and collapses Accounts/net-worth/dashboard; the importer
     strips this *because* it can't round-trip, but no write path does. (2) Pull cursor is server-clock
     while the pull filter is author-clock → an offline device's backlog is **never delivered to peers**;
     **verify before roadmap steps 7–8**, which bulk-import months-stale dated data.
     Security: (3-5) unauthenticated `POST /auto_import/sources` = subprocess RCE on the box, `rest`-source
     `[secrets]` exfiltration, and `PUT /llm/config` redirecting notes+receipts — all three collapse into
     one ~50-line bearer-token middleware. `CorsLayer::permissive()` verified unused by the client.
     **3 prior-deferral status upgrades** (CORS, Gemini key in query string, CSP); blanket
     "auth deferred (Tailscale)" ruled **no longer defensible** — it predates the server executing
     commands and writing secrets, and was out of scope in *both* prior reviews.
     **SITTING 2 (2026-08-25)** — 5 reviewers over ~20k never-reviewed backend lines (money chain,
     event store + 6 projections, sync, server routes, auto-import/secrets, platform, both repos).
     **4 more Criticals, every one re-verified first-hand before filing.**
     (1) `JournalFile` is a **registered production projection** (`lib.rs:300`) whose handlers raw-append
     with no `; txn_id:` anchor check — the only non-idempotent handler in the system. `pull_only` has no
     in-flight guard and Sync Now is a bare `spawn`, so two taps (or one during the 20s scheduler pull)
     write every pulled txn into `budget.journal` **twice** → balances silently diverge from the Ledger
     list and never self-heal. Also fires with zero concurrency whenever a peer's author timestamp leads
     the server clock.
     (2) `ParserPosting::reality` is never read anywhere (grep: **0 hits**) → `[Assets:Budget:Food]`
     virtual postings import as **real** and inflate net worth.
     (3) The elided-leg split never consults `p.balance` (grep: **0 hits**) → `Assets:Cash = 500.00 CAD`
     is treated as the elided leg and handed an invented amount; assertions are also never re-rendered,
     so every reconciliation checkpoint is erased on regeneration.
     (4) An **empty** description renders an unparseable line → same whole-file collapse as sitting 1's
     `*`/`!` bug; reachable from a bank CSV with a blank memo column.
     (5) **These are one class, not five bugs** — `render_*` writes user text into a grammar without
     escaping and `TransactionRecordedPayload::new` validates nothing. Five known variants now
     (`*`/`!`, empty, zero-postings, leading `(code)`, embedded ` ;`, comma-in-tag). `many0(item)+eof`
     means no partial success, so one bad row of 10,200 takes down net worth + Accounts + roster
     together. **One validation point fixes all of them** — do this before roadmap step 7's re-import.
     **Key warnings:** routines is the projection family nobody hardened (4 bare `UPDATE`s → a rename
     that outruns its create is lost forever); the **push** path has no poison-pill escape (server 400s
     the whole chunk, retry resends it forever → all outbound sync wedged); `sync/client.rs:306` is the
     push-side twin of the mixed-clock Critical; `SyncBuffer` (691 LOC) is **dead in production** —
     `append` has zero non-test callers — and that is the root cause of the missing-`trigger()` bug.
     **Security:** no new Criticals. One unparseable email permanently kills a mailbox's auto-import
     (`imap.rs:122` `?` inside the dispatch loop, cursor advance below it); untrusted PDFs reach
     unsandboxed poppler uncapped/untimed; the subprocess helper inherits `GEMINI_API_KEY`. Repo-wide
     grep for `.env_clear(|timeout(|.kill(|current_dir(` in `auto_import/`: **0 hits**.
     **Performance:** the transactions projection defines ~20 fields and **zero indexes** — the only
     projection without them → every Finances read is a full scan at N=10,200. `budget_progress`
     bypasses the journal cache (distinct from the documented `net_worth_history` case).
     **Bloat found a bug:** `import.rs:257` + `journal_import.rs:358` never call `push_debouncer
     .trigger()` — Obsidian/hledger imports never wake the pusher; `auto_import.rs:471` documents this
     exact bug, already fixed once next door. Plus 4 commands registered but unreachable from any UI.
     **CI gap (orchestrator):** `omni-me-app` is **never compiled by CI** (`-p` allowlist omits it);
     its 38 tests and the frontend's 82 never run; no clippy, no fmt. Structural cause of the Cycle 2
     "release builds don't compile" blocker. **Fix:** `--workspace` instead of the `-p` allowlist.
     **Calibration:** the `/code-review ultra` run was **cancelled by the user (2026-08-25)**. Replaced
     with a zero-cost retrospective substitute: the friction log's resolved root causes are real bugs
     found by dogfooding rather than review, and 6 of 8 sit in files sitting 2 just covered — so
     "would this pass have caught it from the code alone?" becomes the miss-rate test. Corpus extracted;
     runs at the tail of sitting 3.
     **SITTING 3 (2026-08-25)** — 4 reviewers over the frontend (20,411 Rust) + `editor.js`
     (952 JS, zero coverage). **Phase A is now COMPLETE.** **4 more Criticals, all re-verified.**
     (1) `journal.rs:553-575` — the #344 background flush persists **body-only text as the whole
     note, destroying the frontmatter**. `js_flush_editor_timestamps()` returns the CodeMirror doc,
     which since Phase 5 is the **body only** (`Editor{initial_content: body.peek()}`, :865), but it
     is passed to the same `invoke_update_journal_entry` parameter the manual Save fills with the
     **full** note (:818). Both guards fail (body vs full note never compares equal), and
     `last_saved_content.set(stamped)` then records the mutilated copy as persisted. **Just opening
     today and backgrounding the app** wipes `date`/`tags`/the three reflection keys. Same class as
     the money-chain Criticals: two value shapes, one `String` parameter, no type distinction.
     (2) `components/editor.rs:84` — the `use_effect` reads no signal, so the editor is built **once
     per mount** and `read_only` flipping never rebuilds it: after **Reopen** the day is permanently
     un-typeable, and after **Close Day** the body stays editable while autosave silently bails →
     typed text discarded with no feedback.
     (3) `routines.rs:185` — the checklist list is **unkeyed** (verified: no `key:` on the wrapper
     div or `ChecklistGroup`) and `ChecklistGroup`'s `use_future` captures `gid` once, so a reorder
     (or a peer deleting a group) leaves items bound to the wrong card and a tick files
     `complete_routine_item(iid_of_a, gid_of_c)` — **completions recorded against the wrong group**.
     (4) `journal.rs:106` — `selected_date` is seeded from `UserDate::today()` on the first render,
     when `tz_signal` is still the `Tz::UTC` default, and **nothing re-anchors it** when the real tz
     arrives. Evening in a UTC-behind zone opens **tomorrow's** entry; the wrong date then persists
     into nav, so it survives the fix unless the stored value is corrected too.
     **Key warnings:** `editor.rs:182` discards the teardown timestamp stamp on plain navigation —
     the **unfixed half** of the already-closed "Android last line loses its timestamp" friction
     entry; `notes.rs:459` leaves a fetch error hydrated+editable so one keystroke autosaves over
     the real note (`journal.rs:393` deliberately does the opposite); `finances.rs:3059` can persist
     an **empty `ListState`** that permanently blanks the Ledger *across restarts*; `load_more`
     (:3072) is re-entrant → duplicate page.
     **Security:** no new Criticals. One High — `MainActivity.kt:39` force-enables WebView DevTools
     on **every** build (deliberate: Android must build `--release`), so any release APK exposes the
     full `__TAURI__.core.invoke` surface over adb. Filed as a **v1 decision**, not a defect — it is
     the on-device debugging mechanism; fix gates the trigger, keeps the capability. Also settles the
     CSP blast-radius question: the editor's `window.*` globals add nothing on top of `__TAURI__`.
     Editor.js is clean of HTML sinks (both widget `toDOM`s use `createElement`+`textContent`).
     **Performance:** the continuity store inserts a full-text entry the first time a day/note is
     **opened** and `remove()` is called from exactly **one** site (`notes.rs:671`, `NewNote` only),
     while `snapshot_for_persist()` deep-clones every tracked session on **every** change — only the
     disk write is debounced. Typing one character deep-clones every document ever opened; cost grows
     with **app age**, and inflates the boot parse. Zero `use_memo` against 308 signals.
     **Bloat:** `components/mod.rs:5-12`'s dead-code rationale is stale **in both directions** —
     `Card`/`PageHeader`/`INPUT_CLASS` have 23/16/19 real uses while `Section` (claimed live) has
     **none**; real dead set is `Section`/`StatTile`/`FieldLabel`/`Trend`. `getEditorContent` +
     `getEditorCursor` are dead on **both** sides of the wasm boundary (converged with Security).
     **Orchestrator own-pass:** rebuilt `editor.bundle.js` from source and diffed — **identical**, no
     drift. The `frontendDist`=debug trap from friction 1.13 was **suspected and disproven**: all
     three real build paths override it via `--config` (verified-clean, recorded so it isn't
     re-investigated). Frontend tests: all **82 sit in 9 pure-logic modules**; `autosave.rs`,
     `continuity.rs`, `sync_refresh.rs` (570 LOC, the "did typing get saved" trio) have **zero** —
     and `autosave.rs` already lost user data. Top Phase B target, and small.
     **CALIBRATION DONE** (`reviews/2026-08-25-calibration.md`) — substitute for the cancelled
     `ultra`. Scored all 9 root-caused friction bugs: **5 findable, 4 structurally unfindable.**
     The split is by class, not luck. Data-integrity/event-sourcing: **strong** — F2 (transactions
     projection bare `UPDATE`) had to be found by dogfooding, and sitting 2 then found the same class
     in the last unhardened family (`routines_projection.rs` still has 4 bare `UPDATE`s at
     :145/:172/:244/:269, verified live); F8 is sharper still — sitting 3 found the **unfixed half of
     an already-closed friction entry**, which is exactly the failure mode a review gate exists for.
     Platform-runtime/visual-layout: **~0%, and more review will not help** — webkit2gtk focus grab,
     `min-width:auto` at 390px, missing `color-scheme` are invisible in source by construction.
     **Consequence for v1:** the instrument that covers the unfindable half is `ui-checklist.md`,
     which reads "Last tested: 2026-04-24" and describes a **deleted 3-tab nav**. Nearly half the real
     bug tail is currently guarded by a stale document → **refreshing it on-device at real widths
     should be a v1 gate item, not polish.**
     **Comment convention — RAISED then DEFERRED by user 2026-08-25 (post-triage, may not be needed).**
     Prompted by two comment-caused findings (`editor.js:808`'s wrong `editable` vs `readOnly` claim;
     `components/mod.rs:5-12`'s stale dead-code inventory), the question was whether to sweep every
     comment in the codebase and set a convention (incl. when to use `todo!()`). **Measured first, and
     the premise didn't hold:** comment density is **13-20%** across all four areas (normal), and
     markers are **2 TODOs** (both in one scoped `TODO(android-native-callback)` block), 0 FIXME/XXX/
     HACK, **0 `todo!()`** — no marker rot to clean. An **exhaustive** audit of the two shapes that
     actually caused bugs found **8 instances total**: 1 harmful (`editor.js:808`), 1 stale
     (`mod.rs`) — **both already filed with fixes** — 1 worth a triage look
     (`journal_template.rs:4`, asserts "used two places" + a claim about `notes_projection.rs`), and
     5 harmless (self-contained rationales). A full sweep would read ~7,400 comment lines to find
     ~1 more instance, and would strip comments that **actively sped this review up** (the
     `desktop-build.sh`/`android-build.sh` headers disproved the `frontendDist` bug in minutes;
     `snapshot_for_persist`'s doc comment documented the perf bug).
     **The rule, if it's ever written:** a comment is safe when it describes **the code it sits on**
     (why this value, why this approach) and risky when it describes **the state of code elsewhere**
     (who calls this, what's used, how a third-party API behaves) — the first can't rot unless you
     edit that line, the second rots when someone edits a *different* file. Both our bugs were the
     second kind. **Deeper form: prefer enforcement over assertion** — drop `allow(dead_code)` and let
     rustc maintain the inventory; a shared const instead of `WIPE_CONFIRM_PHRASE`'s comment-only
     defence; a test spanning both `⟦⟧` encoders instead of a cross-reference note; and a
     `Body`/`FullNote` newtype would have made the frontmatter Critical a **compile error**.
     `buffer.rs:297` already does this right (`unimplemented!("append never called by SyncBuffer")`).
     **Decision:** fold comment fixes into Phase C opportunistically (those files are being edited
     anyway); revisit the convention after triage and only if it still looks needed.
     **PHASE C — logical-consistency document TRIAGED AND FIXED (2026-08-26).** All 40 findings
     dispositioned and annotated in place, 8 commits, every one green (core 504 / app 39 / server 21 /
     frontend 82, clippy clean ×4 configs, golden guardrail green). Three user decisions taken:
     unsupported hledger syntax → **refuse and report** (0 instances in the real 10,209-txn journal);
     `SyncBuffer` → **delete** with the push nudge made structural; `AccountReconciled` → **remove**.

     Highlights worth not re-deriving:
     - The `account`-directive Warning was a **live blocker on step 8**, verified both ways against the
       real file: fixed = 10,209 txns / 0 errors; control with the fix disabled = **0 imported**.
     - A **sixth** append-without-nudge site (`auto_import::dismiss_batch`) was found during triage, and
       the class is worse than filed: `pusher::run_loop` has no interval fallback, so an un-nudged bulk
       import pushes **nothing**. Second independent cause of "imported data absent on mobile".
     - Enforcement added per the user's "defensive" instruction:
       `commands::shared::tests::no_command_appends_events_directly`, control-verified both directions.
     - One regression test was **caught worthless** (passed against the broken code) and rewritten;
       control now shows 0-of-4 vs 4-of-4. Same discipline disproved an assumed fix (ledger-parser v6
       rejects a bare unqualified amount).
     - Deferrals, each with a trip-wire: `Prices` precedence (unreachable — 0 `P` directives live),
       `DefaultHasher` ids (single toolchain today), `event_mapper` `.abs()` (**tried, reverted, KEEPING**
       — it is deliberate and tested; needs a real refund receipt to settle). Projection `version()`
       bumps deliberately not taken (step 8 wipes anyway).
     - UI verified with `dx serve` + Playwright: Journal / Routines / Finances render, Ledger survives
       the navigate-away round-trip, 0 console errors. Denylist clean, 0 hits / 22 patterns.

     **Remaining:** triage the **security**, **performance** and **bloat-complexity** documents (same
     rules: every deferral gets a trip-wire; verify prior FIXED markers) → Phase B test-gaps (written
     AFTER Phase C, biased toward absence-tests). **13 Criticals total across 3 sittings; the 4 logic
     Criticals and all 4 frontend Criticals are now fixed.**

  **C. Email ingest prep:**
  6. **Variation of #337 + #341** — prep ALL the user's email inboxes so the app cleanly ingests
     everything it needs. Fix bugs / add necessary features *as they arise in the process*, ad hoc.

  **D. Data catch-up (clean data BEFORE the wipe):**
  7. **Bring imports current** — the ledger import is months stale; use the **paisa process** to get it
     up to date and import-ready. Same for the **Obsidian journal** data (stale — user turned OFF Obsidian
     sync anticipating this; **user will provide / point to the most up-to-date journal data**).

  **E. Box wipe + clean re-import:**
  8. **Wipe the box clean of everything** — current live data is deemed not worth sorting through;
     re-import from the clean data produced in step 7.

  **F. Phase 6 full release:**
  9. Phase 6 polish (6.2 branch-gate, 6.3 v1 semver + tag, 6.4 archive/reset the bloated docs) →
     ship an **updatable app on mobile + desktop** → make a **trivial change and test the update path
     end-to-end**; fix it if the OTA path is broken.

  **POST-RELEASE (explicitly gated — only after the core app is solid on journal + routines + finances):**
  - LLM Chat (the main way to get data/insights out).
  - Task + project tracking.
  - Overall inbox monitoring + personal-assistant capabilities.
  - *Do NOT add any new top-level category until the existing three are working end-to-end.*

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

- [x] **2.1** Containerize *runtime* config — split-agnostic. **DONE 2026-06-23 (public, bank-free image).** Multi-stage `server/Dockerfile` (builds `-p omni-me-server` only; OpenSSL-only native deps; non-root uid 10001; curl HEALTHCHECK) + root `docker-compose.yml` (one named `omni_data` volume → `/data` carrying both `surreal_data/` + blobs via `BLOB_DIR`; read-only `credentials.toml` mount + optional `.env` for `GEMINI_API_KEY`; `restart: unless-stopped`; 30 s graceful stop) + `.dockerignore` + bank-free `secrets/credentials.toml.example` + `.env.example` + `.gitignore` rules. Verified locally with Docker (sudo-gated): clean build, `/health`→200, zero-config boot (`sources=0`), volume persists across `down`/`up`, SIGTERM graceful stop in 0.2 s. Full design + the deferred pipeline live in `~/.claude/plans/lets-start-phase-2-whimsical-hopcroft.md`. [S]
- [x] **2.2** Provision the production box + Docker + Tailscale bootstrap. **DONE 2026-06-28 (Hetzner CX23, Ubuntu 24.04).** Walked the line-by-line runbook live: hardened `deploy` user (key-only + passwordless sudo), Docker + compose, Tailscale (`tag:server` + keyless Tailscale-SSH for CI), tailnet-only `ufw`, GHCR read-login, out-of-band credentials, runtime-mounted Python layer. **DigitalOcean dropped** — its $200/60-day credit is now referral-only; **Netcup** picked as the future parametric backup (account-create + deploy to be confirmed when a second box is actually wanted — not provisioned, avoids idle spend). [USER]
- [x] **2.3** Deploy pipeline — **2.3a image DONE 2026-06-23; 2.3b CD pipeline AUTHORED 2026-06-23 box-independent, box-present RUN DONE 2026-06-28 (first deploy green: build → private GHCR → ephemeral tailnet node → on-box `remote-deploy.sh` → health-gate; `OMNI_BIND_ADDR`=tailnet IP for tailnet-only serving).** Homed in the private overlay (Model A). **2.3a:** built + locally Docker-verified the personal image — multi-stage `Dockerfile` in `omni-me-private` building `-p omni-me-private` (overlay server binary + its subprocess helper, side-by-side for sibling-of-exe discovery); public source fed in via a BuildKit named context exporting `../omni-me` HEAD with `git archive` (target-free; working-tree `target/` is 17 GB), under `--locked` (surrealdb 3.0.4); plus `build.sh`, a run-only `docker-compose.yml`, `.dockerignore`, a bank-section `secrets/credentials.toml.example`, `.env.example`, gitignore. 5/5 local checks green (both binaries ship; `/health`→200; zero-config `sources=0`; overlay wiring proven via a dummy bank source → live source → real outbound-TLS call + graceful 401 backoff; volume persists across `down`/`up`; SIGTERM stop 0.2 s). **2.3b (CD pipeline) AUTHORED box-independent 2026-06-23:** a manual-dispatch GitHub Actions `deploy.yml` (build job: clone the public engine into a sibling checkout — same named-context trick as the local build, so the path-deps + pinned lock stay valid; **no git-rev dep swap** — that was considered and rejected, it would churn the lock + re-float surrealdb past 3.0.4; SHA-tag → push to **private GHCR**; deploy job: join the tailnet as an ephemeral node → ship the prod compose + scripts → run the on-box deploy) + box-side scripts (`snapshot`/`restore-snapshot`/`health-gate`/`remote-deploy`: pre-deploy volume snapshot → pull → compose up → health-gate on Docker health → **auto-rollback** to the prior image, then snapshot-restore if still unhealthy) + a production compose (GHCR image via `${OMNI_IMAGE}`, tailnet-only port bind, out-of-band credentials mount + mounted Python-driver runtime) + OCI provenance labels + a line-by-line provisioning runbook + operator guide. Decisions: **bank secrets out-of-band on the box** (never GitHub — only the Tailscale auth is a GH secret); the subprocess bank source's **Python-driver runtime is mounted at runtime** from the box (decoupled, so a future Rust rewrite drops it with zero image change), its session seeded by the shipped in-app OTP re-auth. Verified **static** (bash -n on the scripts + YAML well-formedness; the build path itself is already proven by the local image). **Box-present run remains:** 2.2 provisioning [USER] (the runbook is ready to walk through) + first deploy + 2.4/2.5 go-live. Parametric target (DigitalOcean trial first / Hetzner long-term). [—]
- [~] **2.4** Verify go-live — **laptop-off VPS sync PROVEN 2026-06-28** (the phone points at the tailnet box and syncs; the laptop is out of the picture). **Remaining:** kill-one-box HA + a forced-rollback test — both need the second (backup) box, deferred. [S]
- [x] **2.5** **DONE 2026-06-28.** Precondition was met 2026-06-17 (app-entered re-auth shipped + real-OTP-verified); this session the deploy landed and the live bank source's in-app OTP re-auth was verified **against the VPS** with the session persisting in the volume — the SSH-for-OTP failure mode is fully gone. [—]

## Phase 3 — Open-core split + extensibility *(the one-way door — RUNS BEFORE Phase 2)* `(logbook)` `(demo?)`

**First decision when opening this phase:** Model A (bank adapters compiled into a private image;
deploy pipeline lives in the private overlay repo) vs Model B (public zero-source image everywhere;
bank adapters run as box-local subprocess plugins per 3.5 — deploy stays mostly public). Lean A now
(least work, matches 3.1–3.3), B as the end-state once the subprocess mechanism matures. This choice
sets the Phase 2 deploy topology.

**STATUS 2026-06-14 — STEP 1 (relocate) COMPLETE (3.1–3.3).** Destination resolved = Model B via a
foundation-first path. Private overlay `RustWright/omni-me-private` created + pushed (`main`@`8b07e83`)
+ registered as a `productive_learning` submodule; public engine bank-free + 446 tests/clippy clean;
private clippy clean + 23/23 adapter tests + green smoke test vs real config (graceful private-source degradation
verified live). SurrealDB pinned 3.0.4 in both repos (lockstep — see backlog).

**STATUS 2026-06-15 — STEP 2a (subprocess contract + first pull-helper) COMPLETE.** Froze the engine↔helper
JSON contract in prose (`SUBPROCESS_SOURCE_CONTRACT.md`) + code (`HelperRequest`/`HelperResponse`/
`HelperStatus` public serde types in `core::auto_import::subprocess`). Built the generic public
`SubprocessSource` (6 fake-helper tests). Converted the first bank adapter to a standalone helper binary in
the overlay (`fetch_drafts` + a `src/bin/<helper>.rs`) that **reads its own credentials** — the engine never
sees a bank secret (boundary now structural). Helper discovery = sibling-of-current-exe + an env override
(the convention all future plugins reuse). Hardened the `driver_script` path (engine validates the helper
command; helper resolves its driver to absolute + existence-checks). Public 6 new tests + clippy clean
(still bank-free); private builds both binaries + 22 tests + clippy clean. **Smoke-verified live:** the
source now ticks via `SubprocessSource → helper → vendor driver`; a real session-expiry → driver non-zero
exit → `needs_reauth` → graceful backoff, the other live sources unaffected. **STEP 2b = next session:**
app-OTP re-auth full stack (3.5a — `AuthState` + `/reauth` route + the helper's `reauth` verb (already
frozen in the contract) + Dioxus "Reconnect" UI). Deferred: a second adapter's helper; the email-handler
source (out-of-scope per the contract — stays in-process; recommended future shape = helper decrypts PDF,
engine extracts via LLM); real account-map (3.9).

**STATUS 2026-06-15 — STEP 2b (app-OTP re-auth backend slice, 3.5a server half) COMPLETE.** The
`needs_reauth` signal is now a tracked, exposed `AuthState` instead of a buried log line: new
`ImportError::NeedsReauth` + `AuthState`/`ReauthOutcome` public types + defaulted
`reauth`/`reauth_capable` trait methods + registry state-tracking (`record_tick` flips to
`NeedsReauth`; a clean tick or successful reauth clears it; a transient blip leaves it) + registry
`reauth()`. `SubprocessSource` threads the new variant and speaks the frozen `reauth` verb; the bank
helper runs its driver's fresh-login path (exit 0→`reauth_ok`, 4→`invalid_otp`, else→`error`); new
`POST /auto_import/reauth` (OTP in body, not URL); `GET /auto_import/status` now carries `auth_state` +
`reauth_capable`. Public +14 tests (62 auto-import) + clippy clean both feature configs + server clippy
clean; private +4 tests (26) + clippy clean + both bins rebuilt. **Live round-trip vs real config
(GREEN):** status exposes the fields (`reauth_capable` true only for the SubprocessSource, false for the
other live sources); a dummy-OTP reauth drove the *real* driver → `invalid_otp` (the driver already
supports the otp fresh-login path); unknown source→404; a non-capable source→`not_supported`; the
subprocess source's scheduler tick surfaced `needs re-auth` and backed off while the other sources ticked
clean. **Caught + fixed mid-verify:** the first round-trip ran a stale binary
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
"data flowing" vs imperative "user must act") — a degraded-but-active source shows no callout, only
`needs_reauth` does. Clippy clean (Tauri + frontend, both feature configs). **Playwright mock
walkthrough (5 states, 0 console errors):** the reauth-capable source shows the callout, others don't; Reconnect reveals the
field; `000000`→rejected (field stays); valid 6-digit→success→row returns healthy on refresh. PNGs in
`logbook/_assets/source-reauth-reconnect/`. **Real-OTP happy path proven E2E vs the real account**
(`cargo tauri dev` desktop → private server on real `credentials.toml`): a live TOTP flipped
`auth_state → active`, then a manual `Fetch now` came back `last_outcome: success` / `health: healthy`
— proving the **session refreshed**, not just the flag cleared (the two-clocks model: `registry.reauth`
only flips `auth_state`, so the successful pull is independent evidence). **This unblocks 2.5.**

**STATUS 2026-06-17 — 3.4 + 3.6 + 3.7 (public engine usable with no banks) COMPLETE.** The public
engine is now self-sufficient: it boots + runs clean with zero config (**3.4** — fixed a real
boot-time panic where the Gemini-key `.expect()` crashed a no-key install; now degrades to
`NullExtractor`/empty-key client), builds **generic sources from a server-side `sources.toml`** at
boot (**3.6** — native `CsvSource` + the existing `SubprocessSource`, via `core::auto_import::config`;
public binary rewired off `no_sources`), and lets the user **add/edit/remove those sources in-app**
(**3.7** — `GET/POST/DELETE /auto_import/sources` + a rebuilt Settings panel; restart-to-apply). Apply
model = restart-to-apply (user call "restart now, live later"); the running registry is never mutated,
so changes take effect on next server boot. **Verified:** 15 new `core` unit tests; clippy clean across
`core`(auto-import)/`server`/`omni-me-app`/`frontend`(mock+default); a live server smoke (zero-Gemini
boot + CSV `tick` → 1 batch event + sources CRUD incl. 400/404); a Playwright mock walkthrough (add →
"pending restart" + banner → remove → edit-prefill w/ locked name → CSV↔Subprocess field swap, 0 console
errors). **Deferred (next):** live add/remove (registry handle-tracking + `deregister`); REST source +
`ImapSource` wired into the public config builder; per-source `schedule_secs`. `(logbook)` capture for
3.7 deferred to a later drafting pass; PNGs in `logbook/_assets/config-driven-sources/`.

**STATUS 2026-06-18 — 3.6/3.7 fast-follows + 3.8 text-side COMPLETE (public-repo only).** Three slices,
all verified. **(A) live add/remove:** `SourceRegistry` now owns each scheduler task's `JoinHandle` +
gained `spawn_one`/`remove` (explicit `abort`); `AppState` carries the build context (`store` /
`projections` / `device_id` / `default_interval`); `config::build_one` factored out of
`build_generic_sources`; the `POST`/`DELETE /auto_import/sources` handlers build+spawn / abort in place
(`{"applies":"live"}`) — no restart. **(B) per-source `schedule_secs`:** honored via a *defaulted*
`AutoImportSource::poll_interval()` (CSV/subprocess carry it; `spawn_one` uses it, else the global), which
sidestepped the `SourceBuilder`-seam ripple → **zero private-overlay change**. **(C) 3.8 BYO-LLM
text-side:** new `core::llm::OpenAiCompatClient` (chat/completions; `complete_json` via `json_object` +
schema-in-prompt; `complete_with_tools` via OpenAI function-calling; key-scrubbed errors), new `[llm]`
section in `credentials.toml`, `AppState.llm_client` widened to `Arc<dyn LlmClient>`, `build_llm_client`
selects provider at boot (restart-to-apply), Settings "LLM Provider" picker + `GET/PUT /llm/config` (key
write-only — GET returns `has_key`, blank-on-save preserves). **Verified:** +22 core unit tests; full
`core`(auto-import) + `server` suites green; clippy clean across `core`/`server`/`omni-me-app`/`frontend`
(mock+default); a **live HTTP smoke** (boot selects OpenAI-compatible; CSV boots at its `schedule_secs=120`
while a live-added source lands at the global 60; live add appears in `/status` with no restart; live
delete; `/llm/config` round-trips with `has_key` + the key never returned; invalid-add→400, delete-missing
→404); a **Playwright mock walkthrough** (LLM picker reveal+save; live add → Healthy in *both* Configured
and Running; live remove; 0 console errors). PNGs in `logbook/_assets/{config-driven-sources,llm-provider}/`.
**Still deferred:** REST source + generic-`ImapSource` config wiring (3.6 tail); OpenAI-compatible vision
extractor (3.8a).

**STATUS 2026-06-19 — 3.8a + 3.9 COMPLETE (public-repo only).** Both shapes of the extensibility mechanism
now reach the *document* layer, and accounts stop being hand-maintained. **3.8a (vision extractor):** an
opt-in OpenAI-compatible `DocumentExtractor` (`[llm] vision = true`) reusing the `[llm]` config + the prompt/
schema/parse hoisted out of `gemini.rs`; `build_extractor` selects it only when opted in (default stays
Gemini/Null). **3.9 (auto-detected accounts):** the Accounts screen / net-worth roster is now **auto-derived
by type** (`Assets`/`Liabilities`/`Unmatched` seen in the ledger ∪ declared − hidden) instead of a
hand-maintained `ROSTER_FILE`; Settings became **overrides-only** (rename / Hide-Unhide), persisted via an
idempotent `AccountAdded` upsert (new `hidden` field, SET-not-CONTENT so reconcile survives); a
`known_accounts` data layer ships for the upcoming `AccountInput` typeahead. **Public-repo only** — the
defaulted/additive changes keep `omni-me-private` untouched. **Verified:** +15 core unit tests; full core
(435) + server suites green; clippy clean across core/server/app/frontend (mock+default); a Playwright mock
walkthrough proving hide drops an account off the Accounts screen + net worth, rename propagates, and the
vision toggle reveals + saves (0 console errors); PNGs in `logbook/_assets/{accounts-auto-detect,llm-provider}/`.
**Still deferred:** the `AccountInput` typeahead *component* (friction-log [M]; data layer shipped); 3.6 tail
(REST + generic-IMAP, pushed down by the user); 3.10 liquidity.

- [x] **3.1** Create private overlay crate written against `core`'s `AutoImportSource`. [M] — **done 2026-06-14**; path-deps on the public crates (pinned git-dep deferred to deploy).
- [x] **3.2** Move the bank adapters + their vendor Python driver + credential structs into the overlay; generic plumbing (`imap*.rs`, `receipts.rs`, `mime.rs`, trait) stays public. [L] — **done 2026-06-14**; public copies `git rm`'d after private verified.
- [x] **3.3** Invert source instantiation — done via the `run(RunConfig{source_builder})` seam in `server/src/lib.rs` (not literally `main.rs`); public `main.rs` = zero-sources builder. [M] — **done 2026-06-14**.
- [x] **3.4** Public app degrades gracefully to zero configured sources + zero declared accounts (no crash; manual entry / journal / budget all work). [M] — **DONE 2026-06-17.** Most rails already existed (empty source builder, drop-by-default roster, `NullExtractor`); the real gap was a **boot-time panic**: the Gemini-key resolution `.expect()`-ed a key, so a no-key / no-`credentials.toml` install crashed on startup. Now both the key resolution and the extractor's config-dir resolution degrade gracefully (empty-key client + `NullExtractor`; LLM routes error at call time, server boots). Also dropped the two bank-specific form defaults (`finances.rs` statement-import + balance-check had hard-coded a specific bank's chequing account) → neutral placeholders (`"Assets:Bank:Chequing"`). Verified: server boots with `GEMINI_API_KEY` unset + no credentials file (`/health` ok, `/auto_import/status` → `[]`).
- [x] **3.5** Subprocess-plugin runner: generic public `SubprocessSource` (command + args; helper owns creds + account-map, so "secret-ref" became "helper reads its own secrets" — a stronger boundary; schedule stays the engine's interval). The first bank adapter converted to a standalone helper binary; contract frozen in `SUBPROCESS_SOURCE_CONTRACT.md` + code types. [L] — **done 2026-06-15 (Step 2a)**; smoke-verified live. Multi-source config *registration* (declare sources via config/UI) is 3.6/3.7; CSV/REST helpers fan out from this runner.
- [x] **3.5a** Interactive source re-auth (**app-entered OTP**) per `SOURCE_REAUTH_DESIGN.md` — generic `AuthState` + status + reauth route in the **public** engine; the bank driver's login-protocol in the **private** overlay; client "Reconnect {source}" UI. Removes the SSH-to-VPS-for-OTP failure mode. **Was the hard precondition for deploying the private bank auto-import to the VPS (Phase 2) — now MET.** [M] `(logbook)` — **DONE 2026-06-17.** Server half (Step 2b, 2026-06-15): engine `AuthState`/`ReauthOutcome` + registry state-tracking + `POST /auto_import/reauth` + `auth_state`/`reauth_capable` on `GET /auto_import/status` + `SubprocessSource` reauth verb + the bank helper's `reauth` handler. Client half (Step 2c, 2026-06-17): inline "Reconnect {source}" callout + OTP field in `AutoImportRow` + `reauth_source` Tauri command, the lossy-serde seam widened at every proxy hop. Routes under `/auto_import/*` (not `/sources/*` as the design sketched). **Verified:** clippy clean (both feature configs); Playwright mock walkthrough of all 5 states (0 console errors); **real-OTP happy path E2E vs the real account** — live TOTP → `auth_state: active`, manual Fetch → `last_outcome: success`/`health: healthy` (session refreshed, not just flag cleared). `(logbook)` capture deferred to a later drafting pass; PNGs preserved in `logbook/_assets/source-reauth-reconnect/`.
- [x] **3.6** Config-driven generic sources: CSV first (+ REST / IMAP) parameterized by config. [L] — **DONE 2026-06-17 (CSV + subprocess; REST/IMAP-wiring deferred).** New server-side `sources.toml` (definitions, *not* secrets — separate from `credentials.toml` per "secrets referenced by name") + loader/`validate`/`build_generic_sources` in `core::auto_import::config`; native `core::auto_import::csv::CsvSource` (balanced `[account]`/`[Unmatched]` drafts, content-hash dedup, forgiving row-skip, header/index column mapping, configurable date format); `csv` crate feature-gated under `auto-import`. Public `server/src/main.rs` now builds from config (was `no_sources`); absent file → zero sources (3.4 holds). 15 unit tests. **Per-source `schedule_secs` DONE 2026-06-18** — honored via a defaulted `AutoImportSource::poll_interval()` (CSV/subprocess return their `schedule_secs`; `spawn_one` uses it, else global), which sidestepped the `SourceBuilder`-seam ripple entirely → no private-overlay change. **Split 2026-06-20:** REST → promoted to tracked task **3.6b** (active); generic-IMAP wiring → indefinitely deferred to the *Post-v1 / when-demanded* backlog.
- [x] **3.6b** REST config source: generic `RestSource` (HTTP GET → JSON field-map → balanced `[account]`/`[Unmatched]` drafts), API key via `credentials.toml` secret-ref; new `validate`/`build_one` arms + type-aware Settings form. Self-contained — no `build_one` signature change. [M] — split from the 3.6 tail 2026-06-20; public-completeness (the user's own REST source runs through the private overlay). **DONE 2026-06-20.** New `core::auto_import::rest::RestSource`: reqwest GET → dotted-path JSON map → balanced drafts; `pluck` dotted-path navigator (**user Learn-by-Doing** — object-keys-only via `try_fold`); `json_amount`/`json_str` coercion reusing csv's now-`pub(crate)` `parse_amount`/`stable_hash`; response-body content-hash dedup; skip-bad-record-not-fatal. **Auth = "secrets referenced by name":** new `[secrets]` map in `credentials.toml`, resolved at *fetch* time via `secret_ref` (RestSource reads its own creds → no builder-signature ripple, unlike IMAP). config `validate`/`build_one` `"rest"` arms + 6 `SourceDef` rest fields; type-aware Settings add-form gained REST (url/records_path/field-paths/auth) + `config_summary` "REST ·" line. **Verified:** 517 core + 2 new config rest tests + 7 rest-module tests, clippy clean (core auto-import / server / app / frontend mock+default, `-D warnings`), Playwright mock walkthrough (REST option → fields render → save → "REST · url → account" in list, 0 console errors); PNG `logbook/_assets/rest-source/`.
- [x] **3.7** In-app source-registration UI (Settings): add / edit / remove sources; secrets referenced by name. [M] `(logbook)` — **DONE 2026-06-17 (restart-to-apply; live add/remove deferred).** Server `GET/POST/DELETE /auto_import/sources` (file-only — they edit `sources.toml`, never the running registry, so changes apply on next restart; POST validates → 400, DELETE-missing → 404); three Tauri command proxies (untyped `serde_json::Value` — the client builds `core` without `auto-import`, so `SourceDef` isn't in scope) + bridge fns w/ stateful mock; Settings `AutoImportSection` rebuilt into **Configured sources** (Add/Edit/Remove + "pending restart" vs live-health badge) over the existing **Running now** list (reauth flow preserved). Add form is type-aware (CSV ↔ Subprocess fields swap); name is the key (locked on edit). **Live add/remove DONE 2026-06-18** — `SourceRegistry` now owns each task's `JoinHandle` + has `spawn_one`/`remove` (abort), `AppState` carries the build context (`store`/`projections`/`device_id`/`default_interval`), and the CRUD endpoints build+spawn / abort in-place → add/edit/remove apply live (no restart). UI copy + badges updated off "pending restart". PNGs in `logbook/_assets/config-driven-sources/`.
- [x] **3.8** Provider-swap: OpenAI-compatible `LlmClient` impl + Settings picker (base URL / model / key); `DocumentExtractor` on the same config rail. [M] — **TEXT-SIDE DONE 2026-06-18 (extractor deferred → 3.8a).** New `core::llm::OpenAiCompatClient` (chat/completions; `complete`/`complete_json` via `json_object`+schema-in-prompt for portability/`complete_with_tools` via OpenAI function-calling; key-scrubbed errors; 7 wiremock tests). New `[llm]` section in `credentials.toml` (`LlmProviderConfig{provider,base_url,model,api_key}`); `AppState.llm_client` widened to `Arc<dyn LlmClient>`; `build_llm_client` selects `openai_compatible` vs Gemini default at boot (restart-to-apply — the LLM is a set-once knob, unlike sources). Settings "LLM Provider" picker + `GET/PUT /llm/config` (key write-only — GET returns `has_key`, blank-on-save preserves) + Tauri proxies + stateful mock. **Deferred → 3.8a (now DONE 2026-06-19):** the OpenAI-compatible *vision* `DocumentExtractor` (separate impl; vision support varies by endpoint) — rides the same `[llm]` config behind an explicit `vision = true` opt-in.
- [x] **3.8a** OpenAI-compatible `DocumentExtractor` (vision via chat/completions) on the same `[llm]` rail; graceful degradation when the endpoint has no vision. [M] — split out of 3.8 2026-06-18. **DONE 2026-06-19 (opt-in).** New `core::extraction::openai_compat::OpenAiCompatExtractor` (vision content shape `content:[{text},{image_url:data-URI}]`; `response_format:json_object` + schema-in-prompt for portability; code-fence-tolerant parse; **images-only `supports`** — PDF excluded since most OpenAI-compatible endpoints reject raw PDF; key-scrubbed errors; 6 wiremock tests). The per-hint prompts + `response_schema` + `parse_response` were **factored out of `gemini.rs` into the `extraction` module** so both extractors share one copy. Graceful-degradation choice = **explicit `[llm] vision = true` opt-in** (default off → extractor stays Gemini/Null; never silently POSTs images to a vision-less endpoint); `build_extractor` selects it only under `provider=openai_compatible && vision && base_url+model`. Settings LLM picker gained a vision checkbox; `GET/PUT /llm/config` carry `vision`. **Verified:** +6 extractor wiremock tests + a `build_extractor`/`build_llm_client` selection test; clippy clean (core/server/app/frontend mock+default); Playwright mock walkthrough (vision toggle reveal + save). **Not separately run:** a full live-server boot against a wiremock `/chat/completions` (the wiremock extractor tests + selection test cover the logic).
- [x] **3.9** Auto-detected accounts (was: move roster into config/data + declared-accounts Settings UI). [M] — **DONE 2026-06-19 (auto-include-by-type).** Reframed per user: the account list is **auto-detected from the ledger**, not hand-maintained. New pure `core::balances` fns: `auto_roster` (Assets/Liabilities/Unmatched seen-in-journal ∪ declared − hidden → the Accounts-screen allowlist; net worth stays correct because only balance-bearing types are summed), `known_accounts` (full set, all types + ancestor segments → the autocomplete data layer), `account_type`. The hand-maintained `ROSTER_FILE` is retired to an optional balance-bearing extra-include (zero regression). Overrides (rename/hide) reuse `AccountAdded` as an idempotent **`UPSERT … SET`** (new `hidden` field on the payload + `accounts` projection + `AccountRow`; SET-not-CONTENT preserves reconcile state). New Tauri commands `list_known_accounts` / `list_detected_accounts` / `set_account_override`; Settings **Accounts** section (rename + Hide/Unhide, overrides-only). **Verified:** +9 core unit tests (incl. SET-preserves-reconcile); clippy clean across the board; **Playwright mock walkthrough proved it end-to-end** — Settings lists 5 detected accounts; hiding Meridian:AED flips it to Unhide AND drops it from the Finances Accounts screen + net worth; renaming Globepay→"Globepay (everyday)" propagates to the Accounts screen; 0 console errors. **Next (immediate follow-on):** the shared `AccountInput` typeahead component (friction-log [M]) consumes `list_known_accounts` (data layer shipped here; `invoke_list_known_accounts` is wired + `#[allow(dead_code)]` until the component lands).
- [x] **3.10** Liquidity-aware `can_i_afford` (per-account `is_liquid` flag drives the verdict; same accounts table; `AffordVerdict.policy_label` → "Liquid assets − next month's recurring"). [S] — **DONE 2026-06-20.** Opt-in `is_liquid` bool threaded payload→projection→`AccountRow`→`AccountSummary`; `sum_liquid_assets` (`None`=nothing marked → net-worth fallback, `Some(0)`=marked-but-empty → can't-afford); `can_i_afford` rewritten (liquid pool when marked, net-worth fallback else, early-return when neither pool — user Learn-by-Doing, incl. the match-with-early-return fix); Settings "Mark Liquid" toggle + badge; `type_complexity` alias on the mock override store. **Verified:** 438 core + 12 server tests, clippy clean ×4 configs (`-D warnings`), Playwright verdict flip (Yes→No on marking Globepay:CAD liquid) + revert, 0 console errors; PNGs `logbook/_assets/liquidity-afford/`.
- [x] **3.11** Synthetic-fixtures discipline: adopt before any parser work against real data. [XS] — **DONE 2026-06-20.** Audit clean: only `extraction/README.md` is committed under `tests/fixtures/`; `.gitignore` covers `.reference/`+`surreal_data/`+`blobs/`; no committed journals/statements/DBs; the lone committed receipt image (`tauri-app/frontend/src/mocks/receipt-loblaws.png`) confirmed **synthetic** by user. Discipline now written down as a committed convention in `core/tests/README.md` (synthetic/inline = committed; real data → gitignored `.reference/` behind `#[ignore]` skip-graceful tests; enforced by mechanism not vigilance). **Correction:** `.reference/paisa/` is the *real* hledger journal (~5,826 txns) — the Phase 4 import source — so the *data* stays gitignored-but-present, **not** deletable. **Refinement (user challenge — don't keep permanently-ignored tests):** deleted `core/tests/journal_import_paisa.rs`; its two `#[ignore]` tests gave zero CI coverage and every path they touched (parse_journal file-walk/include-glob/elision/error-collection/per-account stats; A2 rewriter) is already covered by synthetic `TempDir` CI tests in `journal_import.rs` — their only value was a one-time pre-cleanup scale validation (POC 0.1b, done). `core/tests/README.md` now states the principle: synthetic-runs-in-CI is the coverage; `#[ignore]` is only for un-synthesizable real-resource diagnostics (e.g. real-Gemini `extraction_integration`), never a home for deterministic-logic coverage.
- [x] **3.12** mylearnbase follow-up: re-shoot Accounts screenshot generic + update alt text. [S] — **DONE 2026-06-21**, superseded by the full public-identity sanitization (all 7 omni-me logbook posts re-shot from the sanitized mock + prose/alt/citation cleanup; see `project.md` session log).
- [x] **3.13** Verify: clean clone builds + runs zero-config; overlay build pulls real sources; BYO-LLM points at an alternate endpoint and works. [S] — **DONE 2026-06-20 (residuals accepted by user).** (1) **Public zero-config boot live-verified:** empty `XDG_CONFIG_HOME` + no key → `/health` ok, `/auto_import/status` `[]`, `sources=0`, NullExtractor fallback, no panic. (2) **Overlay `cargo check` clean against the post-3.10 engine** — proves 3.10's additive (`#[serde(default)]`) changes didn't break the composition root. (3) **BYO-LLM via this session's green tests** — 7 `OpenAiCompatClient` wiremock tests (mock `/chat/completions`: complete/json/tool-calls/error/rate-limit/key-not-leaked) + `build_llm_client_selects_openai_compatible_text` boot-selection. **Residuals (accepted, address if they bite):** overlay pulling *real* bank sources at runtime = user-owned (live-verified earlier in 3.5/3.5a); no single full server↔mock-LLM e2e boot (client+selection tests cover the logic, same as 3.8a's note). **Also:** removed stale pre-split public-server data (`surreal_data/`+`blobs/` at repo root).

## Phase 4 — Real-data go-live import (Cycle-3 6.5) *(PULLED AHEAD OF Phase 2 — 2026-06-21)*

**Sequencing (2026-06-21):** done **before** Phase 2 deploy — real data needs to be in hand to push
while testing the deployment.

**Placement (2026-06-21) — Phase 4 needs no repo changes:** the import path
(`core::journal_import` + the projections + R2 query + base-currency setting) is generic and already
**public**; it imports *any* hledger journal. The real journal, the resulting SurrealDB, and
`credentials.toml` are **gitignored — they live only on the work machine, in neither repo** (no leak).
So Phase 4 is a **local operation** run on the machine that holds the cleaned journal (the
data-cleanup machine), not a code change. Only if cleanup surfaced *roster-specific* import/rewrite
rules would a small piece land in the private overlay; the generic A2 rewriter stays public.
**Phase 2's split is already homed** (see the Phase 2 note): the "deploy to my box" pipeline + the
go-live image (which runs the *private* overlay binary) live private; the public repo keeps
build/test/publish of the bank-free image.

- [x] **4.1** Import the cleaned journal end-to-end — event emission → SurrealDB + journal-file projection round-trip (the part 6.4 stopped short of). Ends the cheap-breaking-changes window. [M] — **DONE 2026-06-22.** Source is **ledger-cli format (not hledger** — corrects the planning note); needed generic public-engine parser fixes first (cost-faithful elided balancing, transaction/header-tag capture via a defaulted `TransactionRecordedPayload.tags`, `P`-directive strip, status-marker normalization, digit-commodity quoting, `raw_balances` zero-cost fallback). Headless one-shot runner + balance/probe validators + a re-import workflow README live in **`omni-me-private/examples/`** (real anchor data → private repo, not public). Idempotent for txns (content-hash `txn_id`); journal notes mint ULIDs → re-run needs a fresh DB. A2 rewriter OFF.
- [x] **4.2** Validate projected balances vs the source journal; dashboard/accounts reflect real data. [S] — **DONE 2026-06-22.** App-projected per-account/per-commodity balances reproduce `ledger -f main.ledger bal` **exactly** (full path: events → `budget.journal` projection → `core::ledger::balances`), via temp-DB dry-run *and* the real app DB. Journal/notes counts reconcile (1:1 across journal + generic + skipped templates).
- [x] **4.3** Exercise the deferred Cycle-3 real-DB paths now that real data flows: R2 query (7.2) + base-currency setting (7.3) against the live SurrealDB. [S] — **DONE 2026-06-22.** R2 `core::query` runs over the live transaction set (tag/account/commodity predicates); CONVENTIONS §8 tag-filtered anchors reproduce **exactly** through the persisted posting tags (institutions-in-tags confirmed). base-currency conversion exercised: book value via `ledger-utils` implied prices from the 2-commodity cost legs; only commodities that never traded directly against base drop (no conversion chaining) — ~0.3% vs `ledger -X CAD`, an app valuation-engine trait, not an import defect.

**Phase 4 GUI-VALIDATED & CLOSED 2026-06-22.** Ran the actual desktop app against the real embedded DB — built standalone via the new `tauri-app/scripts/desktop-build.sh` (desktop analog of `android-build.sh`; `cargo tauri build --debug` with a `--config` override embedding the **release** frontend, because a `cargo tauri dev` binary run standalone serves the UI from a dead localhost dev server → blank page) — and confirmed the import end-to-end in the GUI: net worth + per-account/per-commodity balances match the `ledger bal` oracle (cumulative reconciliation, 0 diffs), the journal calendar is fully populated, the Accounts screen renders. Full DB rebuild flow (move-aside backup → temp-DB dry-run → oracle reconcile → real import → `probe_realdb`) absorbed a regenerated corpus (journal import isn't idempotent). **Three generic UI bugs found + fixed (synthetic tests, commit-safe):** (1) any per-account override (liquid/hide/rename) appended an `account` directive the Rust `ledger-utils` parser rejects, collapsing every balance view → `core::ledger::prep_content` now strips `account` blocks (+ test `balances_skips_account_directives`). (2) base-currency money rendered 24-digit decimals → rounded to 2 dp at the `commands::budget` view boundary; native precision preserved. (3) the journal editor hung on "Initializing editor environment…" → unified the `cfg(debug_assertions)`-split loader to one polling path (the release-only `onload` path never resolved in an embedded webview, and was never exercised since Android only ran the debug/polling path). **Deferred → post-launch fix cycle (see Carried backlog):** per-institution tag-breakdown drill-down on the Accounts view; balance-cache perf; JournalFile account-directive append-dedup.

## Phase 5 — Editor feel + properties *(partly dogfooding-driven)* `(logbook)`

- [x] **5.1** Inline properties panel (decision B) above the body; typed widgets for date / tags / 3 reflection keys; raw escape hatch for legacy props. **DONE 2026-07-03 (journal)** — see the "Typed properties panel" friction-log entry for the full write-up; generic-notes panel split off as its own item. [L]
- [x] **5.2** YAML↔form model kept in sync with the editor; the form emits parser-safe YAML. **DONE 2026-07-03** — `note_frontmatter::{split_journal, serialize_journal}`, pure-client + safe-by-construction (a strict subset of what `is_complete`/`parse_markdown` accept); 6 round-trip/shape unit tests. [M]
- [x] **5.3** Harden `is_complete` (`core/src/events/notes_projection.rs:282`) to accept block lists / reordering / blank lines (also helps Obsidian-import compat). **DONE 2026-07-03** — see the "Harden `is_complete`" friction-log entry for the write-up. [S]
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

- [x] **6.1** App logo — **DONE 2026-07-04.** Multi-round collaborative design (25-candidate parallel-subagent brainstorm across wordmark/greek/abstract → converged on an **"enso" open-brush ring** in accent `#448aff` on charcoal `#1e1e1e` with a small off-white "me" core). Identity system: the enso **"o" alone = app icon**; **"om" = compact wordmark** (o + a clean gateway "m"). Locked the **light** weight (rounded brush terminals + fattened thin end so it survives 32px/24px/circle-crop). Canonical source in `tauri-app/branding/` (`omni-me-icon.svg` full-bleed, `omni-me-fg.svg` transparent mark, `omni-me-bg.svg` charcoal, `omni-me-om.svg` wordmark, `generate-logo.py`, `logo-manifest.json`). Regenerated the whole cross-platform set via `cargo tauri icon .../branding/logo-manifest.json` — desktop (icns/ico/pngs), iOS AppIcons, Android legacy + **adaptive** (charcoal-bg PNG + transparent-mark fg, fixing the default `#fff` halo). Verified desktop 128px + both Android adaptive layers. Exploration history archived (now tracked) at `.archive/logo-design/`. Optional follow-up: full horizontal `omni·me` lockup. [S]
- [ ] **6.2** Branch-gate workflow: feature branches + merge gates to protect stable. [S]
- [ ] **6.3** v1 semver stamp + git tag. [XS]
- [ ] **6.4** Archive + reset `project.md` (session log + status history → `.archive/`, leaving a lean current-state doc) once stable-v1 ships — it's grown unwieldy carrying every session's detail. Consider the same for `tasks.md`. Tie to the v1 tag so the archived snapshot is a clean cut point. [S]

---

## Running friction log *(fill during dogfooding; triage into the live phase)*

### 2026-07-04 — cross-platform dogfooding (phone + laptop, both on the Hetzner box) — NEW, list incomplete ("...and it goes on")

**CRITICAL — sync / data integrity:**
- [ ] **Auto-sync never fires.** Edits save locally but don't propagate until the **Sync** button is pressed manually, despite both devices connected to the Hetzner server. Applies to journal, notes, tasks — everything. (dogfooding 2026-07-04) [?]
- [ ] **Content/body edits don't materialize on the receiving device.** Manual sync of a journal entry shows **"1 up"** on the sender and **"1 down"** on the receiver, but the received entry stays **BLANK**. Same for **note bodies** — a note's *creation* syncs (the empty note appears on the other device) but typed **text** does not. So *creation* events and *task create/complete* propagate, but **text-content updates don't apply**. Likely a projection/event-apply gap for body-content events (journal continuity / note body), not the transport (counts move). **Headline bug — breaks the multi-device premise.** [L?] — **CODE-FIXED for notes + journal (commit `d049f1e`, Session 6):** `on_journal_updated` / `on_generic_updated` now **UPSERT** so a body edit materializes even when this device never saw the create (lost to an old batch-abort). **Pending on-device confirm** (pairs with 305 + the on-device sync pass). *Not yet checked off — awaiting real-device verification.*
- [x] **Imported journal + note data absent on mobile.** None of the pre-existing imported journal/notes are on the phone — only (partially) newly-created items sync. No initial/bulk backfill to a fresh device. [?] — **✅ ROOT-CAUSED + FIXED + box re-seeded 2026-07-05.** Real root cause (NOT a backfill gap): the headless importer stamped every event with a **phantom `device_id = "headless-import"`**, but the sync client pushes **only its own device's events** (`get_since_by_device`, correct anti-re-push-storm behaviour). So the desktop's full imported ledger (~10k txns) was **stranded in local.db, unpushable** — the box only ever held ~125 events, so a fresh phone had nothing to pull. Fixes: (a) `headless_import` device_id is now **env-driven** (`OMNI_DEVICE_ID`) — import stamps the desktop's real id; (b) importer journals are now **date-keyed** (matches the in-app grammar / the bug-1 cutover, no ULID drift); (c) **clean wipe + re-import + seeding push** executed — box wiped (reversible snapshot), local.db re-imported under the real device_id (10207 txns / 1294 journal / 343 notes, **reconcile byte-faithful to `ledger bal`** across all 106 accounts), then pushed. **Box now holds 11844 events; a pull from a fresh device id returns all 11844 → any new device backfills the full real ledger (server side proven).** Headless seeding runner: `omni-me-private/examples/push_local.rs`. **Remaining: on-device phone confirm** (pull+apply on the actual Samsung — pairs with the deferred frontend live-refresh @315).
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
- [x] **Journal stuck on "Loading…" on cold first-open** (reopen fixes it). **✅ RESOLVED 2026-07-05 — see resolution bullet below.** (History kept; both earlier hypotheses were wrong.) **DEEPER THAN FIRST THOUGHT — links to sync backfill; carry into the sync session.** Reproduced precisely via uninstall→reinstall→first-open (the *only* way — a warm relaunch hides it): the journal hangs on the **data-fetch** "Loading…" state indefinitely (2min+ observed), a relaunch loads fully in ~8s. NOT the editor-init poll ("Initializing editor environment…" is a separate indicator; that fix is unrelated).
  - **Attempt 1 (2026-07-05, INSUFFICIENT — retest still hung):** hardened `continuity.rs` `use_continuity_provider` boot read, which set `loaded=true` only on the `Ok` arm of `invoke_get_workspace()` — so if that invoke returns `Err`, every `loaded_peek` waiter (journal `while !store.loaded_peek()`, `main.rs` tab-restore) strands on "Loading…". Fix = retry the boot read (poll idiom, ~10s cap) + **fail open** (`loaded=true` regardless) + split a `load_succeeded` signal to keep the debounced write-back from clobbering a good workspace file on a failed read. **This is valid defensive hardening and worth keeping**, BUT on-device retest (uninstall→install→cold-open) **still hung 2min+** → it was NOT the active cause.
  - **Corrected root cause (why attempt 1 missed):** on a *fresh* install `get_workspace` reads a **file**; a missing file returns `Ok("")` (backend handles `NotFound`), so `loaded` *does* flip — the continuity race only bites on a true `Err`. The real blocker is downstream: **`invoke_get_journal_by_date` (a DB read) blocks during the fresh-install initial sync backfill.** Empty local DB ⇒ the sync engines (spawned in `lib.rs` setup before/around `manage`) pull a full backfill that saturates/locks the DB, so the first journal DB read never returns → `loading` stays true. Reopen works because the backfill already completed (DB populated, cursor advanced) ⇒ the read is fast. Ties directly to the **sync subsystem** bugs above (backfill volume + fail-fast apply). **Fold into the dedicated sync session** (fix backfill so it can't block first-open reads, and/or make the journal fetch time-out+retry / render the template while the read is pending). Backend `tracing` goes to stdout, not logcat, so on-device backend introspection needs a stdout→logcat bridge first. **V1 GATE: must be fixed before finalizing the app (user, 2026-07-05) — no band-aid, wait for the real fix.** [M, → sync session]
  - **✅ RESOLVED — TRUE root cause found on-device (2026-07-05); BOTH earlier hypotheses (WebView cold-load @316/319; DB-backfill block @ corrected-root-cause above) were wrong.** Cracked with **Chrome DevTools over adb** (`adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>` → CDP `Runtime.evaluate` against the stuck page). Findings while hung: the shell + WASM actually **render in ~85ms** (every asset 200 OK), IPC round-trips in ~4–44ms when fired manually, process sat at **0% CPU** — so it was neither a WebView/WASM cold-load nor a DB backfill; `get_journal_by_date` was **never even called**, and backend logcat (`RustStdoutStderr` tag *does* carry backend tracing on Android — the "stdout→logcat bridge" note above was moot) showed **no `get_workspace` checkpoint either**. Real bug: the continuity boot read fires `get_workspace` **before Tauri's native IPC handler is ready on cold first-open, and that invoke is silently *dropped* — its promise never resolves *or* rejects.** The retry loop only advanced on `Err`, so it parked on the first `.await` forever → `loaded` never flipped → every `loaded_peek` waiter (journal fetch, tab-restore) hung. Attempt-1's retry-on-`Err` hardening (kept) can't cover a promise that never settles. **Fix (frontend):** new `bridge::invoke_get_workspace_timed(ms)` races each attempt against a `setTimeout` (JS `Promise.race`) so a dropped invoke fails like an `Err` and is retried; `continuity.rs` boot loop uses it with a 500 ms per-attempt timeout + 15 s wall-clock fail-open cap. **Verified on-device, two genuine fresh installs (uninstall→install→cold open):** `get_workspace` lands ~900 ms (dropped attempt → 500 ms timeout → retry succeeds once IPC ready), journal renders by ~935 ms → **tap→journal ~1.4 s** (was ∞); editor shows the template, no "Loading…". `startup_probe` diagnostic scaffolding **removed** (`lib.rs`/`commands/notes.rs`/`commands/workspace.rs` + `startup_probe.rs` deleted). Backend clippy + 38 lib tests green; frontend clippy clean both mock+default. **V1 GATE cleared.** (Independent of the deferred frontend live-refresh + sync-backfill items above — those remain open.)
- [x] **Date-picker bricks the desktop app.** Setting the date freezes the whole app until you click out to a different app (focus/modal trap). **DONE 2026-07-05 (Option A + validation, user's call).** Replaced all 5 native `<input type="date">` in `finances.rs` with a new shared `components/date_field.rs::DateField` — plain `YYYY-MM-DD` text entry (no native GTK picker → no freeze) with inline validation: pure `is_valid_date_str` (empty ok; else must parse as a real `NaiveDate`, so `2026-13-40`/`2026-02-30`/wrong-separators/incomplete are rejected) drives a red ring + "Use YYYY-MM-DD" hint. Verified: 3 unit tests pass; clippy clean both configs; **live at 1280px** — field is `type=text` (Playwright `anyNativeDateInput:false` across the app), invalid input → ring+hint, valid → cleared. Desktop-freeze itself to be confirmed in the user's pass (native modal is simply gone). **ROOT CAUSE (2026-07-05):** the 5 native `<input type="date">` in `finances.rs` (1380, 2719, 2733, 3333, 5445) open the **webkit2gtk/wry native date-picker popup**, a known Linux embedded-webview issue where the GTK popup grabs focus and doesn't release it to the wry window. Settings already avoids this (its date fields are plain `type=text` with a `%Y-%m-%d` placeholder). **Two fix options (awaiting user call):** (A) quick [S] — make the 5 finances dates plain text `YYYY-MM-DD` (consistent w/ Settings), kills the freeze now, loses the calendar-popup affordance; (B) premium [M, own session] — build a custom in-DOM date picker reusing the journal month-grid (`journal.rs` ~900-1000), keeps click-to-pick, browser-verifiable, but a cross-cutting shared-component build → its own planning-first session per the defer-major-phases rule. [M?]

**Quick / mobile:**
- [x] **Mobile Notes: Save button fully offscreen** (layout overflow on the notes section, mobile). **DONE 2026-07-05.** Root cause verified live at 390px: the note-editor header's `flex-1` title `<input>` had the flex default `min-width:auto`, so it couldn't shrink below its ~260px content width → the header overflowed by ~96px → the Save button was pushed to x=403 (viewport 390), fully offscreen. Fix: added `min-w-0` to the input (lets flex shrink it); re-measured with the fix injected → header no longer overflows, Save at right=374 (onscreen). Clippy clean both configs. [S]
- [x] **Remove the "AI Analyze" button** (user decision, 2026-07-04). **DONE 2026-07-05** — removed the visible UI (button + `LlmResultsDisplay` results/error panels + orphaned `processing`/`llm_result`/`llm_error` signals in `journal.rs`). Per user clarification, the backend `process_note_llm` effect + the `invoke_process_note_llm` bridge wrapper + `LlmResult` type are **retained** (`#[allow(dead_code)]`) for a future non-button analysis trigger. Frontend clippy clean on both mock + default configs. [XS]
- [x] **Account suggestions don't auto-load on mobile** (typeahead doesn't populate automatically on the phone). **DONE 2026-07-05.** Verified live at 390px: the known-account list *does* load (typing "A" surfaced 4 matches), but focusing an empty field showed **0** suggestions — `rank_suggestions` returned `Vec::new()` for an empty query, so nothing appeared until a keystroke (reads as "doesn't auto-load," worse on mobile where typing is costly). Fix: empty query now returns the top-`MAX_SUGGESTIONS` known accounts, so the dropdown populates on focus (tap-don't-type). Clippy clean. **NOTE:** on a real device with sparse synced data the list may still be thin until the sync-backfill lands — the on-focus behavior is the frontend half. **On-device CONFIRMED 2026-07-05:** phone shows nothing on focus *and* while typing → its known-accounts list is **empty** (no accounts backfilled to the device). So the frontend fix is correct; on-device visibility is blocked on the **sync backfill** (deferred, v1 gate). Desktop, which has the data, works. [S]

**2026-07-05 confirmation-pass — new items (user's pass over the pushed build):**
- [x] **Desktop control contrast unreadable** in Settings — specifically the **default-currency picker** and the **LLM-provider selector** (checkbox/select controls). Low contrast against the dark theme; hard to read state. *User has 2 screenshots.* [S] — **DONE 2026-07-05 (c7215d1).** Root cause: no `color-scheme` was set, so Linux webkit2gtk painted native form controls (both `<select>`s + their dropdown popups, the vision `<checkbox>`, scrollbars) with the **light GTK system theme** over the app's dark bg → unreadable. Fix = one line: `color-scheme: dark` on `:root` in `input.css` — the spec-standard way to tell a WebView to render native controls in dark mode (app-wide, covers every native control, no control-replacement needed; same native-control family as the date-picker freeze). **Verified** via `dx serve` + Playwright at 1280px: both selects now render dark bg + light text + visible arrow (Chromium honors `color-scheme` identically to webkit2gtk, so the flip is faithful). Final webkit2gtk contrast confirmation rides the user's desktop pass (Chromium can't render the native GTK popup itself). **REOPENED 2026-07-06 (on-device pass):** the parenthetical assumption above was WRONG — webkit2gtk does **not** honor `color-scheme: dark` for native `<select>`s; the **closed picker box** stayed unreadable on the real desktop (user confirmed "closed picker box"). c7215d1's Playwright "verified" was Chromium-only theater for this exact control. **Real fix:** a global `select { -webkit-appearance:none; appearance:none; background-color; color; + chevron }` rule in `input.css` — webkit2gtk only honors CSS bg/color on a `<select>` once the native appearance is cleared. Kept `color-scheme: dark` for the other native controls (date picker, checkbox, scrollbars). This is **not** Playwright-verifiable (Chromium already rendered it fine) → pending the user's on-device confirm before re-closing. **RE-CLOSED 2026-07-06 (on-device confirmed):** user verified on the webkit2gtk desktop — both closed picker boxes now render dark bg + light readable text; the initial chevron/value overlap on the auto-width currency picker was fixed with `padding-right:2rem !important` (beats utility `px-*`). Shipped in the release-tier `desktop-build.sh release` binary. Fix lives in `input.css` (global `select` appearance:none rule). Lesson banked to the dx-workflow memory: native-control styling is not Playwright-verifiable — webkit2gtk ≠ Chromium for form controls.
- [ ] **IMAP pollers fail — Gmail and Yahoo, for different reasons.** Both email pollers error (the two failure modes differ per provider — likely app-password / OAuth / IMAP-enabled / host+port). *User has a screenshot.* Diagnose per provider. [?]
- **[private/live build — record] Default sync `server_url` → `http://omni-box-hetzner:3000`, not `localhost`.** The default the app ships with (on a fresh/empty settings DB) is localhost, so every reinstall the user re-points it by hand. Fine for debug copies, but the **personal/live build** (compiled + auto-updated on the server) should default to the box address so reinstalls need no manual reconfig. NOTE: `server_url` is a **persisted setting** (the desktop smoke-run read `omni-box-hetzner` from the *existing* local DB — that's the user's prior manual value, not the fresh-install default). Fix likely rides the private overlay's build-time `--config` injection (same channel as the OTA endpoint/minisign pubkey) or the hardcoded default const. (user, 2026-07-05) [private/config, S] — **DONE 2026-07-21 (fix implemented; on-device confirm pending).** Chosen shape = **public exposes a compile-time knob, private CI sets it**: `DEFAULT_SERVER_URL` (`tauri-app/src-tauri/src/lib.rs`) is now `option_env!("OMNI_DEFAULT_SERVER_URL")` (unset → `localhost` so the public zero-config build is unchanged; no box hostname in the public repo) + a `cargo:rerun-if-env-changed` in `build.rs`. Precedence preserved: persisted `server_url` file > runtime `OMNI_SERVER_URL` env > compile-time default > localhost. Private `app-release.yml` sets `OMNI_DEFAULT_SERVER_URL=http://<box>:3000` from `vars.HETZNER_BOX_HOST` in **both** build jobs (desktop extends the existing resolve step; android gets a mirror "Resolve box URL" step — env passes through to the nested cargo compile, so `android-build.sh` is untouched). **Bonus:** also fixes Android OTA discovery on a fresh install (`commands/update.rs:61` builds `{server_url}/updates/android/latest.json`). Verified: standalone rustc proves the const flows the env (unset→localhost, set→box); `cargo clippy -p omni-me-app` clean; `app-release.yml` YAML valid; public diff denylist-clean. **Remaining:** a billed `app-release` CI dispatch + on-device fresh-install confirm ([USER]).

**FEATURE — IMAP email-parsing pollers, proper setup (own session, user 2026-07-05):**
- [ ] **Set up the Gmail + Yahoo IMAP pollers to actually parse the user's emails** for the relevant transaction/receipt data. User estimates this needs **almost an entire session to itself** (per-provider auth, IMAP config, parsing rules, mapping to import drafts). Its own planning-first session — pairs with the IMAP-pollers-fail item above (fix connectivity first, then build the parsing). [L, → own session]

**FEATURE — journal line timestamps, redesign (design-first → own session, user 2026-07-05):**
- [x] **Reveal-on-select line completion timestamps** (Teams-style). Supersedes the current inline-timestamp feature. [M/L] — **✅ BUILT + verified (2026-08-24, ef8a1df + journal-count fix).**
  - **Today's behavior** (`tauri-app/assets/js/editor.js:334-366`, "1.3 journal-mode line timestamp on Enter"): journal mode has an Enter keymap (`timestampEnterHandler`) that, on Enter at end of line, inserts `"\n" + HH:MM + " "` — i.e. it prepends the **start time** of the *next* line as **literal inline text** in `raw_text`. `currentTimestamp()` uses local `getHours()/getMinutes()` → **24H, no timezone**.
  - **Bug the user hit:** the timestamp only engages **on the first carriage return**, so the very first line of a fresh entry is un-timestamped — you must remember to press Enter first before typing. Can't just open a journal and start writing. (This bug is **not worth patching** on its own — the redesign below replaces the whole mechanism.)
  - **User's redesign vision:** record the timestamp when a line is **finished** (not started); **don't** clutter the surface with inline times; **reveal** a line's completion time only when that (already-completed) line is **selected** — like MS Teams chat, where message timestamps stay hidden until you tap a message. **Display format (user, 2026-08-12):** 12-hour **AM/PM with timezone** (e.g. `2:32 PM EDT`), not today's 24H `HH:MM`.
  - **Hard design questions for the dedicated session** (render real candidates per the design-render habit):
    1. **When is a line "finished"?** caret leaves the line (Enter to next / click away / blur)? debounced idle? This defines when the timestamp is stamped and frozen.
    2. **Where do per-line timestamps persist?** They must move **out of `raw_text`** into side metadata, but lines get edited/reordered/deleted in a plaintext, event-sourced journal — keying by line index is fragile; by content-hash breaks on edit. Needs a real model (CodeMirror line-mapped metadata + a stored map, folded into the journal event/projection). Interacts with the sync/event model.
    3. **Reveal UX candidates:** right-aligned muted time on the active line · gutter marker · hover/tap tooltip · status-bar "line completed at …". Mobile (tap) vs desktop (caret/hover) both need to feel right.
  - **DECISION (user, 2026-08-12): build Option A first, then re-evaluate Option B by observed risk/clunkiness.**
    - **Option A (chosen first) — conceal-in-editor, keep storage as-is:** leave the time in `raw_text` but render it **hidden** via a CodeMirror decoration, and **reveal** it (reformatted to 12H+tz) on the active line / hover / tap. This **sidesteps hard design-question #2 entirely for v1** — the timestamp stays plain text, so persistence, sync, and Obsidian export are unchanged; no fragile out-of-`raw_text` line-metadata model needed. *Trade-offs that gate the A→B re-evaluation:* (a) raw markdown still carries the prefix, so Obsidian/export still shows it; (b) correct **historical** timezone display needs the stored token to carry a tz offset — local `HH:MM` alone can't reconstruct it, so A must either upgrade the stamped token to include tz/offset or accept displaying the *current* tz. Design-questions #1 (when a line is "finished") and #3 (reveal UX) still apply to A.
    - **Option B (fallback if A is clunky/limited):** true side metadata (design-question #2) — clean raw text + full tz fidelity, but needs stable per-line identity + a stored map folded into the journal event/projection model. Bigger, cross-cutting build.
  - Deferred to its own planning-first session (cross-cutting: editor JS + frontend + journal event/projection model). Relates to the typing-feel bucket (5.4). **Sequencing: after the in-flight on-device sync/perf pass.**
  - **DESIGN LOCKED (user, 2026-08-23 — dedicated session, candidates artifact rendered):** reveal style = **inline right-aligned** on the active line (user rejected gutter/tooltip/status-bar after testing the artifact); **scope = journal only** for v1 (where the user cares); **Option A** (conceal-in-text) first, re-judge B after use. **Edit handling = FREEZE at first finish** (typo fixes never move the time; only lines *touched this session* get stamped → opening an old entry never back-stamps it). **Cross-day = token stores full date+time+tz** (`⟦2026-08-24 07:12 EDT⟧`, concealed line prefix): reveal shows bare `7:12 AM EDT` when the finish date == the entry's date, and date-qualified `Aug 24 · 7:12 AM EDT` when it differs (handles "closed the entry next morning / days later"). **Stamp trigger = caret leaves a changed line, or blur/teardown** (so a line written last night keeps last-night's time even if the app closes on it). Retire the old Enter-prefix `timestampEnterHandler`. All editor-layer (no Rust projection/sync change under Option A): `editor.js` (conceal ViewPlugin + active-line reveal widget + stamp-on-leave), `components/editor.rs` (+`entry_date` prop → `options.entryDate`), `pages/journal.rs` (pass the day's date). Old entries' literal `HH:MM ` prefixes stay as visible text (not migrated).
  - **✅ BUILT (2026-08-24):** `editor.js` — concealed line-prefix token `⟦YYYY-MM-DD HH:MM TZ⟧` (math brackets, tz via `Intl` at stamp); conceal ViewPlugin + `atomicRanges` (caret skips the hidden token) + active-line float-right reveal widget; stamp = caret leaves a line the user **authored this session** (marked by CARET line, not the raw changed range — so a plain Enter at the end of a pre-existing line never back-stamps it) + blur/teardown flush for the final line; **freeze** = a line with a token is never re-stamped; `suppressDirty`-guarded so programmatic load / live-refresh never marks lines touched. Old Enter-prefix `timestampEnterHandler` retired. Pure `window.formatRevealTime(token, entryDate)` exposed. `journal.rs` `body_stats` now strips tokens so the word/char counter ignores hidden metadata (+3 unit tests). **Verified:** isolated Playwright harness (stamp/conceal/reveal/freeze/blur/cross-day/no-back-stamp on both initial-content & setEditorContent loads) + real Dioxus journal via `dx serve` mock (the full Rust→JS bridge; entryDate reached JS) + on-device Android WebView health check via CDP (bundle loaded, formatter same-day/cross-day correct, editor mounted, no crash). Did NOT type into the user's real synced journal — final hands-on stamp/feel pass is theirs.
  - **⚠️ Option-A trade-off for the A→B call (surfaced while building):** the concealed token lives in `raw_text`, so **every** raw-body consumer must strip it to stay clean. Handled here: the editor (conceal) + the word/char counter. **Still leak the raw token** (each an Option-B motivation if they matter): Obsidian/markdown export, LLM context, search, and any future preview. Under Option B (side metadata) none of these would need per-consumer stripping.
  - **✅ FOLLOW-UP FIX (2026-08-24, ce6e57f) — final line unstamped on app close.** User on-device: write a line, finish it, DON'T press Enter, close+reopen → the line had no timestamp (a new line below it did). Root cause: `blur`/`destroyEditor` don't fire when Android backgrounds / OS-kills a swiped-away app, so the debounced autosave persisted the final line WITHOUT its token; on reopen it's old untouched content that (by design) never back-stamps. Fix: on `visibilitychange`→hidden / `pagehide`, stamp the in-progress line and save it immediately (not via the 1s debounce). New `window.flushEditorTimestamps()` (editor.js) + `js_flush_editor_timestamps` bridge; `journal.rs` DayView registers the leave-detector (per-mount, removed on unmount) that nudges a channel → in-scope `spawn` drain flushes + saves (a direct signal write from the JS callback panics — same reason as main.rs's sync:applied bridge). **Verified:** `visibilitychange`→hidden CONFIRMED to fire on real Android background via CDP probe (the original `blur` never did) + full handler stamps the never-left line on `pagehide` in `dx serve` (real Dioxus, no scope panic, idempotent, no back-stamp). Needs `web-sys` `EventTarget`+`Event` features. **✅ USER-CONFIRMED ON-DEVICE (2026-08-24): the exact repro now passes — line finished without Enter, app closed+reopened, timestamp present and visible.**
  - **FUTURE IDEA (user, 2026-08-24, stashed — NOT now):** a "reveal all timestamps" mode (show every line's completion time at once, vs the default reveal-on-select). Long-term nice-to-have.
- [ ] **On-device sync feels laggy + the open view doesn't always live-refresh** (on-device test, 2026-08-14). Auto-sync **works both directions without the manual button** — validated live: a fresh phone backfilled all **12,643** events (device audit `total=12643, ever_synced=true`), and edits propagate desktop↔phone on their own. Two UX gaps surfaced: **(1) latency** — inbound edits take up to ~20s: the server has no push channel so receivers POLL (`core/src/sync/puller.rs:27` `DEFAULT_PULL_INTERVAL=20s` + 4s warmup; a network-online accelerator nudge exists but steady-state is the 20s poll). Tunable (shorter interval = more requests) or add a push/SSE channel (bigger build). **(2) live-refresh gap** — after an auto-pull applies, the backend emits `sync:applied` (`tauri-app/src-tauri/src/lib.rs:389-402`, only when `pulled>0`) → frontend bumps `sync_epoch` → subscribed views (journal/notes/routines/finances all read it) refetch; BUT the currently-open view **sometimes** stays stale until you navigate away+back (forcing a remount refetch). Suspects: the `sync:applied` nudge not reliably reaching the specific open component, or the editor **dirty-protect** (from `aa41789`, prevents live-clobber-while-typing) over-suppressing the body refresh even when not actually dirty. Needs frontend diagnosis. **Both are polish, NOT blockers — core sync + the 306/308 fixes are validated on-device.** [M, frontend]
### On-device test findings — batch 2 (2026-08-14, finances/UX pass on a Samsung S9 + desktop)
_Positives confirmed: **ledger snappy after first load** (stale-while-revalidate read-cache working), **finances UI reads much nicer** (design system). The below are the issues surfaced._

**On-device confirmation pass 2026-08-23 (user, S9, debug APK w/ `OMNI_DEFAULT_SERVER_URL`=box):** ✅ date entry (calendar popover) · ✅ nav back (Overview→Institution→Back→Overview) · ✅ Ask/Afford cards gone · ✅ short month labels · ✅ routines 7-day grid readable (user accepts as interim; frequency-aware redesign still open). 🟡 top-bar auto-hide worked but jittered → **mitigated, user-accepted** (goes away once keyboard is up; ↓ #top-bar). 🟡 trend tooltip taps but doesn't scrub on touch → **deferred** (↓ #income-spending). 🔴 account entry still broken on fresh device → **root-caused + refix + on-device verified** (↓ #account-entry). Remaining open from this batch: #off-switch, #desktop-cold-open, #android-back, #trend-touch-scrub. (#recurring-drilldown DONE + Dashboard-extended + overlap-fixed + **user-confirmed on-device 2026-08-23**.)
- [x] **[URGENT — live bank source] Compiled brokerage poller was stuck in a login-retry loop on the box.** It was waiting for OTP and **kept re-attempting login after each failure**, generating repeated real-bank login-attempt notifications (risk: **account lockout / fraud flags**). **FIXED + DEPLOYED + LIVE 2026-08-23** (`core/src/auto_import_scheduler.rs`): both scheduler loops now **halt** on `Err(NeedsReauth)` instead of retrying-with-backoff — each retry re-ran the login driver, which *was* the hammer. The source goes dormant in `NeedsReauth`; a successful **Reconnect** (`SourceRegistry::reauth`) **re-arms** the loop (new `rearm_if_dormant`, `JoinHandle::is_finished`-guarded so a healthy source never double-spawns) and fires an immediate fresh pull. Halt is scoped to `NeedsReauth` only — transient `Upstream` blips keep their exponential backoff. +3 tests (halt / resume-on-reconnect / transient-still-retries); 30/30 scheduler pass. **Deployed:** committed to public `main` (`0efe819`) → private overlay `deploy.yml` (build+push image → box remote-deploy → **health-gate passed**, 41s). Box `/health` ok; the brokerage source now reports `active`/`healthy` (user reconnected the OTP), so the breaker is in the running code and will halt-and-wait on the next reauth instead of hammering (**not yet observed under a live reauth** — nothing to trigger it while healthy). The **box-side STOP** fallback (comment out the source's credentials section → `docker compose … up -d`) is no longer urgent now the breaker is live; a proper in-app off-switch is tracked ↓. [BUG, high — DONE, live-verified]
- [x] **No UI off-switch for compiled bank sources.** Surfaced chasing the hammer ↑ (2026-08-23): the Settings **Add/Edit/Remove** (3.7) only manages `sources.toml`-declared generic sources; a compiled overlay bank source shows only under "Running now" with **Fetch now / Reconnect** — no way to disable/pause it from the app, so there was no in-app path to stop the runaway source (had to go box-side: comment out its credentials section, `docker compose … up -d`). Want a **pause/disable toggle** on every source row (incl. compiled ones) that live-aborts its scheduler task without deleting config. (The new `NeedsReauth` circuit-breaker makes a *reauth-waiting* source go quiet on its own, but a *healthy* compiled source still can't be paused.) [M, frontend+backend] — **DONE + web-verified 2026-08-25 (#367).** Architecture:
    - **Registry (`core/src/auto_import_scheduler.rs`):** `SourceStatus` grew a `paused: bool`; new `SourceRegistry::pause(name)` aborts the task + `task=None` + `paused=true` (entry & config kept), `resume(name)` re-spawns via `spawn_with_registry` (immediate pull) + clears the flag. `task=None` on pause is load-bearing: `rearm_if_dormant` treats a *finished* handle as "resume me", so a successful `reauth` on a paused source clears `auth_state` but must NOT resurrect the loop — only `resume` does. Tests: pause-aborts-keeps-entry, resume-respawns, resume-noop-when-running, pause-unknown-false, reauth-does-not-unpause. (+5)
    - **Persistence (`core/src/auto_import/paused.rs`, NEW):** `paused_sources.toml` holds paused *names* (not defs — that's what lets it cover compiled sources with no `sources.toml` entry). `set_paused` is idempotent load-modify-save (temp+rename). A pause MUST survive restart — re-arming a paused bank source on reboot is the exact runaway hammering #367 exists to prevent. (+4 tests)
    - **Boot (`setup.rs::spawn_sources` + `server/src/lib.rs`):** the paused set is threaded into `spawn_sources`, which **registers-without-spawning** a paused source → it appears in the snapshot flagged `paused`, resumable, but never ticks *even once* at boot. `run()` loads the set before spawning; a load failure degrades to "nothing paused" (never fails startup). Applies uniformly to compiled overlay sources (all flow through `spawn_sources`). (+1 test: paused-registers-but-never-ticks)
    - **Routes (`server/src/routes/auto_import.rs`):** `POST /auto_import/sources/{name}/{pause,resume}` → registry op + persist (persist is a hard requirement, 500 on failure — a silently-non-persisted pause is the failure mode we're preventing); 404 if the name isn't registered. Config add/edit + remove now clear any stale persisted pause (re-config implies re-run).
    - **Client:** `set_source_paused` Tauri command (HTTP proxy) + `invoke_set_source_paused` bridge (with a `MOCK_PAUSED` set so the mock is walkable); `paused` field added to both `AutoImportSourceView`s (`#[serde(default)]`, older-server-safe). **UI (`settings.rs` `AutoImportRow`):** every Running-now row gets a Pause↔Resume toggle + a "Paused" pill + a "Paused — not auto-importing until resumed." subtitle; **Fetch now is disabled while paused** (a manual fetch on a paused runaway source is exactly what we don't want).
    - **Verified (Playwright, mock, desktop + narrow):** every row (globepay compiled, northwind-sync reauth-capable, imap-*, my-checking config) shows Pause; pausing globepay → Paused pill + Resume + disabled Fetch now + "'globepay' paused." banner; resume → back to Pause/Fetch-now; no body x-scroll at 1280 or ~390; **0 console errors**. Clippy clean ×4 (core-via-server, server --all-targets, frontend mock + non-mock, src-tauri lib); core auto_import 102 tests + new pause/resume/paused/setup tests all green. On-device confirm rides the next server deploy + APK.
- [x] **Account entry is torture + FALSE "No such account in ledger".** Manual account typing is painful AND validation rejects accounts that DO exist (full name typed, query returns results, yet "No such account"). **FALSE-negative half FIXED 2026-08-23; ROOT CAUSE CORRECTED.** The old `roster_len=0` hypothesis was a **red herring** — `roster_len` is the *legacy* `ROSTER_FILE` (retired in 3.9, empty by design everywhere); `list_known_accounts` doesn't use it. Confirmed via the user on-device: the phone's **Accounts screen + net worth are correct**, so `budget.journal` (and thus the backend `known_accounts_from`) **is** populated on-device (sync-pull runs the `JournalFile` projection). The real bug was **frontend-only**: `use_account_suggestions_provider` fetched the account list **once at boot** and never refreshed — on a fresh phone that fetch runs *before* the 12,643-event backfill fills the ledger, leaving the list empty (→ every real account flagged "No such account") until an app restart, while the live txn queries (re-read per navigation) worked. **Fix:** the provider now subscribes to `sync_epoch` (one-shot `use_future` → reactive `use_effect`+`spawn`, the same idiom journal/notes/routines/finances use) and re-fetches as the backfill lands; moved its registration in `main.rs` to *after* the `SyncRefresh` provider (else `use_sync_epoch` falls back to an inert signal). Frontend clippy clean (mock+default, wasm); Playwright mock walkthrough clean (app boots 0 console errors → no hook-order panic from the reorder; Finances Manual form → account field dropdown populates + filters `Assets*`). **UPDATE 2026-08-23 (on-device) — the `sync_epoch` re-fetch above did NOT hold on a real fresh device.** Diagnosed live via WebView CDP + logcat: backend is perfectly healthy (on-device `budget.journal` parses to 92 accounts; a live `invoke('list_known_accounts')` returns all 92; the backfill DID emit `sync:applied pulled=12659`; the Tauri event round-trips to JS listeners) — but the shared `AccountSuggestions` list stayed empty because the root fetch runs at boot **before** the backfill writes the journal, and the same-scope root-provider effect's epoch re-fetch never re-ran on this device. **Real fix (verified on-device):** `AccountInput` now refreshes the shared union **on focus if empty** (`components/account_input.rs`) — focus always lands *after* the backfill and hits one field at a time (no thundering herd). Sidesteps the root-effect timing entirely; self-heals. **Verified on a wiped + re-synced device (fresh install path) via CDP-driven manual-entry form:** real account `Expenses` → recognized, no warning, 8 dropdown suggestions; fake `Zzz:Nonexistent` → correctly flagged 'New account', 0 suggestions. The *torture* half (typing UX) was the date-entry item ↓ (DONE). [BUG, high — DONE, on-device verified]
- [x] **Date entry is text-only + painful.** The native calendar was replaced with text-entry-plus-warnings after a prior picker-freeze bug (native `<input type=date>` opens the webkit2gtk GTK popup that grabs focus and freezes the app). Typing full dates every time was torture. **DONE 2026-08-23 (user chose the calendar-popover option).** `DateField` (`components/date_field.rs`) keeps the typeable+validated `YYYY-MM-DD` input and adds a 📅 button that opens an **in-app month-grid popover** (pure Dioxus, no native control → no freeze): prev/next month nav, today ring, selected-day highlight, tap a day to fill+close, outside-click closes. Applies to all 6 `DateField` sites (forms + compact filter rows). Reused the journal calendar's grid logic by extracting `MonthCell`/`build_month_cells` (+`prev_month`/`next_month`) into a shared `components/month_grid.rs` (journal now imports it; +tests moved/added). Clippy clean (mock+default, wasm), 77 frontend tests pass. **Playwright-verified (mock, desktop):** icon opens the popover on the value's month (Aug 2026), day-15 pick fills `2026-08-15` and closes, today (23) ringed; 0 console errors. On-device confirm rides next APK. [M, frontend — DONE]
- [~] **Desktop froze on first open with an "initializing editor" message** — had to restart the app before it proceeded (before the sync test). User thinks it's a known issue. Desktop-side editor-init/cold-open hang (cf. mobile cold-open fix 1357151). **🅿️ SHELVED 2026-08-24 — not reliably reproducible; workaround = close+reopen. Full investigation + remaining hypotheses + proposed fix under the release-roadmap #370 entry above.** [BUG, desktop]
- [x] **Nav back destination is wrong.** Overview → Institutions → Back landed on the **Analyze** tab, not the **Overview** tab you started from (Institutions drill-down is routed under Analyze; the back-stack didn't remember the originating surface). **FIXED 2026-08-23** (`pages/finances.rs`). Root cause: Accounts/Institutions **and** Reconciliation are each reachable from *both* the Overview and Analyze surfaces, but their `on_back` hardcoded `→ Analyze`; a static back-target can't be right for a screen with two parents. Fix = an ephemeral `return_to: Signal<FinancesView>` capturing the surface at open-time; each multi-origin drill-down's `on_back` returns there (Dashboard→Unmatched→Reconcile returns to Dashboard). The Analyze-only drill-downs keep their fixed back. Ephemeral by design (only surface roots persist → restore always lands on a root). **Playwright-verified (mock):** Overview→Institutions→Back→Overview; Analyze→Accounts→Back→Analyze; 0 console errors. [BUG, frontend-nav — DONE, on-device verify rides next APK]
- [x] **Android system back button closes the app** instead of acting as in-app Back. Want the hardware/gesture back to pop the app's own nav first (the on-screen Back is often out of thumb reach on mobile). **DONE 2026-08-25 (#372) — web-verified; on-device rides next APK.** Architecture: the frontend owns a root `BackNav` context (`main.rs`) so pages stay decoupled — each page calls `use_page_back(depth_fn, on_pop)` to (a) publish its poppable depth upward and (b) receive a "pop one level" pulse it applies **in its own scope** (no cross-scope signal writes → no Dioxus panic; same channel→spawn marshaling as the `sync:applied` path). The root reduces every back press to one precedence chain: **open drawer → active page's drill-down → non-home tab → (home root) background the app**, and keeps `window.__omniCanGoBack` current via a reactive effect (drawer ‖ page-depth ‖ non-home-tab). Native side: `MainActivity.onBackPressed` (android-overrides, copied by `build.rs`) reads the flag with `evaluateJavascript` and either dispatches `window.dispatchEvent(new Event('omni:back'))` (frontend pops one level) or `moveTaskToBack(true)` — backgrounds rather than finishes the activity, which is the standard root-back behavior AND keeps the app warm to sidestep the #370 cold-open cost. Bridge helpers `listen_window_event` / `set_can_go_back` (`bridge.rs`) use the plain DOM event bus so it works under dx serve / desktop / Playwright too. Per-page wiring: **Finances** routes through a new single-source-of-truth `finances_back_target(view, return_to)` map (mirrors the on-screen Back destinations incl. the multi-origin `return_to` for Accounts/Reconciliation; clears the same selection state; +3 unit tests, 82 frontend tests pass); **Notes** (List↔editor); **Routines** (DailyChecklist→GroupList→GroupDetail/AddGroup, 2 levels); **Journal** (calendar drawer closes on back). Clippy clean ×2 configs (`--all-targets`, wasm). **Playwright-verified (mock, desktop+390px mobile), 0 console errors:** home root flag=false; each non-home tab flag=true; Finances Institutions→Accounts→back→Overview→back→Journal; Routines GroupDetail→GroupList→DailyChecklist→Journal with correct flag transitions; Notes editor→List; calendar drawer open (home tab)→back closes→flag=false; mobile nav drawer open→back closes→flag=false. **On-device APK verification pending next Android build** (cf. the 1.13 "rides next APK" pattern; predictive-back API 33+ migration noted in the Kotlin comment but N/A on the API-29 target). [M, frontend/Android — DONE, on-device verify rides next APK]
- [x] **Top-bar sync + hamburger icons waste real estate** — make them hideable / reveal-on-demand. **DONE 2026-08-23 (user chose auto-hide-on-scroll).** The header (`main.rs`) now collapses its height+padding (not just a transform, so content reclaims the strip) when the content column scrolls **down**, and returns on scroll **up** or at the top — driven by the content div's `onscroll` (`ScrollData::scroll_top()`, small thresholds to debounce jitter; reveal within 8px of top). Reveals on tab switch too (a fresh page starts scrolled to top). The mobile hamburger hides with it — fine, since scroll-up or the 1.12 edge-swipe brings nav back. **Playwright-verified (mock):** scroll down → header collapses (chip+hamburger gone, content rises); scroll up → returns; 0 console errors. (Note: needs a real scroll range to trigger — a page that barely scrolls just keeps it visible, which is correct.) **UPDATE 2026-08-23 (on-device): jitter REDUCED, not eliminated — user accepts current state.** Cause: collapsing the ~90px header shrinks the scroll range and clamps `scrollTop`, which the direction logic misreads as "scrolled up" → reveal → un-clamp → hide … a loop. Mitigation shipped: auto-hide only engages when content clearly overflows (`scroll_height - client_height >= 150`); below that the header stays pinned. **User on-device 2026-08-23: still sees some jitter on their case (scroll range evidently >150px), but it doesn't bother them — it vanishes the moment the keyboard is up while typing (exactly when the real-estate matters).** Left as-is (no more build cycles for it). If revisited: raise the threshold or add real hysteresis (separate hide-engage vs force-reveal thresholds) so the collapse/clamp can't re-trigger. [S, frontend — mitigated, user-accepted]
- [x] **Remove the "Ask finances" placeholder AND the "Can I afford" box from the UI — KEEP the afford *function*.** Rationale: the coming **chat/LLM interface** (MCP-like: a model can invoke all app actions on the user's behalf + answer questions) becomes the primary way to do these, superseding the placeholder + the afford box. See [[project-llm-primary-interface-next-push]]. **DONE 2026-08-23.** Removed the `AskFinancesCard` (Analyze "Ask your finances / Soon" placeholder) + `AffordCard` ("Can I afford …" box on Dashboard) and their orphaned frontend plumbing (`invoke_check_affordability`, `mock_check_affordability`, `AffordVerdictView`). **Kept the afford *function*** — the backend `check_affordability` command (`commands/budget.rs`, registered in `lib.rs`) + `core::dashboard::AffordVerdict` are untouched, so the LLM/MCP layer can invoke it (the WASM bridge wrapper is re-addable in ~10 lines if an in-app surface ever returns). Also dropped "can-I-afford" from the Dashboard tool subtitle. **Playwright-verified (mock):** neither card renders; Dashboard shows net worth + unmatched + trend + recurring; 0 console errors. [S — DONE]
- [x] **Recurring: tap a suggested pattern to see the actual matched transactions** it considers recurring. Today it only shows amount / account / frequency / qty-found; want to drill into the underlying txns. **DONE 2026-08-23.** Each pattern card in the Recurring review (`RecurringRowCard`, `pages/finances.rs`) gets a **"View N transactions"** toggle that lazily fetches + inlines the underlying txns (date + description, under a "Matched transactions · <amount>" header; "Hide" collapses). New backend command `list_recurring_matches(pattern_id)` (`commands/budget.rs`, registered in `lib.rs`) re-finds them: looks up the pattern, queries its Expenses account over the seen-window via the existing `list_transactions`, then keeps only postings matching the pattern's exact account + amount(2dp) + commodity — the same grouping key the detector uses, via a new shared `recurring::posting_in_pattern` (+unit test, mirrors `detect_parsed` so the shown set == what was counted). Bridge `invoke_list_recurring_matches` (+mock returning sample rows). Clippy clean (core, app, frontend mock+default); core + 77 frontend tests pass. **Playwright-verified (mock, 1280):** Analyze → Recurring → "View 6 transactions" expands to 3 dated rows under the header, "Hide transactions" collapses, second pattern independent; full UI intact (no panic). On-device confirm rides next APK. **EXTENDED 2026-08-23 (Dashboard card, per user):** the review-screen drill-down was invisible on the user's device because their review queue is empty (all 10 patterns already confirmed/dismissed from prior use), so the same drill-down was added to the Dashboard's confirmed **"Recurring obligations"** card (Analyze → Tools → Dashboard). New `RecurringObligationRow` component (`pages/finances.rs`) makes each obligation a tap-to-expand row (▸/▾ chevron) that lazily fetches via the *same* `invoke_list_recurring_matches`. Feasible with **zero re-derivation**: the DB pattern id is now threaded straight through `core::dashboard::RecurringObligation.pattern_id` (← `RecurringPatternRow.id`, `meta::id(id)` form) → the view types → the frontend, so `list_recurring_matches` keys on the exact id the lookup query uses (confirmed patterns are in the DB with `status=confirmed`, and the command reads all statuses). Clippy clean ×4 configs; core (20 dashboard tests, +id-threading assert) + 77 frontend tests pass. **Playwright-verified (mock, 390):** Analyze → Dashboard → 3 obligation rows; expand Netflix → "MATCHED TRANSACTIONS · 16.99 CAD" + 3 dated rows; expand Telus independently (55.00 CAD); collapse toggles; 0 console errors. **On-device CDP-verified 2026-08-23 (debug APK, real data):** all 5 confirmed patterns render as expandable rows; pattern #1 → 8 real matched txns, pattern #2 → 10 (independent, real backend — the path mock can't reach, since `mock_list_recurring_matches` returns canned rows); collapse/expand toggles hold. **Follow-up overlap FIX 2026-08-23 (`49a239d`):** long account-path vendors (`Expenses:Housing:Rent`, 21 chars) overlapped the amount — the row's vendor span lacked `truncate`+`min-w-0` (flex items default `min-width:auto` → refuse to shrink below content, so `min-w-0` on the *container* alone doesn't ellipsize the child). Vendor span now `truncate min-w-0`, cadence `shrink-0`; on-device geometry check across the 5 real rows: `anyOverlap:false`, the 21- and 27-char vendors ellipsize while "monthly" + amount stay fully visible. [S/M, frontend — DONE, on-device verified]
- [x] **Income & Spending bars need hover tooltips** showing amounts, like the Overview trend chart already has. **DONE 2026-08-23** (`pages/finances.rs`, `MonthlyTrendCard`). The bars carried only a native `title=` (slow, unstyled, invisible on touch); replaced with the net-worth chart's richer pattern — a `hover_idx` signal, per-column hover targets, and an absolutely-positioned styled chip showing **month + income (green) + spending (red)**, anchored to the hovered column. The month label also brightens on hover. **Playwright-verified (mock):** hovering a column surfaces the chip (e.g. 2026-02 → 3,200.00 income / 2,670.42 spending), no clipping; 0 console errors. **NOTE:** hover-only, so on the S9 (touch) it needs a tap/press affordance — folded into the on-device pass (cf. #11 dataviz density). **ON-DEVICE 2026-08-23:** user confirms tap shows the crosshair for the tapped point, but **can't scrub/drag across points** like desktop hover does. **DEFERRED (own follow-up):** true touch-scrub needs a single `ontouchmove`/`onpointermove` on the chart mapping screen-X → SVG coord → nearest point index (touch events target the start element, so per-point `onmouseenter` can't fire mid-drag) — fiddly coordinate work, split out rather than risk it in the fix batch. [S, frontend/dataviz — hover DONE; touch-scrub DEFERRED]
- [x] **Trend graph is too crowded on the S9** (small screen). Responsive dataviz density needs to adapt to narrow widths. **DONE 2026-08-23** (`pages/finances.rs`, `MonthlyTrendCard`). Reproduced at 360px (the S9's CSS width): the Income-vs-spending bar axis labels (`YYYY-MM`, e.g. `2026-05`) **wrapped to two lines** per column. Fix = new `short_month("2026-05") → "May"` helper; the axis now shows the abbreviated month (`Dec Jan Feb Mar Apr May`, single line, `whitespace-nowrap`), with the full month+year still in the #10 hover tooltip. +1 unit test (`short_month_abbreviates_and_falls_back`). **Playwright-verified at 360px:** labels single-line, no wrap; 0 app console errors. (The net-worth Overview chart was already fine at 360px — only 2 endpoint date labels.) [S/M, frontend/dataviz — DONE, on-device confirm rides next APK]
- [x] **Routines Manage 7-day trend: descriptions truncated to ~2-3 chars on mobile** — basically unreadable. **READABILITY FIXED 2026-08-23** (`pages/routines.rs`, `HistoryGrid`). Root cause: `grid-cols-[1fr_repeat(7,32px)]` — the 7 fixed 32px day-columns ate the width, leaving the `1fr` name column ~60px, so `truncate` clipped names to 2-3 chars. Fix = responsive stack: on mobile the name spans all 7 columns (its own full-width line, no truncate) with the day-cells below (`mx-auto`-centered under the Mon–Sun header); on md+ the inline single-row grid is unchanged. **Playwright-verified at 360px:** "Glass of water" / "Meditation" fully readable, cells aligned under day headers; 0 app console errors. **NOTE:** the bigger **frequency-aware redesign** (make the trend adapt to daily/weekly/monthly/custom, not 7-day-hardcoded — "Daily Flow consistency visualizer redesign") is still open as its own backlog item; this fix is only the mobile-readability half. [M, frontend — readability DONE; frequency-aware redesign still open]
- Already-tracked (reaffirmed this pass): **timestamp reveal for notes/journal** → the 344 entry above (sharpened 2026-08-14: 12H+tz, Option-A-first); **email cleanup + IMAP** → 337/341 (private-repo tracked, [[feedback-public-repo-keep-private-identity-out]]); **catch up ledger+journal before the final import** + **OTA update path still untested** → go-live prep ([[project-hetzner-db-reset-for-testing]] go-live remake; OTA device round-trip was deferred to polish @ delivery-pipeline backlog).
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
- [x] **Editor feel: space + typography** (dogfooding 2026-07-03, on-device vs Obsidian; ref
  shots `~/Pictures/editor_reference/`). The journal/notes editor uses space poorly vs Obsidian:
  (1) prose renders in **monospace 14px** (code-like, wide glyphs, early wrap) — should be a
  proportional font ~16px with airy line-height; (2) **triple-nested padding** (page wrapper card
  `rounded-lg border shadow-2xl` + `Editor`'s own `border rounded-xl p-4` + inner `p-4` + column
  `p-4`) wastes horizontal width — Obsidian hugs the width with one small gutter; (3) fixed
  `min-h-[400px]` **island with dead space** — the writing surface should fill the screen height;
  (4) a **per-line underline** artifact (CM default markdown heading/link highlight). **DONE
  2026-07-03** — CodeMirror `EditorView.theme` in `assets/js/editor.js` (proportional system-sans
  16px / line-height 1.65, no underline, `flex 1 0 auto` fill); flattened the triple-nested cards
  in `editor.rs`/`journal.rs`/`notes.rs` (full-bleed, one gutter); full-height via a `min-h-full flex
  flex-col` page-root chain + `flex-1` editor region (kept the page column as the single scroll
  parent so 1.10 caret-above-keyboard is untouched). Verified: journal + notes (mobile + desktop),
  proportional/roomy/full-height/no-underline, 0 console errors; frontend clippy clean mock+default.
  Plan: `~/.claude/plans/binary-puzzling-lerdorf.md`. [M]
- [x] **Nav drawer: swipe-to-close** (dogfooding 2026-07-03). Swiping the open drawer back toward
  the edge did nothing — only a scrim tap closed it. **DONE 2026-07-03** — added the inverse of the
  edge-swipe-open in `main.rs` (`EDGE_SWIPE_CLOSE_PX`; track any touch while open, leftward travel
  past the threshold → close). Verified: leftward swipe closes; vertical/sub-threshold gestures + a
  scrim tap behave correctly. [XS]
- [x] **Calendar = right-swipe sidebar widget, not a separate tab** (dogfooding 2026-07-03, user
  flagged — don't lose). In Obsidian the calendar is a compact month-grid **widget in a right
  sidebar that swipes in from the right edge and overlays the current note** (quick day-jump without
  leaving the note), with **per-day activity dots** (`••`/`•` = entries that day) + a **day-complete
  check** + note word/char stats. omni-me buried it behind a full-screen `Today | Calendar` tab
  toggle with big empty cells. **DONE 2026-07-03** — retired the `Today | Calendar` sub-tab
  (`JournalSubTab`/`JournalSubNav` deleted) and rebuilt the month grid as a right-edge `CalendarDrawer`
  (mirror of the left `NavDrawer`: always-mounted, class-toggled slide, scrim + inverse-swipe close)
  overlaying the day view. Opened by a **right-edge swipe** (mirror of the nav swipe-to-open, anchored
  to the right edge via `viewport_width()`; swipe-left opens, swipe-right closes) **and** a toolbar
  **Calendar button** (desktop's opener — no swipe there; drawer is no longer `md:hidden`). Selecting a
  day jumps `selected_date` + closes, staying in the note. **Activity marker per day:** filled accent
  dot = has entry, check ✓ = `complete`; backed by a new `list_journal_day_stats(from,to) -> [{date,
  complete}]` (core query + Tauri command + bridge + type; replaced the date-only `list_journal_dates`).
  SVG chevrons replaced the emoji ◀/▶. Verified (390px + 1280px, mock): drawer slide + scrim-close +
  day-jump-and-close; synthetic-touch swipe open/close pass with negatives (sub-threshold + non-edge
  no-op); 0 console errors; clippy clean ×4 (core, app, frontend mock+default). **Synergy realized:**
  the two drawers stay conflict-free by touch-origin (left strip vs right edge), no coordination flag.
  Deferred: note word/char stats footer (needs the day's live content plumbed up) → new item below. [M]
- [x] **Calendar footer: note word/char stats** (Obsidian parity; split from the calendar-widget
  item 2026-07-03). Obsidian's calendar sidebar shows the *active note's* word/char count in a footer.
  Deferred from the drawer rework because it needs the viewed day's live editor content lifted from
  the keyed `DayView` up to `JournalPage` (where the drawer lives). Small, but crosses the
  keyed-component boundary — do when touching journal continuity next. **DONE 2026-07-04** — crossed
  the keyed boundary with an **up-channel signal**: `JournalPage` owns a `viewed_body: Signal<String>`
  passed *down* into `DayView`, which mirrors its live `body` up via a post-hydrate `use_effect`
  (gated so the empty pre-load body can't flash "0 words"). `JournalPage` computes counts through a
  pure `body_stats(&str) -> (words, chars)` helper (whitespace-delimited words, Unicode-scalar chars;
  frontmatter excluded by construction since Phase 5 lifted it into the panel — `body` is already the
  prose) and passes them to a new `CalendarDrawer` footer (`mt-auto`, pinned to the drawer bottom,
  singular/plural labels). Because signals carry their own subscription graph, a keystroke re-renders
  the parent + drawer *without* a `key` remount (the key only changes on day-jump). Verified: **1 new
  unit test** (`body_stats_counts_words_and_chars`: empty/whitespace-only → 0 words, prose, collapsed
  whitespace runs, `café`=1 word/4 chars) → **71 frontend tests pass**; clippy clean ×2 (mock+default,
  wasm, `-D warnings`); **Playwright 390px & 1280px** — footer shows the viewed note's stats, word
  count matches the real body exactly, **updates live on edit** (append " zzq" → 11→12 words / 64→68
  chars), autosave (**SAVED** pill) + day markers unregressed, 0 console errors. [S]
- [x] **Typed properties panel** (Phase 5.1/5.2; dogfooding-confirmed by the Obsidian Properties
  card in the ref shots). Model A (Obsidian-style, user-decided 2026-07-03): lift the `---`
  frontmatter **out of the editor body** into a typed card above it — date (read-only in journal) /
  tags chip input / the 3 reflection widgets (`homework_for_life`/`grateful_for`/`learnt_today`) +
  a raw legacy escape hatch. **DONE 2026-07-03 (journal only; generic-notes panel split off below
  per user scope decision).** New **frontend-only** module `note_frontmatter.rs`: `JournalProps` +
  `split_journal` + `serialize_journal`. Key finding that reshaped the plan: **the frontend crate
  has no `core` dep and no YAML lib** — so instead of reusing `core::import::map_frontmatter`/
  `parse_markdown` (the plan's assumption), the split/serialize are **pure-client, safe by
  construction**: emit empty reflection → bare `key:`, non-empty → double-quoted single-line
  `key: "…"`, tags inline `[a, b]`, reflections before any legacy block. That output is a strict
  subset of what `core::is_complete` + `core::import::parse_markdown` already accept → **no core
  changes, no new Tauri commands, no async in the edit loop.** `content` stays the single source of
  truth (autosave/continuity/1.2/1.7/1.8 untouched): the panel + a body-only editor are two inputs
  that `recombine()` into `content`, and **hydrate never recombines** so an untouched entry can't
  phantom-save. `JournalPropertiesPanel` (+ `ReflectionField`, `field-sizing:content` auto-grow;
  Dioxus gotcha: a component param literally named `props` collides with the `#[component]` macro →
  renamed `model`). Verified: **6 unit tests** (round-trip byte-identical on the template render;
  `is_complete`-shape for empty vs filled; special chars `:`/`"`/newline; legacy preserved after
  reflections; block-list tags read; no-fence) + clippy clean ×2 (mock+default, `-D warnings`) +
  Playwright 390px & 1280px: fresh template splits to date `2026-07-10`/tag `daily_note`/empty
  reflections/`## What happened` body with **no `---` in the editor**; opening an entry shows
  **SAVED** (no phantom save); tag add/remove, reflection edit, raw-hatch expand; body edit
  recombines + autosaves and keeps the panel state (no `---` leak, editor not reset); 0 console
  errors. **Deferred to on-device dogfooding** (mock can't drive them): the Complete-pill flip on a
  real backend (covered by the `is_complete`-shape unit test) and the closed-day read-only panel
  (the `read_only` prop is wired). Plan: `~/.claude/plans/binary-puzzling-lerdorf.md`. [L+M]
- [x] **Calendar day-jump left the previous day's content on screen** (latent bug in the
  uncommitted calendar-widget rework, surfaced 2026-07-03 while verifying the properties panel — an
  empty past day showed today's entry). Root cause: the widget rework replaced the old
  `Today | Calendar` sub-tab (which unmounted `DayView`) with a drawer that keeps `DayView` mounted,
  so day selection relied solely on `key: "{selected_date}"` — but **a bare `key` on a directly-
  rendered Dioxus component is a no-op; keys only force remount inside a list.** The `date` prop
  updated (header + Save state) while the entry/body/props signals kept the prior day. **DONE
  2026-07-03** — render `DayView` through a one-element `for day in std::iter::once(selected_date)`
  so it gets list semantics and the key actually remounts it on jump. Verified: jumping to an empty
  day now loads its template (fresh entry, no AI-Analyze button, template body). [XS]
- [x] **Generic-notes properties panel** (the smaller half of 5.1/5.2, split from the journal panel
  2026-07-03 per user scope). Notes have raw `---` frontmatter but **no in-app tags editor today** —
  add a `NotePropertiesPanel` (tags chip editor + raw escape hatch; title is already a separate
  field, no date). Reuse `note_frontmatter`'s split/serialize pattern generalized for notes (no
  reflection keys). `pages/notes.rs` `NoteEditor` mirrors `DayView`'s content flow. **DONE
  2026-07-03.** `note_frontmatter` gained `NoteProps` + `split_note`/`serialize_note` (tags-only
  known key; everything else → `legacy_raw`). Key divergence from the journal serializer: a generic
  note may have **no frontmatter at all**, so `serialize_note` emits a `---` block *only* when there
  are tags or legacy props — editing a fence-less note never injects a spurious fence (covered by
  `note_with_no_frontmatter_stays_fence_free` + `note_removing_last_tag_drops_the_empty_fence`). Same
  pure-client, `content`-is-source-of-truth, **never-recombine-on-hydrate** invariant as the journal;
  autosave/continuity (1.3/1.7/1.8) untouched. Extracted the shared **`components/tag_editor::TagChipEditor`**
  (both panels now use it) and moved the serialization-safety `sanitize_tag` next to the serializer.
  Caveat (matches journal): the projection's `tags` column is **LLM-derived** (`on_llm_processed`),
  so frontmatter tags edited here persist in the note but don't populate the note-card tag list — a
  backend-side follow-up if wanted. Verified: **11 frontend unit tests** (6 journal + 5 note/sanitize,
  incl. round-trip, add-tag-creates-fence, remove-last-tag-drops-fence, block-list read, sanitize) +
  clippy clean ×2 (mock+default, `-D warnings`) + Playwright 390px & 1280px: existing-note panel
  (Tags + raw hatch, body-only editor, SAVED-on-load no phantom save), add tag (`sanitize_tag` strips
  `#`/space/`!`), body edit recombines + preserves the tag + autosaves with no `---` leak, remove tag
  drops the fence, raw-hatch edit recombines (legacy never leaks to the editor), continuity round-trip
  re-hydrates the stored full-raw content (raw hatch auto-expands when legacy present), New-Note blank
  draft (empty panel, adding a tag creates frontmatter without leaking `---`); **journal panel
  regression-checked** (tag add/remove still work via the shared `TagChipEditor`); 0 app-level console
  errors. [M]
- [x] **Harden `is_complete`** (Phase 5.3; `core/src/events/notes_projection.rs:282`). The scanner
  terminated on the first non-`key: value` line after any kv, so block-list YAML (`tags:\n  - x`),
  blank lines mid-frontmatter, or reordering silently broke journal auto-close (the template
  worked around it with inline `tags: [daily_note]`). **DONE 2026-07-03** — rewrote the scanner to
  be **fence-aware**: it peeks the first non-blank line, and when it opens a `---` fence it scans
  the *entire* block to the closing fence — blank lines, indented continuation lines, and block-list
  items (`- x`) are skipped as YAML continuations, never terminators. **Key reordering, block-list
  `tags`, and stray blank lines can no longer hide a later reflection key** (the exact Obsidian-import
  shapes). The fence-less mobile shape keeps its forgiving leading-run behavior (stops at the first
  blank/non-kv line after a kv = the body). Pure backend logic, no UI. Verified: **11 `is_complete`
  unit tests** (5 original + 6 new: block-list tags, reordered keys, blank-lines-in-fence,
  fence-less block list, body-prose-stays-false, block-list-reflection-stays-false-since-empty-scalar)
  + full `notes_projection` module (16) + full core lib suite (**450 passed**) + `cargo clippy
  -p omni-me-core --all-targets` clean. Pairs with the Phase-5.1/5.2 panel serializer (whose output
  was already `is_complete`-safe; this widens the *acceptance* side for imports). [S]

- [x] **Account-field autocomplete + unknown-account affordance** (dogfooding 2026-06-17). **DONE
  2026-06-20 (public-repo / frontend-only).** Shared `AccountInput` (`components/account_input.rs`):
  controlled `value`+`on_input` (each site keeps its save closure), suggestion dropdown + keyboard
  nav (Arrow/Enter/Escape), `AccountMode::{Add,Query}`-driven unknown affordance (`Add`→"New account
  — will be created", `Query`→amber "No such account in the ledger"), fed by an `AccountSuggestions`
  root context (one `invoke_list_known_accounts` fetch, `refresh()` after account-creating saves).
  Matching = **case-insensitive prefix, cap 8** (user Learn-by-Doing); `is_known` case-insensitive to
  match. Wired into all **7** account-path sites (TransactionForm/Budget/NoMatch/StatementImport/
  Journal-rename → Add; QueryBuilder/BalanceCheck → Query). Clippy clean both feature configs;
  Playwright mock walkthrough green (0 console errors; PNGs `logbook/_assets/account-input-typeahead/`).
  **Deferred to dogfooding:** segment-aware / leaf-by-short-name matching (`coffee`→`Expenses:Food:Coffee`)
  until the mental model is clear; the Query-mode case-strictness nuance. Original ask:
  Everywhere the user types an account name, offer a **type-ahead dropdown** of matching accounts
  (search-box style); and make it **visually clear when the typed account is not yet in the ledger**.
  - **Build once, reuse everywhere:** a single shared `AccountInput` typeahead component, not a
    per-form re-implementation (shared-UI-shape principle). **Input sites to cover:** TransactionForm
    (add/edit posting), `QueryBuilderView` account predicate (R2), budget setup (category = account),
    reconciliation no-match category fill-in (`resolve_unmatched`), balance-check account field,
    journal-import rename inputs.
  - **Suggestion source:** the `accounts` table once **3.9** lands (declared accounts), likely unioned
    with accounts actually *seen* in the ledger/journal projection. **Hard dependency on 3.9** (lifts
    the roster into a queryable table) — do after it.
  - **Unknown-account affordance is context-dependent:** in an **add** context a non-existent account
    is *allowed* but flagged "New account — will be created" (catches typos without blocking intent);
    in a **query** context, flag "No such account in the ledger" so an empty result reads as "this
    account doesn't exist", not "no matching transactions" (consistent with the empty-search-shows-
    nothing principle).
  - **Open design Qs:** does "exists" mean *declared* (accounts table) or *seen in ledger* (used in ≥1
    posting)? — they diverge. Segment-aware completion over the `:`-hierarchy (mirroring the R2
    account matcher) so typing `Expenses:F` suggests `Expenses:Food` etc.?
  - Triage: data dependency rides on **Phase 3 / 3.9**; the cross-app UI layer could land as its own
    3.x or in **Phase 5** (editor/typing feel). [M]

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

### 2026-07-06 — journaling personalization

- [ ] **Journal template hardcodes the user's personal journaling framework** — `frontend/src/journal_template.rs::render` bakes in the user's own choices: the three reflection property keys (`homework_for_life`, `grateful_for`, `learnt_today`), the `## What happened today?` section heading, and the `daily_note` tag. Those same three keys are also hardcoded in the day-complete `is_complete` check (`core/src/events/notes_projection.rs`), and the `tags: [daily_note]` inline-list form is itself a workaround for that parser — so the template, the auto-close logic, and the typed properties panel (`journal.rs::JournalPropertiesPanel`, 3 fixed reflection fields) are all coupled to this one personal schema. Generalizing (user-configurable reflection prompts + template) means reworking `is_complete` to not key off fixed names + adding a config surface for the prompt set. Also a mild personalization-in-open-core smell (personal journaling prompts sit in the public repo, though not identity/financial data). [M] — flagged by user 2026-07-06, **deferred ("resolve later")**; pairs with 5.4 typing-feel + the properties-panel work.

### 2026-07-21 — finances perf + overall UI/UX coherence (HEADLINE — own session)

- [ ] **Finances section feels slow to load / unresponsive, and the overall UI/UX lacks coherence (mobile + desktop).** User (2026-07-21): "better ways to present the data and expose interfaces for me as a user to interact with it." Two intertwined threads: **(1) perf** — the finances views feel laggy on load (seed already noted: balance-cache landed 2026-07-04, but load/interaction responsiveness in the finances section specifically still feels slow — profile the real path: command latency, projection reads, frontend render/hydration, mobile vs desktop); **(2) UX/IA redesign** — the app doesn't feel like one coherent system; rethink how finance data is presented and how the user interacts with it, on both form factors. **Cross-cutting → its own planning-first session** per the defer-major-phases rule, opened with **rendered design candidates** (per the design-render-candidates habit; design for full future scope, go wide before narrowing). Do NOT start as a tail-of-session. [L, → own session] — **IN PROGRESS — Stages A/B/C landed 2026-08-10** (plan `could-you-start-reviewing-curious-dahl.md`; approved IA = **Overview · Ledger · Analyze**). **A (perf):** read-path `tracing` instrumentation (`972cdfb`); measured real data (10,209 txns) → naive indexes insufficient (SurrealDB 3.0.4 won't skip the ORDER BY sort), so the win is frontend caching, done in C3. **B (design foundation):** CSS-var token layer + shared primitives (`Card`/`Button`/`PageHeader`/`Banner`/`StatTile`/`SegmentedNav`/`TextInput`/`Icon`) (`39a6021`); user picked Overview look **C · Balanced**. **C (IA build, 6 commits `dcd37d0`→`e87a5fb`):** C1 real net-worth-history backend (`core::dashboard::net_worth_series`, endpoint == hero; 3 core tests); C2 persistent sub-nav replacing the flat 18-variant hub (all flows preserved, surface persisted in `NavState`); C3 stale-while-revalidate frontend read-cache + skeletons (the top felt-latency lever); C4 the C·Balanced Overview (net-worth hero + range-switchable SVG area chart 1M/3M/6M/1Y/YTD/All + 2×2 card grid); C5 Ledger master-detail (desktop side-by-side / mobile slide-over, row highlight); C6 Analyze landing (cash-flow trend + budgets snapshot + reserved LLM entry). Review gate: core tests + both wasm clippy configs green, Playwright-verified 390+1280 with 0 console errors, inline-edit mutation confirmed. **REMAINING:** ~~Stage D~~ **DONE 2026-08-24** (full primitive refactor across all 5 pages + input-class fold; see #594 in the roadmap above for commit list). Still: on-device/real-data end-to-end pass (mock can't exercise the backend or real perf; rides the queued DB reset). [L, → own session]

---

## Carried backlog (slot into a phase or pull from the friction log)

**Post-launch fix cycle (from Phase 4 GUI validation, 2026-06-22):**
- [x] Per-institution (tag) breakdown drill-down on the Accounts view — group an account's postings by `institution`/`product` tag via the existing `core::query` tag layer (the payoff of institutions-in-tags; `probe_realdb` already resolves the splits exactly). [M] — **DONE 2026-07-04.** Marquee item: a balance-bearing account (e.g. `Assets:NonRegistered:CAD`) pools money across institutions, so this tag-grouping is the *only* per-bank view. Pure core helper `query::group::group_account_by_tag` (sum per (tag-value, commodity), `(unassigned)` fallback, zero-net filter, UNASSIGNED-last sort) + `balances::account_tag_breakdown` (builds `Prices` in-core → base conversion; kept `convert_to_base` private, no `ledger_utils` dep in app). Tauri `account_tag_breakdown(account, group_by∈{institution,product}, base, as_of)` command; frontend `AccountSummaryCard` gains an expand chevron (path-swap, not a purge-dropped `rotate-90`) → on expand fetches the breakdown via a `use_effect` keyed on `expanded`+`group_by` signals, renders per-group sub-rows + a small `Institution | Product` toggle (`TagGroupRow`; single-CAD groups collapse to a total, multi-commodity groups show the per-commodity split w/ base ≈). **Verified:** 8 core tests (6 group + 2 breakdown); frontend clippy wasm mock+default `-D warnings` clean; Playwright mock 390+1280 (expand → institution groups → product toggle → back, 0 console errors). **Leak-check PASS** — added lines carry only fictional names (Globepay/Northwind/Summit/Meridian), 0 denylisted identifiers; pre-commit privacy guard present. Backend built against synthetic tests; real-data verify rides the queued DB re-import.
- [x] Balance-cache perf — `account_summaries` / `dashboard_summary` / `list_detected_accounts` each re-read + re-parse the full `budget.journal` per call; cache parsed balances (invalidate on new budget events). [M] — **DONE 2026-07-04.** Parse-once cache in `AppState`: `journal_cache: RwLock<Option<JournalCacheEntry>>` holding `Arc<JournalArtifacts>` (balance + prices). **Invalidation = the file's own `(mtime, len)` stamp, NOT a hand-bumped counter** — the plan's `journal_version: AtomicU64` would have to be poked at *every* `apply_events` path (single-event, batch import, journal import, sync-pull, auto-import, rebuild — 8 sites, plan named 2) or it silently serves stale balances; a `stat` can't drift out of sync with contents, costs the same as the atomic, and covers future write paths for free. Stamp sampled *before* the read → a mid-rebuild write caches fresher content under the older stamp and the next call re-parses (extra parse, never a stale read). New `AppState::journal_artifacts()` (fast path = read-lock cache hit, no parse; slow path = read+`ledger::parse_artifacts` once) + `journal_artifacts_or_empty()` (degrades a malformed journal to empty artifacts, preserving `auto_roster`/`known_accounts`' old declared-only fallback). Core refactor: `ledger::parse_artifacts` (one parse → both `Balance` via new `balances_from(&Ledger)` + `Prices`; `account_summaries` alone used to parse ~3×/call) + `JournalArtifacts{balance,prices}`/`::empty()`; parsed-input `*_from` variants of `account_summaries`/`auto_roster`/`known_accounts`/`account_tag_breakdown`/`dashboard_summary` (content-taking fns kept as thin wrappers for tests). All 5 read commands + `effective_roster` now consume the shared cache (threaded as `&JournalArtifacts` — no `ledger_utils` dep in the app crate); dead `read_budget_journal` removed. **Verified:** 459 core tests (new `parsed_input_variants_match_content_path` locks cached == content-path across summaries/roster/known_accounts w/ a P-directive FX journal); clippy clean on core + app (`-D warnings`), app `cargo check` clean. No frontend change (drill-down UI unchanged). **Leak-check PASS** — denylist + word-boundary scan of all core/ + tauri-app/ diffs and new `.rs` files clean; only fictional names in fixtures. Cache exercises real data on the queued DB re-import (mock bridge bypasses these commands, so Playwright can't reach the cache).
- [x] JournalFile `account`-directive append-dedup — the projection appends a fresh `account …` line per override toggle (DB upsert is idempotent, but the rendered file accretes duplicates; harmless now that `prep_content` strips them, but it bloats the file). [S] — **DONE 2026-07-04.** The `account_added` arm now calls a new `JournalFile::upsert_account` instead of `append`: under the existing `write_lock` it reads the whole file and splices via the pure, unit-tested `upsert_account_block` — **replace an existing `account <name>` block in place (latest wins) or append when absent**. Block boundary (`find_account_block`) = the directive line + its indented continuation sub-directives (`note`) + one trailing blank-line separator, so the replacement's own trailing blank doesn't double up; EOF-without-trailing-blank handled. `is_account_directive` matches the name on an exact whitespace/EOL boundary → `Assets:Cash` never clobbers `Assets:Cash:USD`, and names with spaces (`Liabilities:Credit Card:CAD`) match. Transactions/P-directives keep the cheap append path (only the rare `set_account_override` pays the full read-rewrite; journal is a regenerable cache so a full rewrite is safe). **Verified:** 466 core tests (6 new: absent→append, in-place replace, longer-name safety, EOF-no-trailing-blank, spaced name, + 2 projection e2e — re-add collapses to one block latest-wins, and re-add leaves interleaved transactions intact); `replay_after_clear_produces_identical_file` still byte-identical (empty-file add is a pure append). Clippy clean on core `--all-targets -D warnings`. Core-only change — no app/frontend wiring, so no app rebuild/Playwright needed. **Leak-check PASS** — denylist + real-identifier word-boundary scan clean on the tracked diff and all untracked source; fixtures use only fictional/generic names (Northwind, Assets:Cash, Liabilities:Credit Card:CAD, "Wallet"/"Visa").

**App delivery + CI/CD (done 2026-06-29):**
- [x] App delivery pipeline for **all targets** (desktop + Android) + a **low-friction wireless update path** (no cable/adb loop). **DONE 2026-06-29 (AUTHORED + CI-build-PROVEN; device round-trip deferred to polish).** Public bank-free engine: generic `/updates` static route (`UPDATES_DIR`, off→404; `ServeDir`), Tauri v2 desktop updater + `createUpdaterArtifacts` + `bundle.icon` set (AppImage self-replace), custom Android OTA (`check_for_app_update`/`download_android_update` → sha256-verified APK → `InstallBridge.kt` FileProvider intent), Settings App Updates section. Private overlay: `app-release.yml` builds+signs APK (release keystore; un-throttled via opt-in `OMNI_BUILD_MEM_SAFE`) + AppImage (minisign; pubkey/endpoint via `--config` from private vars) → tailnet → `publish-update.sh` (atomic place + per-platform `latest.json` + retention `OMNI_UPDATES_KEEP`=3, prune older+`.sig`). [USER] made keystore+minisign keypair, set GH secrets/vars, prepped `/var/omni-updates`, redeployed server w/ `/updates` mount; a real run went **all green** + **verified over the tailnet** (android APK 36.7 MB 200; desktop AppImage 89 MB 200 w/ valid sig). New idempotent `deploy/provision-box.sh` (state dirs owned by `deploy` — closes a latent fresh-box gap); **DigitalOcean removed from all 3 workflows** (netcup=future backup); Tailscale on laptop + **Hetzner Cloud Firewall** (SSH-only; app server already tailnet-only). **Deferred → polish:** device-present round-trip (download → system install-over → relaunch); proper logo; full tailnet-only SSH + Tailscale-SSH ACL → netcup/HA. [L]
- [x] Safe **production-box DB reset** for polish-phase testing. **DONE 2026-06-29 (authored).** `deploy/reset-db.sh` (baseline/restore/wipe over `snapshot.sh`/`restore-snapshot.sh`/`health-gate.sh`; always safety-snapshots before restore/wipe) + one-click `db-ops.yml` (`workflow_dispatch`, reuses the deploy tailnet-join). **Still pending: the one final reset before v1 release.** [M]
- [x] **Public CI path-filtering + flaky-test fix (2026-06-29).** Replaced the fragile `contains(toJSON(...modified))` substring gate with trigger-level `paths-ignore` + a `dorny/paths-filter` `changes` job (backend/frontend outputs) gating the build jobs — no wasted recompiles (verified: a `core/`-only commit ran Build & Test, **skipped** the WASM frontend). The correct gating surfaced a **pre-existing flaky test** the old filter had been silently skipping (`journal_file::apply_appends_multiple_transactions_in_order` — `JournalFile::append` dropped the `tokio::fs::File` before its background `write(2)` completed, so the second append could vanish under a saturated runner); fixed with `flush().await` before drop (`05a2aeb`); CI green. [S]
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
