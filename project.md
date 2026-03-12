# Project: omni-me

**Status:** Active
**Last Updated:** 2026-03-07

## Session Log

| Session | Date | Status | Summary |
|---------|------|--------|---------|
| Session 1: Initiation | 2026-03-02 | Complete | Defined goal (all-in-one personal life app with LLM processing + data sovereignty), user (self only), success criteria (daily use by choice, adaptable), timeline (MVP end of March, prototype by September), and motivation (tax pain, lost ideas, no more excuses) |
| Session 2: Research | 2026-03-05 | Complete | All 13 features reviewed, core tech decisions finalized (Tauri v2, Dioxus, SurrealDB, Event Sourcing, Gemini Flash). Output: `research.md` |
| Session 3: Architecture | 2026-03-07 | Complete | Formalized `architecture.md` — security review, risk assessment (5 risks with mitigations) |
| Session 3: Planning (Cycle 1) | 2026-03-08 | Complete | 38 tasks across 7 phases. PWA deferred. All Claude Code. 3 parallel tracks for core infrastructure. Output: `tasks.md` |
| Session 4: Implementation | 2026-03-08 | In Progress | Phase 0 POCs all passed (desktop + Android). SurrealDB, Tauri, Dioxus, CodeMirror validated. |

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

### Session 5: Testing & User Catchup

**Date:** [Date]

#### Phase A: User Test Writing
**Areas Tested:**
- [Area/module 1]
- [Area/module 2]
- [Area/module 3]

#### Phase B: Test Review
**Test Coverage Review:**
- Gaps identified: [Any coverage gaps found during review]
- Additional tests added: [Tests added to address gaps]

**Feedback for Next Cycle:**
- [Improvement or feature to consider]
- [Issue discovered that needs addressing]
- [Architectural concern to revisit]

**Milestone Commit:** `[commit hash]`

---

## Cycle 2: [Cycle Name/Goal]

### Session 3: Planning

**Date:** [Date]

**Feedback Incorporated from Previous Cycle:**
- [Feedback item from Cycle 1 and how it's being addressed]

**Objective:** [What this cycle aims to accomplish]

**Scope:** [What's included and excluded]

**High-Level Phases:**
1. [Phase 1]
2. [Phase 2]
3. [Phase 3]

**Reference:** See `tasks.md` for detailed atomic task breakdown

---

### Session 4: Implementation

**Date Started:** [Date]
**Date Completed:** [Date]

**Planned Work:**
[Brief summary]

**Actual Work:**
[Brief summary with deviations]

**Key Commits:**
- `[commit hash]`: [description]

---

### Session 5: Testing & User Catchup

**Date:** [Date]

#### Phase A: User Test Writing
**Areas Tested:**
- [Area 1]
- [Area 2]

#### Phase B: Test Review
**Test Coverage Review:**
- Gaps identified: [Gaps]
- Additional tests added: [Tests]

**Feedback for Next Cycle:**
- [Feedback items]

**Milestone Commit:** `[commit hash]`

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
