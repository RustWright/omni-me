# UI Interaction Checklist

Verification checklist for the omni-me UI. Run per `UI_WORKFLOW.md`
(`dx serve --platform web --features mock --port 8080` + Playwright MCP), at **390px and
1280px**.

**Status: rewritten 2026-08-28 against the current UI. Only the § Assistant sections have been run since (2026-09-10, browser/mock); every other section is still untested.**

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

## Assistant — proposals and the approval inbox

⚠️ The mock records a decision and **nothing else**. A green sweep here proves the
inbox *flow*; it says nothing about whether accepting actually creates the record. Ask
anything containing the word "note" to make the mock propose one — a deterministic
trigger, because otherwise these rows are unreachable from a browser.

- [x] A proposal renders under the answer that made it, with its summary, the
      assistant's rationale, and the body it would write
- [x] Accept settles the card in place — buttons replaced by "You approved this."
- [x] Decline settles it the same way and creates nothing
- [x] The thread list shows a count banner only while something is waiting, and the
      wording is singular at 1
- [x] The banner opens a Suggestions inbox listing **only pending** proposals
- [x] Deciding the last one leaves the inbox's empty state, which says the assistant
      cannot change anything on its own
- [x] Back from the inbox returns to the thread list and the banner is gone
- [x] A thread re-opened later still shows each proposal's outcome under its own answer
- [x] 0 console errors across the whole flow; no horizontal overflow at 390px
- [ ] An irreversible proposal shows "This one cannot be undone" — **no action declares
      itself irreversible yet**, so this is unreachable, not untested
- [ ] A proposal from a newer build (unknown action) renders a decidable card —
      covered by a unit test, never seen on screen
- [D] Accepting actually creates the record — the mock cannot author events
- [D] A proposal made by the agent on the box appears here after a pull
- [D] A proposal decided on one device shows as decided on the other

## Assistant — memory and permissions

⚠️ Both screens are reached from links under the thread list. The mock's permission
fixtures include one **irreversible** action (`message.send`) deliberately: every real
action today is reversible, so without it the "cannot be granted" half of the rule is
unreachable from a browser.

- [x] "What it believes about you" lists live beliefs with confidence and review date
- [x] Evidence renders as chips under the statement
- [x] A belief drawn without opening any record says so, rather than showing nothing
- [x] "Show retired" reveals retired beliefs with the reason they were retired; hiding
      them again works
- [x] The permissions screen lists each action with its approval record
- [x] An action never proposed says "Never proposed yet", not "0%"
- [x] ⛔ An **irreversible** action shows no toggle and explains why — verified with a
      perfect 4/4 record, so evidence does not override reversibility
- [x] Granting shows "Does this without asking"; revoking returns it to asking
- [x] 0 console errors; no horizontal overflow at 390px
- [ ] A scheduled check-in renders "Asked on your behalf" — **the mock has no scheduler**,
      so this is unreachable in browser, not untested
- [D] A check-in actually fires at its hour
- [D] A granted action is carried out without reaching the inbox

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
