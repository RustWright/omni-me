# Tasks — Cycle 1: MVP

**Target:** End of March 2026
**Scope:** Infrastructure + Journal/Notes + Routine Manager
**Strategy:** Parallel worktree subagents, all Claude Code

---

## Phase 0: Risk Validation POCs (Days 1-2)

Go/no-go gates. Fallbacks in `architecture.md`.

- [x] **P1** Tauri v2 Android APK sideload [M] — PASSED desktop + Android
- [x] **P2** SurrealDB embedded [M] — PASSED, CRUD + file persistence with kv-surrealkv
- [x] **P3** Dioxus 0.7 WASM in Tauri [M] — PASSED, component renders + IPC round-trip
- [x] **P4** CodeMirror 6 in Tauri [M] — PASSED, editor loads + bidirectional text round-trip

**Parallel:** P1+P2 (Group A), then P3+P4 (Group B) after P1.

---

## Phase 1: Project Skeleton + Infrastructure (Days 3-5)

- [ ] **1.1** Rust workspace (`tauri-app`, `core`, `server`) [M] `depends:P1,P3` — Tauri v2 + Dioxus wired, `.gitignore`, `cargo build` passes
- [ ] **1.2** VPS provisioning [M] — DO 2GB Droplet, SSH key-only, UFW, unattended-upgrades, Tailscale, Rust, SurrealDB server (localhost)
- [ ] **1.3** CI/CD: GitHub Actions → VPS [M] `depends:1.1,1.2` — build server → SCP → systemd restart, health check
- [ ] **1.4** SurrealDB connection layer [M] `depends:P2,1.1` — `core` crate, embedded + remote modes, `events` + `sync_state` tables
- [ ] **1.5** Axum server skeleton [S] `depends:1.1,1.4` — `/health`, SurrealDB, CORS, tracing, graceful shutdown

**Parallel:** 1.1+1.4 (Group A) and 1.2 (Group B). Then 1.3 and 1.5.

---

## Phase 2: Event Store + Sync (Days 5-7)

Module: `core/src/events/`, `core/src/sync/`, `server/src/routes/sync.rs`

- [ ] **2.1** Event store data model + append [M] `depends:1.4` — `Event` struct (ULID, event_type, aggregate_id, timestamp, device_id, payload), `EventStore` trait, SurrealDB impl
- [ ] **2.2** Event type registry + validation [S] `depends:2.1` — enum of valid types, typed payload structs, validate on append
- [ ] **2.3** Projection framework [M] `depends:2.1` — `Projection` trait (apply, rebuild), `ProjectionRunner`, version tracking
- [ ] **2.4** Notes projection [S] `depends:2.3` — note_created/updated/llm_processed → `notes` read table
- [ ] **2.5** Routines projection [S] `depends:2.3` — routine_* events → `routine_groups`, `routine_items`, `routine_completions` tables
- [ ] **2.6** Sync server endpoints [M] `depends:2.1,1.5` — POST `/sync/push` + `/sync/pull`, device_id filtering, timestamp validation
- [ ] **2.7** Sync client (Tauri) [M] `depends:2.6,1.4` — pull → append → push → rebuild → update last_sync_timestamp, offline-safe
- [ ] **2.8** Sync integration test [S] `depends:2.7` — 2 devices, create on A, sync, verify on B, concurrent creates

---

## Phase 3: LLM Pipeline (Days 5-8, parallel with Phase 2)

Module: `core/src/llm/`

- [ ] **3.5** Deterministic pre-processor [S] `depends:1.1` — extract_urls, extract_dates, extract_monetary_amounts, regex + tests
- [ ] **3.1** `LlmClient` trait + GeminiClient [M] `depends:1.1` — complete(), complete_structured<T>(), reqwest, structured output, API key, rate limiting
- [ ] **3.2** Tool calling framework [M] `depends:3.1` — Tool definition, calling loop with Gemini, placeholder tools (create_tag, extract_task, assess_mood)
- [ ] **3.3** Prompt versioning [S] `depends:3.1` — versioned templates as constants, PromptRegistry, record prompt_version + model per call
- [ ] **3.4** Note processing pipeline [L] `depends:3.2,3.3,3.5,2.1` — pre-process → structured output → tool calling → emit note_llm_processed, manual trigger

**Parallel:** 3.5+3.1 start together. 3.2+3.3 after 3.1. 3.4 waits for all.

---

## Phase 4: UI Shell + CodeMirror (Days 6-9, parallel with Phases 2-3)

Module: `tauri-app/src/`, `tauri-app/assets/js/`

- [ ] **4.1** Dioxus app shell [M] `depends:P3,1.1` — bottom nav (Journal, Routines, Settings), tab switching, responsive layout
- [ ] **4.2** CodeMirror 6 bundle + IPC bridge [L] `depends:P4,4.1` — JS bundle (esbuild), createEditor/getContent/setContent/onContentChange, bidirectional IPC
- [ ] **4.3** Editor Dioxus wrapper [S] `depends:4.2` — `<Editor>` component (initial_content, on_change, read_only), lifecycle, 300ms debounce

---

## Phase 5: Journal/Notes Feature (Days 8-12)

- [ ] **5.1** Note creation flow [M] `depends:4.3,2.1,2.4` — "New Note" → editor → save → note_created event → rebuild → list
- [ ] **5.2** Note editing [S] `depends:5.1` — tap note → editor → note_updated event
- [ ] **5.3** Note list view [S] `depends:5.1` — date grouping (Today/Yesterday/This Week/Older), preview, tag count, pull-to-refresh sync
- [ ] **5.4** LLM trigger [S] `depends:5.1,3.4` — "Process with AI" button → pipeline → show derived data (tags, summary, mood, tasks)
- [ ] **5.5** Note search [S] `depends:5.1` — search bar, substring on raw_text + tags, debounced

**Parallel:** 5.2, 5.3, 5.4, 5.5 all independent after 5.1.

---

## Phase 6: Routine Manager Feature (Days 10-15)

- [ ] **6.1** Routine group CRUD [M] `depends:4.1,2.1,2.5` — list groups, "Add Group" form (name, frequency, time_of_day), routine_group_created event
- [ ] **6.2** Routine item management [M] `depends:6.1` — group detail, "Add Item" (name, duration_min), routine_item_added event
- [ ] **6.3** Daily checklist [L] `depends:6.1,6.2` — today's groups by frequency, expandable checkboxes, tap → routine_item_completed, skip option, time budget
- [ ] **6.4** Routine editing [S] `depends:6.1` — modify name/frequency/time, routine_group_modified event, optional justification
- [ ] **6.5** Routine history [S] `depends:6.3` — 7-day grid (items × days, green/red/gray)

---

## Phase 7: Integration + Polish (Days 14-17)

- [ ] **7.1** Sync-on-open [S] `depends:2.7,4.1` — wire SyncClient into Tauri setup hook, status indicator, toast errors, refresh read models
- [ ] **7.3** Android APK final [S] `depends:all` — release build, debug-signed, full test (notes, LLM, routines, sync), document sideload
- [ ] **7.4** Settings screen [S] `depends:4.1` — API key (secure storage), VPS URL, device name, last sync, "Sync Now", "Rebuild Projections"
- [ ] **7.5** Error handling + logging [M] `depends:all` — tracing throughout, Dioxus error boundaries, user-friendly messages

---

## Parallel Execution Map (Session 4)

```
Days 1-2: POCs
  [Agent A] P1 + P2 in parallel
  [Agent B] P3 + P4 in parallel (after P1)

Days 3-5: Foundation
  [Agent A] 1.1 → 1.4 → 1.5  (workspace + DB + server)
  [Agent B] 1.2               (VPS provisioning)
  [Main]    1.3               (CI/CD, after both)

Days 5-9: Core (3 parallel tracks)
  [Agent A] 2.1 → 2.2 → 2.3 → 2.4+2.5 → 2.6 → 2.7 → 2.8  (events/sync)
  [Agent B] 3.5+3.1 → 3.2+3.3 → 3.4                         (LLM)
  [Agent C] 4.1 → 4.2 → 4.3                                  (UI)

Days 8-15: Features (2 parallel tracks)
  [Agent A] 5.1 → 5.2+5.3+5.4+5.5  (Journal)
  [Agent B] 6.1 → 6.2 → 6.3 → 6.4+6.5  (Routines)

Days 14-17: Integration
  [Main] 7.1 → 7.3, with 7.4+7.5 in parallel
```

**File separation (conflict-free):**
- Events/sync agent: `core/src/events/`, `core/src/sync/`, `server/src/routes/`
- LLM agent: `core/src/llm/`, `core/src/preprocess/`
- UI agent: `tauri-app/src/`, `tauri-app/assets/`

---

## Meta: Validation Goals

- [ ] Track Claude Code subscription usage across sessions — validate whether it handles full implementation without aipack
- [ ] If limits hit, document where and plan aipack integration for Cycle 2
