# Project: omni-me

**Status:** Active
**Last Updated:** 2026-03-02

## Session Log

| Session | Date | Status | Summary |
|---------|------|--------|---------|
| Session 1: Initiation | 2026-03-02 | Complete | Defined goal (all-in-one personal life app with LLM processing + data sovereignty), user (self only), success criteria (daily use by choice, adaptable), timeline (MVP end of March, prototype by September), and motivation (tax pain, lost ideas, no more excuses) |

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

## Cycle 1: [Cycle Name/Goal - e.g., "MVP Implementation"]

### Session 3: Planning

**Date:** [Date]

**Objective:** [What this cycle aims to accomplish]

**Scope:** [What's included and what's excluded]

**High-Level Phases:**
1. [Phase 1 - e.g., "Set up project structure"]
2. [Phase 2 - e.g., "Implement core functionality"]
3. [Phase 3 - e.g., "Add basic error handling"]

**Reference:** See `tasks.md` for detailed atomic task breakdown

---

### Session 4: Implementation

**Date Started:** [Date]
**Date Completed:** [Date]

**Planned Work:**
[Brief summary of what was planned in Session 3]

**Actual Work:**
[Brief summary of what was actually accomplished - note any deviations from plan]

**Key Commits:**
- `[commit hash]`: [brief description]
- `[commit hash]`: [brief description]
- `[commit hash]`: [brief description]

**Notes:**
[Any blockers, surprises, or important observations]

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
