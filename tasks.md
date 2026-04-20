# Tasks — Cycle 2: Daily-Usable (Obsidian-Replacement MVP)

**Target:** Editor + save/sync + note domain reach the point where the user switches from Obsidian for daily journaling.
**Scope:** Tier 1 (editor revamp, auto-save + auto-sync, journal/generic split, Obsidian import/export) + Full Tier 2 (routine UX — undo, edit, remove, frequency, duration, data wipe). Tier 3 (budget) deferred to Cycle 3.
**Strategy:** Phase 0 serial (Track A foundation) → Phases 1/2/3 parallel (4 worktree agents: A wraps, B editor, C nav shell, D sync) → Phases 4/5/6 parallel → Phase 7 stretch.

Tracks:
- **Track A** — Core/events (Rust): `core/src/events/`, `core/src/projections/`, Tauri commands
- **Track B** — Editor (JS): `tauri-app/assets/js/editor.js`, `editor.bundle.js`
- **Track C** — UI shell (Dioxus): `tauri-app/frontend/src/`
- **Track D** — Sync (Rust): `core/src/sync/client.rs`, `core/src/sync/buffer.rs`, Tauri network plugin wiring

Size tags: [XS] ≤30min · [S] ~1h · [M] ~2-3h · [L] ~4-6h

---

## Phase 0: Core Foundation — Track A [SERIAL, blocks everything]

Breaking schema changes. No backwards-compat shims — old Cycle 1 events are test data.

- [x] **0.0** Decide: wipe vs. migrate Cycle 1 events. **Decision: WIPE** (2026-04-19) — user plans to regenerate test data while dogfooding the new MVP. Skip Task 0.12 (migration script). [XS]
- [x] **0.1** Replace `NoteCreated`/`NoteUpdated` with split event types: `JournalEntryCreated/Updated/Closed/Reopened` + `GenericNoteCreated/Updated/Renamed`. Rewrite enum in `core/src/events/types.rs` [M]
- [x] **0.2** `NoteLlmProcessed` updated to take `aggregate_id` (works for both journal_id and note_id) [XS]
- [x] **0.3** Replace `RoutineGroupCreated` payload: drop `time_of_day`, add `order` [S]
- [x] **0.4** Add `RoutineGroupReordered`, `RoutineGroupRemoved`, `RoutineItemModified`, `RoutineItemRemoved` events [S]
- [x] **0.5** Add `RoutineItemCompletionUndone`, `RoutineItemSkipUndone` events [XS]
- [x] **0.6** Add `DataWiped` event [XS]
- [x] **0.7** Frequency canonical parser (`"daily"` | `"weekly"` | `"biweekly"` | `"monthly"` | `"custom:N"`) — shared helper in `core/src/routines.rs` [S] — **user-contributed via Learn by Doing**; monthly typo + `contains`→`starts_with` tightening applied in review.
- [x] **0.8** Rewrite `NotesProjection`: two read tables (`journal_entries` keyed by date with `journal_id`, `generic_notes` keyed by `note_id`). Apply new event types; compute `complete: bool` from 3-property parse; apply close/reopen. [L]
- [x] **0.9** Update `RoutinesProjection`: drop `time_of_day`, add group `order`, support modify/remove/undo/reorder + frequency parser [M]
- [x] **0.10** Auto-close background tick: scheduler runs at local midnight, scans `journal_entries` for `complete: true AND NOT closed`, emits `JournalEntryClosed { trigger: Auto }` for each [M] — core logic in `core/src/auto_close.rs`, Tauri scheduler in `tauri-app/src-tauri/src/auto_close_scheduler.rs` (chrono-tz for zone-aware sleep).
- [x] **0.11** Tauri commands: `create_journal_entry`, `update_journal_entry`, `close_journal_entry`, `reopen_journal_entry`, `create_generic_note`, `update_generic_note`, `rename_generic_note`, `reorder_routine_groups`, `modify_routine_item`, `remove_routine_item`, `remove_routine_group`, `undo_completion`, `undo_skip`, `wipe_all_data` [L]
- [x] **0.12** ~~If migration chosen at 0.0: one-time migration script~~ — SKIPPED per 0.0 decision to wipe
- [x] **0.13** Unit tests for new events + projection idempotency + completeness detection + auto-close tick [L] — 99 workspace tests passing (84 core unit + 3 scheduler + 4 sync-client + 5 sync + 3 app).

**Phase 0 complete → unblocks Phases 1, 2, 3.** ✓ 2026-04-19

---

## Phase 1: Editor Revamp — Track B [PARALLEL with 2, 3]

- [x] **1.1** Auto-wrap extension for `"` `'` `(` `[` `{` `*` `_` `` ` `` [M] — `EditorState.transactionFilter`. Single-quote contraction rule: skip auto-pair when preceded by a word character (so `don't` types naturally).
- [x] **1.2** Checkbox shortcut: `- [ ]` at line start → formatted checkbox [S] — `Decoration.replace` widget, click toggles.
- [x] **1.3** Line timestamp extension (journal-mode only, toggleable) [M] — `options.journalMode` flag + Enter-at-EOL keymap inserts `HH:MM ` prefix.
- [x] **1.4** Editor emits `dirty` / `clean` signals to Dioxus via IPC for debouncer wiring [S] — `window.editorEvents.{onDirty, onClean, isDirty}` + `window.markClean()`.
- [x] **1.5** Bundle rebuild + integration test (Playwright MCP) [S] — bundle regenerated (~1MB unminified). Auto-wrap + checkbox + timestamp + dirty/clean verified in-browser.

---

## Phase 2: Sync Debounce + Retry — Track D [PARALLEL with 1, 3]

- [x] **2.1** Debounced local append: `core/src/sync/buffer.rs` — 1s idle window, coalesces bursts via `tokio::sync::Notify` [M]
- [x] **2.2** Debounced sync push: `core/src/sync/pusher.rs` — 2s after buffer flush triggers `SyncClient::push_only` [S]. Client decomposed into `pull_only`/`push_only`/`last_sync_timestamp` primitives.
- [x] **2.3** Exponential backoff retry: `core/src/sync/retry.rs` — curve 1→2→4→8→16→32→60s cap, ±10% jitter via `rand` [M]
- [x] **2.4** OS network event listener: `core/src/sync/network.rs` — probe-based, edge-triggered. Android native `ConnectivityManager.NetworkCallback` deferred (TODO in-file); probe works cross-platform. [M]
- [x] **2.5** Wire OS events to retry accelerator: `core/src/sync/accelerator.rs` — `Online` event → `RetryEngine::hint()` cuts long sleep, does NOT reset attempt counter [S]
- [x] **2.6** Sync status reporter: `core/src/sync/status.rs` — 4-state `SyncStatus::{Idle, Syncing, Retrying, Error}` (kebab-case) + `SyncStatusSnapshot { status, retry_attempt, last_error }`. Tauri command `get_sync_status` in `tauri-app/src-tauri/src/commands/sync.rs`. [S]
- [x] **2.7** Integration test `server/tests/sync_phase2_integration.rs::kill_server_edit_queue_retry_recover` — full scenario passes [M]

---

## Phase 3: Navigation Shell Revamp — Track C [PARALLEL with 1, 2]

- [x] **3.1** Bottom tab bar (mobile) + sidebar (desktop) layout [M] — responsive at 768px via Tailwind `md:` prefix. Single rsx tree — `hidden md:flex` / `md:hidden` split.
- [x] **3.2** Feature-level tabs: Journal / Notes / Routines / Settings [S] — `Tab::Notes` added; `pages/notes.rs` created (374 lines).
- [x] **3.3** Second-level tabs within Journal: `Today` / `Calendar` (Calendar stub for Phase 4) [S]
- [x] **3.4** Second-level tabs within Notes (generic): `Recent` / `Search` [S] — Search respects empty-query=empty-result preference.
- [x] **3.5** Sync status indicator component (4-state, in header) [S] — polls `invoke_get_sync_status` every 5s, graceful fallback on error.

**Cross-track integration (commit `48b3981`):**
- `SyncStatusSnapshot` mirrored in frontend `types.rs`; fixed Track C's `SyncState`-only deserializer which silently fell through to Idle on real backend.
- Editor `js_create_editor` binding extended with `options: JsValue` 4th arg; `Editor` component gained `journal_mode: bool` prop; journal.rs passes `journal_mode: true`. `window.markClean` exposed via `js_mark_editor_clean` for future auto-save wiring.

**Phases 1-3 complete → unblocks Phases 4, 5, 6.** ✓ 2026-04-19

---

## Phase 4: Calendar + Day-Close UI — Track C [PARALLEL with 5, 6]

- [x] **4.1** Month calendar grid component with dots for days with journal entries [M] — `CalendarView` in `journal.rs` with Mon-first 6×7 spillover grid. `build_month_cells` built via Learn-by-Doing (`std::iter::successors` pattern). Host-runnable test locked in.
- [x] **4.2** Tap calendar day → open that day's journal [S] — `TodayView` → `DayView(date)` refactor. Shared `selected_date` signal + keyed remount; calendar click switches sub-tab and jumps. "← Back to today" link when viewing non-today.
- [x] **4.3** Day-closed visual state (muted styling, "closed" badge) [S] — landed in Phase 3 merge.
- [x] **4.4** Reopen button on closed journal view [S] — landed in Phase 3 merge.
- [x] **4.5** "Close day" button on journal view (manual trigger) [S] — landed in Phase 3 merge; works for any open entry, not just today.

**Phase 4 complete.** ✓ 2026-04-20

---

## Phase 5: Templates + Obsidian Import/Export — Tracks A+C [PARALLEL with 4, 6]

- [ ] **5.1** Journal template engine: autofill date, `daily_note` tag, 3 fields — rendered into editor on new journal [M] `depends:0.11,1.4`
- [ ] **5.2** Obsidian import parser: walk directory, parse YAML frontmatter + markdown body [M] `depends:0.1`
- [ ] **5.3** Frontmatter mapper: known fields → typed, unknown → `legacy_properties` blob [S] `depends:5.2`
- [ ] **5.4** Path classifier: nested path → `Journal` if date-like filename, else `Generic` [S] `depends:5.2`
- [ ] **5.5** Import diff preview UI: per-row accept/skip/edit [L] `depends:5.3,5.4,3.2`
- [ ] **5.6** Import commit: batch emit `JournalEntryCreated` / `GenericNoteCreated` events via Tauri command [M] `depends:5.5,0.11`
- [ ] **5.7** Obsidian export: projection → markdown + frontmatter files, nested paths preserved [M] `depends:0.8`
- [ ] **5.8** Import/Export settings screen entries [S] `depends:5.6,5.7,3.2`

---

## Phase 6: Tier 2 Routines — Tracks A+C [PARALLEL with 4, 5]

- [ ] **6.1** Routine item edit form (name, duration + unit, order) [M] `depends:0.11,3.2`
- [ ] **6.2** Duration unit picker (min / hour), store as normalized minutes [S] `depends:6.1`
- [ ] **6.3** Routine item remove (button with confirmation) [S] `depends:0.11,3.2`
- [ ] **6.4** Routine group remove (button with confirmation on group detail view) [S] `depends:0.11`
- [ ] **6.5** Frequency picker expansion: Biweekly, Monthly, Custom-N-days with inline int input [M] `depends:0.7,3.2`
- [ ] **6.6** Undo complete/skip UI (tap completed item reverts) [S] `depends:0.11`
- [ ] **6.7** Settings → Data Wipe (two-step confirmation, emits `DataWiped`, clears local DB) [M] `depends:0.11,3.2`
- [ ] **6.8** Daily Flow screen rewrite: remove time-of-day section headers, render flat user-ordered list of groups (respect `order` field from projection) [M] `depends:0.9,3.2`
- [ ] **6.9** Drag-to-reorder groups on Daily Flow (emits `RoutineGroupReordered`) [M] `depends:6.8`

---

## Phase 7: Stretch [IF TIME]

- [ ] **7.1** Round-trip test (import → export → re-import, verify byte-stable for supported frontmatter)
- [ ] **7.2** Duration unit: seconds option
- [ ] **7.3** Search clear button (X in search input) — Cycle 1 backlog item

---

## Parallel Execution Map (Session 5)

```
Phase 0 (Track A, serial): 0.0 → 0.1 → 0.2+0.3 → 0.4+0.5+0.6+0.7 → 0.8+0.9 → 0.10 → 0.11 → 0.12 → 0.13

Phases 1/2/3 (parallel, 3 agents):
  [Agent B] 1.1 → 1.2+1.3+1.4 → 1.5
  [Agent D] 2.1 → 2.2 → 2.3+2.4 → 2.5+2.6 → 2.7
  [Agent C] 3.1 → 3.2 → 3.3+3.4+3.5

Phases 4/5/6 (parallel, 3 agents):
  [Agent C1] 4.1+4.3 → 4.2+4.4+4.5
  [Agent C2/A1] 5.2 → 5.3+5.4 → 5.5 → 5.6, plus 5.1 and 5.7 in parallel, then 5.8
  [Agent C3/A2] 6.1→6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 6.8→6.9

Phase 7 (main): 7.1, 7.2, 7.3 as time permits
```

**File separation (conflict-free):**
- Track A: `core/src/events/`, `core/src/projections/`, `tauri-app/src-tauri/src/commands*.rs`
- Track B: `tauri-app/assets/js/editor.js`, `tauri-app/assets/js/editor.bundle.js`
- Track C: `tauri-app/frontend/src/`
- Track D: `core/src/sync/`, Tauri network plugin wiring

---

## Cycle 3 Backlog

- [ ] **Tier 3: Budget feature** — hledger + Paisa + OCR (Mindee) — deferred from Cycle 2
- [ ] **Auth on sync endpoints** — still deferred while Tailscale-only
- [ ] **UI testing workflow** — validate `UI_WORKFLOW.md` (Playwright MCP) as real process vs. tentative
- [ ] **SurrealKV stability** — commit queue overflow after ~24h, decide restart/reconnect strategy
- [ ] **Sync push error handling** — server returns 200 even when appends fail; fix to propagate errors
- [ ] **PWA fallback** — deferred from Cycle 1 + 2

---

## Meta: Validation Goals

- [ ] Track Claude Code subscription usage across Cycle 2 sessions
- [ ] Validate `UI_WORKFLOW.md` process on first Cycle 2 UI work (Phase 1 editor + Phase 3 nav)
- [ ] If subscription hits limits, document where and plan aipack integration for Cycle 3
