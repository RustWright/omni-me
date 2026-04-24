# Project: omni-me

**Status:** Cycle 2 Implementation — Phases 0-6 + Phase 5 + Phase 7.1/7.3 complete. Only Phase 7.2 (seconds) deferred.
**Last Updated:** 2026-04-23

## Session Log

| Session | Date | Status | Summary |
|---------|------|--------|---------|
| Session 1: Initiation | 2026-03-02 | Complete | Defined goal (all-in-one personal life app with LLM processing + data sovereignty), user (self only), success criteria (daily use by choice, adaptable), timeline (MVP end of March, prototype by September), and motivation (tax pain, lost ideas, no more excuses) |
| Session 2: Research | 2026-03-05 | Complete | All 13 features reviewed, core tech decisions finalized (Tauri v2, Dioxus, SurrealDB, Event Sourcing, Gemini Flash). Output: `research.md` |
| Session 3: Architecture | 2026-03-07 | Complete | Formalized `architecture.md` — security review, risk assessment (5 risks with mitigations) |
| Session 3: Planning (Cycle 1) | 2026-03-08 | Complete | 38 tasks across 7 phases. PWA deferred. All Claude Code. 3 parallel tracks for core infrastructure. Output: `tasks.md` |
| Session 4: Implementation | 2026-03-08 | Complete | Phase 0 POCs all passed (desktop + Android). SurrealDB, Tauri, Dioxus, CodeMirror validated. |
| Session 4: Phase 1 | 2026-03-25 | Complete | Workspace scaffold (1.1), SurrealDB connection layer (1.4), Axum server (1.5), CI/CD workflow (1.3). VPS deferred — DO rejected payment, going to Hetzner when stable. |
| Session 4: Phases 2-6 | 2026-03-29 | Complete | Event store + sync, LLM pipeline, UI shell + CodeMirror, Journal/Notes, Routine Manager — all features functional on desktop. Code review done (6 fixes applied). |
| Session 4: Phase 7 | 2026-03-30 | Complete | Tracing, editor fixes, sync wiring, settings page, Android APK, Tailscale sync. Fixed projection apply after sync pull. Found SurrealKV stability issue (commit queue overflow after ~24h). Cycle 1 complete — all features working, sync verified phone→server→desktop. |
| Session 4: Planning (Cycle 2) | 2026-04-19 | Complete | 48 tasks across 7 phases + 3 stretch. Tier 1 (editor revamp + auto-save/sync + journal/generic split + Obsidian import/export) + full Tier 2 (routine UX). Breaking event schema changes accepted. Time-of-day labels removed. Day-close: manual + auto (when 3 properties filled). Tier 3 (budget) deferred to Cycle 3. Output: `tasks.md`. |
| Session 5: Phase 0 (Cycle 2) | 2026-04-19 | Complete | Core foundation rewrite — event schema split (journal/generic), routine event expansion (reorder/remove/undo/modify), DataWiped event, frequency parser (Learn-by-Doing), notes+routines projections rewritten, auto-close background tick (chrono-tz, local midnight+30s grace), 15 new Tauri commands. 99 workspace tests. Commit `1031e23`. |
| Session 5: Phases 1-3 (Cycle 2) | 2026-04-19 | Complete | Three parallel worktree subagents merged: Track B editor (auto-wrap, checkbox widget, journal-mode timestamp, dirty/clean IPC), Track D sync (debounced buffer 1s, push debouncer 2s, exponential backoff 1→60s ±10% jitter, probe-based network monitor, retry accelerator, 4-state status reporter, kill-server integration test), Track C nav shell (responsive sidebar/bottom at 768px, Notes tab added, Journal Today/Calendar sub-tabs, Notes Recent/Search sub-tabs, sync status chip). Integration glue: frontend `SyncStatusSnapshot` mirrors backend, editor `journal_mode` prop wired through. Commits `48578ff..a0e407b`. 113+ tests passing across workspace. |
| Session 5: Phase 4 (Cycle 2) | 2026-04-20 | Complete | Calendar + Day view landed sequentially on main (Phase 3 merge had already done 4.3/4.4/4.5). `TodayView` → `DayView(date)` refactor with keyed remount; shared `selected_date` signal lets calendar clicks jump to arbitrary days with "← Back to today" link. `CalendarView` builds 6×7 Monday-first spillover grid, reactive to month nav, with per-day accent dots pulled from `list_journal_dates`. `build_month_cells` implemented via Learn-by-Doing (user wrote `std::iter::successors` variant after review feedback). Host-runnable logic test locked in — confirmed `cargo test` on default target works for non-browser code in the wasm-targeted frontend crate. Commit `957a4ce`. |
| Session 5: Phase 6 (Cycle 2) | 2026-04-20 | Complete | Tier 2 routines landed sequentially on main across two sittings. Sitting A: Data Wipe (Danger Zone in Settings, arming + typed-phrase confirmation, paste/cut/drop disabled, emits `DataWiped`) + remove buttons for items and groups (6.3, 6.4, 6.7). Sitting B: routine-item edit form (inline Edit/Save/Cancel per step, name + duration + unit) with `duration.rs` helper for min↔hour conversions — `split_minutes_for_display` implemented via Learn-by-Doing (user chose exact-divisor policy); duration unit picker added to both Add and Edit forms; frequency picker expanded with Biweekly / Monthly / Custom-N-days + clamped inline int input serialized as `custom:{N}` (6.1, 6.2, 6.5). Sitting C: drag-to-reorder routine groups on Daily Flow — `reorder.rs` pure logic module (`reorder_groups_after_drop` + `to_orderings_payload`) implemented via Learn-by-Doing (user chose asymmetric drag-up-before / drag-down-after policy after iterating on symmetric filter-trick), wired through HTML5 DnD with optimistic `pending_order` override signal and `invoke_reorder_routine_groups` (6.9). Visual testing via `dx serve --features mock` + Playwright MCP verified drag-reorder, custom-N picker, edit form, add form. |
| Session 5: Phase 7.3 + Phase 5.1 | 2026-04-22 | Complete | Notes search clear button (X inside input, Escape key also clears) — 7.3. Journal template engine: `journal_template::render(date)` primes both `initial_content` and the `content` signal on new journal open; template body user-written via Learn-by-Doing (fenced YAML, `daily_note` tag list, 3 frontmatter properties, H2 prompt heading) — 5.1. Feedback saved: reflection fields stay in YAML frontmatter (user writes single run-on sentences, uses Obsidian property panel). Also corrected stale memory that flagged 6.6 (undo complete/skip) as deferred — verified already implemented in Phase 6. Commit pending. |
| Session 5: Phase 5.2-5.8 + Phase 7.1 | 2026-04-23 | Complete | Obsidian import/export end-to-end in one autonomous run (user near weekly token cap, skipping Learn-by-Doing + Playwright verification, to revisit after reset). Added `core::import` module with parser + walker + mapper + classifier (`serde_yml` dep). `TODO(human)` slot reserved in `split_frontmatter_and_body` — placeholder impl lets downstream work. Tauri commands: `preview_import`, `commit_import` (re-reads files from disk, not UI data), `export_obsidian` (filename sanitizer strips forbidden chars). Frontend: `pages::import_export::{ImportFlow, ExportFlow}` with Idle→Scanning→Previewing→Committing→Done state machines; per-row accept/skip checkbox + editable override for title/date + has-legacy-properties indicator. Settings page wires `ImportExportSection` above Danger Zone. 18 core import tests pass including the 7.1 round-trip test. 7.2 (seconds duration) deferred to Cycle 3 — would require breaking event schema across 16 touch points. Commit pending. |
| Session 6: Phase A + test-gap audit | 2026-04-24 | Complete | 4-perspective reviews generated (Security + Logical Consistency on Opus 4.7, Performance + Bloat/Complexity on Sonnet 4.6) at `reviews/2026-04-24-*.md`. **3 Critical findings**: (1) `core/src/routines.rs:99` `Frequency::Monthly` silently skips months without the anchor day (day-31 anchor never runs in Feb/Apr/Jun/Sep/Nov); (2) `tauri-app/frontend/src/journal_template.rs:22-37` ↔ `core/src/events/notes_projection.rs:280-305` parity break — template emits `tags:` followed by `- daily_note` YAML list item, parser terminates on first non-kv line, the three reflection keys never get scanned, `is_complete` stays `false`, auto-close never fires; (3) `core/src/sync/buffer.rs:159-163` `do_flush` drains queue before `append_batch`, silently drops events on DB error (`flush_loop` swallows Result via `let _`). Regression: Cycle 1 Session 6 marked `on_item_modified` DB-query consolidation as fixed, but only `on_group_modified` was updated — `on_item_modified` still runs 3 queries per edit. Test-gap proposal at `reviews/2026-04-24-test-gaps.md` (12 proposed tests: 8 regression guards, 2 tripwires, 2 edge-case fill-ins; 2 deliberately skipped with rationale). `reviews/` is git-ignored — no commits for Phase A; `project.md` is the only tracked record of the findings. **Next:** user triages findings per-review one-by-one in fresh context → Phase B test implementation → Phase C fix cycle. |

---

## Session 1: Project Initiation

**Date:** 2026-03-02

### Questions & Answers

**1. What is the goal of the project?**
Two layers:

- **Deliverable:** A personal all-in-one app for budgeting, journaling, goals, tasks, decisions, and life management.
- **Underlying motivation:** Extreme curiosity leads to information overload and anxiety about losing track. LLMs now make it possible to not just collect personal data, but meaningfully process it. The app serves two goals: (1) accelerate personal growth by having an intelligent feedback loop on your own life, and (2) create an objective record of that growth to replace vague feelings with verifiable history. A third key driver is **data sovereignty** — every need this app addresses is served by existing products that monetize user data; building it yourself keeps that data under personal control and positions well for the coming age of AI.

**2. Who will be the primary user/consumer (who benefits)?**
Solely the user themselves. No multi-user or sharing features needed.

**3. What does success look like?**
Daily use as a first choice, not a fallback. The test is: would you reach for it naturally? Concrete example — tax season: if finances had been tracked for the past year, you could file with confidence knowing exact spend, reliable data, archived invoices/receipts/images. Two dimensions of success:
- **Usage:** It's built, used, and *wanted* — the preferred tool for everything it's intended to cover
- **Adaptability:** It can grow (and shrink) with evolving needs over time — not a static product
Core requirement underlying both: the data being collected is actually being processed in a way that serves the original purpose (growth acceleration + verifiable record).

**4. What are the time/urgency expectations?**
- **MVP:** End of March 2026 (~4 weeks)
- **Full-featured prototype:** Before September 2026 (~7 months)
- **Final product:** Doesn't exist — this is an evergreen personal tool that will always be iterated on; no planned end state.

**5. Why does this project matter to you right now? What's driving this need?**
The idea has existed for a while but now the pain is acute and the excuses are gone:
- **Active pain:** Tax season is forcing a painful document hunt that better organization would have prevented
- **Ongoing loss:** Consuming interesting books and ideas daily but losing them to distraction before implementing anything
- **Enablers removed:** A website is already set up to track development, and LLM coding agents make building this solo realistic in a way it wasn't before
- The honest answer: should have started a year ago. Starting now because there's no good reason not to anymore.

---

## Session 2: Research Session

**Date:** 2026-03-05

**Summary:** Dedicated research session before architecture decisions. Reviewed all 13 features, all service options, and finalized core technical choices. Output captured in `research.md`.

**Key Decisions Made:**
- **Deployment:** Tauri v2 (Android APK, sideloaded) + PWA fallback. Dioxus for UI, CodeMirror 6 for editor.
- **VPS:** DigitalOcean 2GB Droplet (~$12 USD/month, $200 credit). Planned Hetzner migration before credit expires.
- **CI/CD:** GitHub Actions → DigitalOcean (high priority, same workflow as personal website)
- **LLM:** Gemini Flash free API. Trait-based abstraction to add Claude API later.
- **Database:** SurrealDB (multi-model, Rust-native, schema-flexible, graph support)
- **Sync:** Event Sourcing (append-only immutable events, no conflict resolution needed, schema-flexible)
- **Editor:** CodeMirror 6 (same editor as Obsidian, excellent Android touch support, MIT)
- **Data model:** Single note type, LLM derives all structure. Tool calling + structured output for consistency.
- **LLM pipeline architecture:** Structured output mode + tool/function calling + deterministic pre-processing + prompt versioning + confidence review gates

**Feature decisions:** All 13 features reviewed. See `research.md` Section 5 for full service map.

**MVP Scope (end of March 2026):**
- Week 1: Infrastructure (Tauri + SurrealDB + Event Store + Sync + CI/CD + LLM pipeline + CodeMirror)
- Weeks 2-3: Journal/Notes feature + Routine Manager
- Validates: APK sideloading, multi-device sync, LLM pipeline

**Reference:** See `research.md` for complete research documentation

---

## Session 3: Architecture

**Date:** 2026-03-07

**Summary:** Reviewed all decisions from research session, confirmed every section unchanged. Formalized into `architecture.md` — concise, model-parseable reference document with tables and clean headers. Added security section covering VPS hardening, data-in-transit encryption, LLM data exposure (accepted risk for MVP), and backup strategy. Conducted risk review identifying 5 risks with mitigations (Tauri sideloading and SurrealDB maturity as highest priority POCs).

**Key Output:** `architecture.md` — the authoritative technical reference for implementation.

**Process Note:** Research session (between Session 1 and Session 2) proved very valuable. Consider updating `PROJECT_PROCESS.md` to include dedicated research step.

---

## Cycle 1: MVP — Infrastructure + Journal + Routines

### Session 3: Planning

**Date:** 2026-03-08

**Objective:** Build the foundational infrastructure (event sourcing, sync, LLM pipeline, Tauri/Dioxus/CodeMirror stack) and two features (Journal/Notes, Routine Manager) to validate the full vertical from mobile input to LLM derivation to multi-device sync.

**Scope:**
- Included: Risk POCs, Rust workspace, VPS provisioning, CI/CD, event store, sync protocol, LLM pipeline (Gemini), Dioxus UI shell, CodeMirror editor, Journal/Notes feature, Routine Manager feature, Android APK
- Excluded: PWA fallback (deferred to Cycle 2), all Cycle 2+ features (tasks, goals, calendar, budget, locations, meals, people, knowledge, archive)

**Key Decisions:**
- PWA fallback deferred to Cycle 2 (tight timeline, single user controls all devices)
- All Claude Code implementation (no aipack) — also validates subscription capacity
- Maximum parallelization via worktree subagents during Session 4
- VPS not yet provisioned — included as Phase 1 task

**High-Level Phases:**
0. Risk Validation POCs (Tauri Android, SurrealDB embedded, Dioxus-in-Tauri, CodeMirror-in-Tauri)
1. Project Skeleton + Infrastructure (workspace, VPS, CI/CD, DB layer, server)
2. Event Store + Sync (event model, projections, sync protocol, integration test)
3. LLM Pipeline (Gemini client, tool calling, prompt versioning, note processing)
4. UI Shell + CodeMirror (Dioxus app shell, editor IPC bridge, wrapper component)
5. Journal/Notes (create, edit, list, LLM trigger, search)
6. Routine Manager (group CRUD, items, daily checklist, editing, history)
7. Integration + Polish (sync-on-open, APK final, settings, error handling)

**Task Count:** 38 tasks across 7 phases. 3 parallel tracks during core infrastructure.

**Reference:** See `tasks.md` for detailed atomic task breakdown with dependencies and parallel execution map

---

### Session 4: Implementation

**Date Started:** 2026-03-08
**Date Completed:** [Date]

**Phase 0: Risk Validation POCs — COMPLETE (all passed)**

| POC | Desktop | Android | Key Findings |
|-----|---------|---------|--------------|
| P2: SurrealDB Embedded | PASSED | n/a | SurrealDB v3 uses `SurrealValue` derive (not serde), `select()` errors on non-existent tables |
| P1: Tauri v2 | PASSED | PASSED | Needs `withGlobalTauri: true` for IPC, `mobile_entry_point` macro for Android |
| P3: Dioxus-in-Tauri | PASSED | PASSED | Dioxus WASM renders in WebView, IPC round-trip works via `window.__TAURI__` |
| P4: CodeMirror-in-Tauri | PASSED | PASSED | esbuild bundle (590KB), bidirectional JS↔WASM interop works |

**Tooling Installed:**
- Tauri CLI v2.10.1, SurrealDB CLI, Android SDK (platform 35+36, NDK r28), Java 17
- Rust targets: wasm32-unknown-unknown, aarch64-linux-android, armv7-linux-androideabi, x86_64-linux-android, i686-linux-android
- Environment vars in `~/.bashrc`: JAVA_HOME, ANDROID_HOME, NDK_HOME

**Notes:**
- No fallbacks needed — all technology bets validated
- `wasm-opt` crashes (DWARF version mismatch) but is non-fatal
- Android APK frontend assets require manual copy to `gen/android/app/src/main/assets/` — needs build script automation
- dx 0.7.2 warns about dioxus 0.7.3 version mismatch but builds fine

---

### Session 6: Code Review

**Date:** 2026-04-11 onwards
**Status:** Complete (see `reviews/` for per-perspective findings)

**Phase A — Multi-Perspective Review:**
Four parallel review documents produced in `reviews/2026-04-11-*.md`:
- Security, logical consistency, performance, bloat/complexity
- Findings bucketed Critical / High / Medium with file:line references

**Phase B — Test Coverage Audit:**
- Identified untested branches (sync client orchestration, idempotency, event payload schemas, UserDate format)
- Added 9 locked-in tests across core + server integration + frontend
- Extracted shared test fixtures into `server/tests/common/mod.rs`

**Phase C — Fix Cycle:**
- Review fixes landed in commit `5c8af12` (bloat + logical inconsistency fixes, timezone system)
- Build pipeline + UI dev workflow fixes in commit `37172b6`

**Feedback for Next Cycle:**
- SurrealKV commit queue overflow after ~24h — watch bug, not blocking
- wasm-opt DWARF crash — only affects `--release` builds
- Sync endpoints unauthenticated — acceptable behind Tailscale for now
- UI_WORKFLOW.md process is tentative; validate during Cycle 2 first UI work

**Milestone Commit:** `80581ca` (session end 2026-04-18)

---

## Cycle 2: Daily-Usable (Obsidian-Replacement MVP)

### Session 4: Planning

**Date:** 2026-04-19

**Feedback Incorporated from Cycle 1:**
- Editor fixes (line wrapping, no line numbers) landed in Cycle 1 polish — good base for Cycle 2 auto-wrap work
- `UI_WORKFLOW.md` tentative process gets first real test in Phase 1 (editor revamp) and Phase 3 (nav shell)
- Cycle 1 backlog rolled into Cycle 2 scope: undo complete/skip, duration unit, edit routine items, remove routine items/groups → Tier 2
- Cycle 1 backlog rolled to stretch: search clear button → Phase 7
- Sync endpoints still unauthed, acceptable while behind Tailscale — no auth work this cycle

**Embedded Research:**
- Obsidian Sync settings researched via WebSearch — Obsidian defaults: ~1s debounce, passive retry, ~60s backoff cap. Cycle 2 adopts similar defaults (1s local / 2s push / 60s cap).
- Obsidian screenshots walkthrough (14 images across laptop + mobile) — enumerated actual daily-use features: auto-wrap, checkboxes, YAML frontmatter, calendar, daily notes template. Wikilinks / graph view / dataview / canvas explicitly excluded from scope.

**Objective:** Get omni-me to the point where the user would reach for it before Obsidian for daily journaling. Specifically: editor ergonomics match, auto-save/auto-sync is invisible, journal-per-day + free generic notes both work, existing Obsidian vault imports cleanly.

**Scope:**
- **Tier 1 (MVP to switch):** Editor revamp (auto-wrap, checkbox, timestamps), debounced auto-save + auto-sync with exponential backoff, note naming domain (journal vs. generic), Obsidian import/export with `legacy_properties` capture
- **Tier 2 (full):** Routine UX — undo, edit, remove, frequency expansion, duration unit, data wipe; drop time-of-day labels, replace with user-ordered flat list
- **Excluded:** Budget feature (Tier 3) deferred to Cycle 3. Sync auth deferred (still behind Tailscale). PWA fallback still deferred.

**Key Decisions:**
- **Event schema — breaking changes allowed:** pre-daily-use means old local events are test data, not production history. No `Option<>` fallback shims or kind-inference tricks. Clean event vocabulary wins over preserving ~50 throwaway records.
- **Note domain split:** Separate event types — `JournalEntryCreated/Updated/Closed/Reopened` (date-keyed, one-per-day, templated) vs. `GenericNoteCreated/Updated/Renamed` (id-keyed, user-titled, free-form). Journal entries get a `journal_id` ULID for LLM/sync aggregate references in addition to the date key.
- **Day-close triggers:** Soft-lock via `JournalEntryClosed` / `JournalEntryReopened`. Auto-trigger fires only when BOTH end-of-day has passed AND all 3 manual properties (`homework_for_life`, `grateful_for`, `learnt_today`) are filled — handles the "fill next morning" case. Manual close button always available.
- **Generic note nav:** Flat recency list + search. Tags rejected as primary nav — LLM-derived tags are too unreliable.
- **Nav shell:** Bottom tab bar (mobile) + sidebar (desktop), feature-level. Second-level tabs within feature for content (Today/Calendar for Journal, Recent/Search for Notes).
- **Import strategy — Option D "Pragmatic capture":** Known frontmatter → typed fields; unknown → `legacy_properties` JSON blob on created-note events. Handles 2022-2023 schema drift without blocking import.
- **Sync defaults (user deferred to recommendation):** 1s local debounce, 2s push debounce, 1s→60s exponential backoff with jitter, OS network events as hints (not authority). 4-state status indicator: synced/pending/retrying/offline. Editing never blocks on sync.
- **Frequency canonical format:** `"daily"` | `"weekly"` | `"biweekly"` | `"monthly"` | `"custom:N"` with shared parser.
- **Time-of-day labels removed:** morning/afternoon/evening dropped entirely. `time_of_day` field removed from `RoutineGroupCreated`; `order` field added; new `RoutineGroupReordered` event. Daily Flow shows a user-ordered flat list with drag-to-reorder.
- **Cycle 1 event data:** wiped by default at Session 5 start. Migration script available as fallback if retention wanted.

**High-Level Phases:**
0. Core foundation (Track A serial) — events + projections + commands
1. Editor revamp (Track B)
2. Sync debounce + retry (Track D)
3. Nav shell revamp (Track C)
4. Calendar + day-close UI (Track C)
5. Templates + Obsidian import/export (Tracks A+C)
6. Tier 2 routines (Tracks A+C)
7. Stretch (optional)

**Parallelization:**
Phase 0 serial (Track A foundation) → Phases 1/2/3 parallel (3 worktree agents: Track B editor, Track C nav shell, Track D sync) → Phases 4/5/6 parallel (3 agents across Tracks A+C) → Phase 7 stretch.

**Task Count:** 48 tasks across 7 phases + 3 stretch items.

**Reference:** See `tasks.md` for detailed atomic task breakdown with dependencies and parallel execution map.

---

### Session 5: Implementation

**Date Started:** [Date]
**Date Completed:** [Date]

**Planned Work:**
[Brief summary]

**Actual Work:**
[Brief summary with deviations]

**Key Commits:**
- `[commit hash]`: [description]

---

### Session 6: Code Review

**Date Started:** 2026-04-24
**Status:** Phase A + test-gap proposal complete; Phase B (tests) and Phase C (fixes) pending per-finding triage.

**Phase A — Multi-Perspective Review:**
Review documents in `reviews/2026-04-24-*.md` (directory is git-ignored; files live locally only). Scope: Cycle 2 git range `1031e23..22395f8`, ~35 commits.

Model split: Opus 4.7 for Security + Logical Consistency (cross-file invariant reasoning); Sonnet 4.6 for Performance + Bloat/Complexity (pattern-match passes).

- **Security:** 2 High (path traversal via frontend-controlled `AcceptedRow.path` in `commit_import`; unbounded `serde_yml` parse exposed to billion-laughs / deep-nesting attacks), 3 Medium (Windows reserved-name sanitizer gap, unbounded `SyncBuffer::append` queue with silent error drop, hardcoded Danger Zone phrase), 5 Positive findings, 3 Cycle-1 deferrals still deferred (CSP, CORS, Gemini query-string logging).
- **Logical consistency:** **3 Critical** — see Session Log row for details. Plus 4 Warnings (parser-ceiling desync, reorder no-dedup non-transactional, auto-close scan→emit race, DST spring-forward fallback wrong), 3 Info, 4 Cycle-1 re-checks (one partial-fix flag on `read_only` prop wiring).
- **Performance:** 5 Runtime findings (biggest: `on_item_modified` regression — Cycle 1 fix only covered `on_group_modified`; `commit_import` sequential 2N DB queries; `on_group_reordered` N queries per reorder), 1 WASM (missing `panic=abort`/`codegen-units=1` in frontend profile), 4 Verified-no-issues (retry jitter/cap, retry u32 saturating counter, `build_month_cells` O(35), calendar cache). Top-5 bang-for-buck table included.
- **Bloat/complexity:** 3 High (triplicated `append_and_apply` helper across `notes.rs`/`routines.rs`/`import.rs`; `preview_import` backend counts duplicated client-side; `body_preview` double char-iteration), 4 Medium (shadow `last_sync_timestamp` pair, `ImportPhase`/`ExportPhase` minor duplication, `split_frontmatter_and_body` asymmetric return, `COMPLETE_PROPERTIES` ↔ `KNOWN_KEYS` sync hazard), 2 Low, 5 Cycle-1 re-checks (bridge macro approaching 19 functions; `append_and_apply` escalated from deferred to High).

**Phase B — Test Coverage Audit:**
- Output: `reviews/2026-04-24-test-gaps.md` (12 proposed tests, not yet implemented).
  - Priority 1 — 8 regression guards (pass today, lock current correct behavior): Frequency parser rejects negative/overflow, parser is case-sensitive, auto-close date-underflow returns Err, auto-close skips already-closed entries, retry backoff never exceeds cap, `split_frontmatter_and_body` edge cases, `split_minutes_for_display` exact-divisor policy.
  - Priority 2 — 2 tripwires for Critical bugs: `Frequency::Monthly` day-31 current-behavior test, `extract_frontmatter_properties` YAML-list-terminates current-behavior test. Both assert today's buggy behavior and will fail automatically once fixed in Phase C.
  - Priority 3 — 2 edge-case fill-ins: `reorder_groups_after_drop` empty/single-element, `sanitize_filename` Windows forbidden chars.
  - Skipped with rationale: cross-crate parity test (needs plumbing first), sync-buffer mid-flush interleaving (belongs with Phase C fix).
- Implementation deferred: per feedback `test-gap audit proposals first`, user triages proposals before any test code lands.

**Phase C — Fix Cycle:**
- Pending. User will go through each review doc one-by-one with an LLM in a fresh context to triage each finding and decide disposition (fix / defer / keep).

**Feedback for Next Cycle:**
- `reviews/` being git-ignored means `project.md` is the durable record — keep finding summaries in the session log row, not just the review file.
- Sonnet-on-Perf+Bloat vs Opus-on-Security+Logic split worked well: Opus caught the cross-file parity bug (Critical #2), Sonnet caught the missed `on_item_modified` regression from Cycle 1. Use same split pattern next cycle.
- "Deferred" status markers from Cycle 1 are not reliable once code grows — `append_and_apply` went from 2 to 3 copies. Deferrals should carry a trip-wire condition for revisit.

**Milestone Commit:** [TBD at end of Session 6 Phase C]

---

## Cycle N: [Continue pattern as needed]

---

## Lifecycle Events

### Status Change: [Event Type - e.g., "Paused"]
**Date:** [Date]
**Reason:** [Why the state changed - e.g., "Motivation changed - need shifted to different priority"]
**Potential Resume Conditions:** [If paused, under what conditions might you resume]
**Notes:** [Additional context]

---

### Status Change: [Event Type]
**Date:** [Date]
**Reason:** [Why]
**Notes:** [Context]
