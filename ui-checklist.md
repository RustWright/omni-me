# UI Interaction Checklist

Verification checklist for the omni-me UI. Run per `UI_WORKFLOW.md`
(`dx serve --platform web --features mock --port 8080` + Playwright MCP), at **390px and
1280px**.

**Status: rewritten 2026-08-28 against the current UI. Only § Assistant has been run since (2026-09-10, browser/mock); every other section is still untested.**

> ⚠️ **Run `npm run copy:editor:dev` (from `tauri-app/`) before trusting a browser sweep.**
> Without `editor.bundle.js` in the dx asset dir the Journal editor never initializes, and
> the first tab switch calls the undefined `destroyEditor` through a wasm-bindgen import
> that is not marked `catch` — which **aborts the wasm**. The app then stops responding to
> every click and presents as "navigation is broken". The tell is "Initializing editor
> environment..." on the Journal page.

> **The previous results were voided, not carried forward.** This file last recorded a
> passing sweep on 2026-04-24 against a three-tab bottom nav (Journal / Routines /
> Settings) with no Notes page and no Finances page. That UI no longer exists. Every
> `[x]` in it was a true statement about a screen that has since been replaced, which
> makes it a false statement about the app — so the boxes below start unchecked rather
> than inheriting marks that were never re-verified.

Legend: `[ ]` untested · `[x]` verified in browser/mock · `[D]` **device-only — the mock
cannot exercise this**, needs a real backend.

---

## Shell and navigation

- [ ] Six destinations reachable: Journal, Notes, Routines, Finances, Assistant, Settings
- [ ] Desktop (1280px): SideNav visible, active destination visually distinguished
- [ ] Mobile (390px): SideNav is off-viewport; hamburger opens the drawer
- [ ] Drawer closes after selecting a destination
- [ ] Selecting the already-active destination does not reload or flicker
- [ ] Header auto-hides on scroll down and returns on scroll up (needs a real scroll
      range — short viewport, incremental steps; see UI_WORKFLOW.md)
- [ ] Sync status indicator renders and reflects state
- [ ] 0 app console errors (after filtering the two known dx artifacts)

## Journal

- [ ] Day view loads today's entry; CodeMirror mounts (no stuck "Loading editor…")
- [ ] Typing marks the entry dirty; save state shows Unsaved → Saving… → Saved
- [ ] Autosave fires on the 1s debounce without a manual save
- [ ] Save failure surfaces "Save failed" after the retry policy gives up
- [ ] Calendar drawer opens; month navigation wraps correctly across year boundaries
- [ ] Selecting a day in the drawer jumps to that day and replaces the visible entry
- [ ] Completion timestamp tokens (`⟦…⟧`) render concealed, not as literal text
- [D] Non-today dates load their real entry (mock returns today only)
- [D] Closed day is read-only and shows the day-complete pill
- [D] Closing a day stops autosave

## Notes

- [ ] List renders; grouping and previews correct
- [ ] Opening a note loads its body into the editor
- [ ] Title edit and body edit both mark dirty and autosave
- [ ] Save state indicator matches the journal's behaviour (shared `SaveState::derive`)
- [ ] Search returns results; empty query shows the prompt, not all notes
- [ ] Back from search preserves list position
- [ ] Tag editor adds and removes tags

## Routines

- [ ] Daily checklist renders today's items
- [ ] Tapping an item completes it and logs a timestamp
- [ ] Un-completing works
- [ ] Skip records a skip distinct from a completion
- [ ] Group list, create, edit, delete
- [ ] Delete requires the two-step "Confirm?" arm (both group and item lists)
- [ ] Group detail shows items and history
- [D] Completion state survives a restart and syncs

## Finances — Overview

- [ ] Net-worth hero renders with the history chart
- [ ] Range switcher (1M / 3M / 6M / 1Y / YTD / All) redraws the chart
- [ ] 2×2 grid renders: per-institution balances, review inbox, cash flow, recent activity
- [ ] Per-institution drill-down opens
- [ ] Add / Import action sheet opens and offers photo / PDF / email / manual + imports
- [D] Review inbox shows real pending batches

## Finances — Ledger

- [ ] Transaction list renders; master-detail works (side-by-side desktop, slide-over mobile)
- [ ] Row selection highlights and opens detail
- [ ] Inline edit mutates and persists
- [ ] Attachment fetch renders in the detail view
- [ ] Query view accepts a DSL query and returns results
- [D] Large-list scroll performance on real data (10k+ transactions)

## Finances — Analyze

- [ ] Analyze landing renders cash-flow trend + budgets snapshot
- [ ] Dashboard, Budget list, Recurring review, Accounts, Reconciliation, Balance check
      each open and render
- [ ] Trend chart axis labels stay single-line at 360px
- [D] Recurring detection proposes patterns; confirming one promotes it

## Finances — capture and import

- [ ] Capture (photo / PDF) opens the picker and shows the extraction flow
- [ ] Transaction form works for manual entry (no pending draft)
- [ ] Transaction form pre-fills from a pending draft (post-extraction confirm)
- [ ] Date fields use the in-app `DateField` popover, never the native date input
- [ ] Statement CSV import: preview then commit
- [ ] Journal (hledger) import: preview then commit
- [D] Extraction actually calls the model and returns drafts
- [D] Imported data appears after commit **and syncs to a second device**

## Assistant

⚠️ The mock answers instantly with canned prose, so it exercises the *shape* of the flow,
never latency, retrieval or real citations.

- [x] Empty state explains what the assistant is for, and that answers arrive later
- [x] Composer: Enter sends, the Ask button sends — ⚠️ Shift+Enter untested
- [x] Asking from the thread list opens the new thread immediately
- [x] The question appears in the transcript before any answer arrives
- [ ] Pending state shows "Thinking…" and clears when the answer lands
- [x] A follow-up appends to the same thread — ⚠️ that the *answer* depends on the earlier
      turns is proven at the agent layer (Stage 1, zero verbs), not through the mock
- [ ] Thread list is ordered most-recently-active first, and refetches on tab entry
- [x] Back from a thread returns to the list — ⚠️ hardware back untested (desktop)
- [x] Citations render under an answer — ⚠️ the chip shows the record *kind*; resolving the
      real title is not built yet
- [ ] A failed / turn-budget / stale answer renders its sentence, not a blank bubble
- [D] The ~90s offline hint — needs the agent genuinely stopped
- [D] A question asked with the app offline is answered once it reconnects
- [D] An answer authored on another device appears here after a pull

## Settings

- [ ] Base currency section reads and writes
- [ ] Updates section shows current version and check-for-update
- [ ] Cache section shows size and clears
- [ ] Auto-import section lists configured sources; add / edit / remove
- [ ] Accounts section lists accounts and applies overrides
- [ ] LLM provider section reads and writes config
- [ ] Obsidian import/export flow runs end to end
- [ ] Wipe requires the typed confirmation phrase; button stays disabled until it matches
- [D] Wipe actually clears events and rebuilds projections

## Cross-cutting

- [ ] Layout holds at 360px, 390px and 1280px with no horizontal overflow
- [ ] Native `<select>` controls are legible — **verify on the desktop app, not
      Playwright**; Chromium and webkit2gtk disagree here and a screenshot has lied before
- [ ] Destructive actions all require confirmation
- [ ] Long-press / touch targets usable at mobile width
- [D] Share-intent hand-off from the Android share sheet
- [D] OTA update check, download, sha256 verify, install prompt

---

## What the mock cannot tell you

Listed once so a green browser sweep is not mistaken for a green app:

- Only *today's* journal entry exists; every other date falls back to a template.
- `closed` / `complete` are never returned, so day-completion and read-only paths are
  unreachable.
- Five bridge mocks are stateful (`MOCK_ACCOUNT_OVERRIDES`, `MOCK_PAUSED`,
  `MOCK_SOURCE_CONFIGS`, `MOCK_LLM_CONFIG`, and `mock_assistant`) and re-implement backend
  semantics by hand — they can agree with the UI and still disagree with the device.
  `mock_assistant` answers instantly with canned prose: never read a latency, a citation or
  a verb list off it.
- No real latency, no real data volume, no sync.
- ⚠️ A Playwright snapshot's `[active]` marker is DOM **focus**, not the app's tab state.
  To check which tab is really active read `window.__omniCanGoBack` or the nav button
  carrying `text-obsidian-accent`.
- Native control rendering differs from webkit2gtk.
