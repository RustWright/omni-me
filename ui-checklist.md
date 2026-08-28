# UI Interaction Checklist

Verification checklist for the omni-me UI. Run per `UI_WORKFLOW.md`
(`dx serve --platform web --features mock --port 8080` + Playwright MCP), at **390px and
1280px**.

**Status: rewritten 2026-08-28 against the current UI; not yet re-run.**

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

- [ ] Five destinations reachable: Journal, Notes, Routines, Finances, Settings
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
- Four bridge mocks are stateful (`MOCK_ACCOUNT_OVERRIDES`, `MOCK_PAUSED`,
  `MOCK_SOURCE_CONFIGS`, `MOCK_LLM_CONFIG`) and re-implement backend semantics by hand —
  they can agree with the UI and still disagree with the device.
- No real latency, no real data volume, no sync.
- Native control rendering differs from webkit2gtk.
