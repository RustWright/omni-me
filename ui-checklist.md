# UI Interaction Checklist

Verification checklist for omni-me UI functionality.
Last tested: 2026-04-17 via Playwright MCP (dx serve --features mock)

---

## Navigation

- [x] Bottom nav shows three tabs: Journal, Routines, Settings
- [x] Active tab is visually distinguished from inactive tabs (blue vs grey)
- [x] Clicking each tab switches to the correct page
- [x] Clicking the already-active tab does nothing (no reload/flicker)

## Journal — List View (empty state)

> Skipped: mock data always returns notes. Test with real backend.

- [ ] Shows "Journal" title, "Search" button, "+ New Note" button
- [ ] Empty state shows "No notes yet" with helper text
- [ ] Search button navigates to search view
- [ ] + New Note button navigates to editor view

## Journal — List View (with notes)

- [x] Notes are grouped by date (Today / Yesterday / Older)
- [x] Each note card shows preview text, date, tag count
- [x] Clicking a note opens it in the editor for editing
- [ ] List refreshes after saving a new or edited note (mock limitation)

## Journal — Search View

- [x] Shows Back button and search input field
- [x] Empty query shows "Type to explore your thoughts" (no results, not all notes)
- [x] Typing a query shows results or "No results found"
- [x] Search fires automatically as you type
- [ ] Search state persists when navigating Back and returning (not tested)
- [ ] **MISSING: Clear/X button in search field to reset query**
- [x] Back button returns to Journal list view
- [ ] Clicking a search result opens the note for editing (not tested)

## Journal — New Note Editor

- [x] Shows Back button, "New Note" title, Save button
- [x] CodeMirror editor loads (no "Loading editor..." stuck state)
- [ ] Text wraps naturally (no horizontal scrolling) (not tested)
- [x] No line numbers displayed
- [ ] Can type freely in the editor (not tested)
- [ ] Save button saves the note and returns to list view (mock limitation)
- [ ] New note appears in the list after saving (mock limitation)
- [ ] Back with unsaved changes prompts confirmation or autosaves (not tested)
- [x] Re-entering New Note after Back starts with empty editor

## Journal — Edit Note

- [x] Editor pre-populated with existing note content
- [x] Shows "AI Analyze" button (only for saved notes)
- [x] Save button present
- [ ] Save persists changes (mock limitation)
- [ ] AI Analyze triggers server-side LLM processing (requires real backend)
- [ ] LLM results display: tags, tasks, dates, expenses, summary (requires real backend)

## Routines — Daily Checklist (empty state)

> Skipped: mock data always returns routines. Test with real backend.

- [ ] Shows "Today's Routines" title and "Manage" button
- [ ] Empty state shows "No routines yet" with helper text
- [ ] Manage button navigates to group list view

## Routines — Daily Checklist (with routines)

- [x] Groups displayed by time of day (Morning with sun icon)
- [x] Each group shows items with checkboxes
- [ ] Tapping checkbox marks item complete (visually distinct) — **mock doesn't update state**
- [x] Skip button visible for incomplete items
- [x] Progress indicator per group (1/2)

## Routines — Group List (Manage view)

- [x] Shows Back button, "Routine Library" title, "+ New Group" button
- [ ] Empty state shows "No routine groups" message (mock always has data)
- [x] Back button returns to daily checklist
- [x] + New Group navigates to new group form
- [x] Existing groups shown as cards with name, frequency, time of day
- [x] Clicking a group opens group detail view

## Routines — New Group Form

- [x] Shows Cancel (X) button, "New Group" title, Save button
- [x] Name field: free text input with placeholder ("e.g. Morning Ritual")
- [x] Frequency dropdown: Daily, Weekly, Custom
- [x] Focus Window dropdown: Morning, Afternoon, Evening
- [ ] Cancel with unsaved changes prompts confirmation or discards (not tested)
- [ ] Save creates the group and returns to group list (mock limitation)
- [x] Re-entering form after Cancel starts with empty/default values

## Routines — Edit Group

- [x] Form pre-populated with existing group values
- [x] Save button present
- [ ] Save persists changes (mock limitation)
- [ ] Cancel with unsaved changes prompts confirmation or discards (not tested)

## Routines — Group Detail

- [x] Shows group name, items list, add item form
- [x] Can add items with name and estimated duration (form present, Add button disabled when empty)
- [x] 7-day history grid at bottom ("7-Day Performance")
- [ ] Grid cells visually distinguish completed, skipped, and not done states (all grey — mock returns no history)

## Settings

- [x] Shows "Settings" title
- [x] Cloud Sync section: device ID, server URL, Sync Now button
- [x] Timezone section: auto-detected timezone display

---

## Naming Differences (checklist vs actual UI)

| Checklist | Actual UI |
|-----------|-----------|
| "Today's Routines" | "Daily Flow" |
| "Routine Groups" | "Routine Library" |
| "Time of Day" | "Focus Window" |
| "Process with AI" | "AI Analyze" |

## Missing Features

- **Search clear button**: No X/clear button in search input field

## Mock Limitations

Mock mode (`--features mock`) provides static data that doesn't persist state changes.
These items can only be fully tested with the real Tauri backend (`cargo tauri dev`):
- Saving notes (create/update)
- Completing/skipping routine items (state doesn't update)
- Creating new groups/items
- LLM processing
- Sync operations

---

## Test Environment

- **UI-only dev:** `cd tauri-app/frontend && dx serve --platform web --features mock --open false --port 8080`
- **Full app dev:** `cd tauri-app/src-tauri && cargo tauri dev`
- **Build pipeline:** `npm run dev` (editor bundle + debug WASM + copy assets)
- **Automated testing:** Playwright MCP tools (navigate, click, snapshot, screenshot)
- **Database:** SurrealDB embedded, stored in OS app data dir (`~/.local/share/com.omni-me.app/`)

---

## Cycle 2 Features

New UI verification scenarios for Cycle 2 features. To be validated during Session 5 as each phase lands.

### Editor — Input Behaviors (Cycle 2)

- [ ] Auto-wrap on `"` with text selection wraps selection in quotes
- [ ] Auto-wrap on `'` `(` `[` `{` same behavior
- [ ] Markdown emphasis auto-wrap: `*` `_` with selection
- [ ] Inline code auto-wrap: `` ` `` with selection
- [ ] Typing `- [ ]` at line start formats as checkbox
- [ ] Line timestamps appear on newline in journal mode
- [ ] Line timestamps do NOT appear in generic note mode
- [ ] Editor emits dirty signal on edit, clean signal after save

### Sync Status Indicator (Cycle 2)

- [ ] Synced state: green indicator
- [ ] Pending state: grey indicator (events buffered locally, not yet pushed)
- [ ] Retrying state: yellow indicator with backoff timer visible
- [ ] Offline state: red indicator when network unreachable
- [ ] Indicator transitions correctly on network loss (simulated)
- [ ] Indicator transitions correctly on network restore (simulated)
- [ ] Editing still works in offline state (no blocking)

### Journal — Daily Template (Cycle 2)

- [ ] New journal entry autofills date header
- [ ] `daily_note` tag auto-applied
- [ ] Three sections pre-rendered: `homework_for_life`, `grateful_for`, `learnt_today`
- [ ] Template only applies to journal kind, not generic notes

### Journal — Calendar View (Cycle 2)

- [ ] Month grid displays
- [ ] Days with journal entries show a dot
- [ ] Tap on day opens that day's journal
- [ ] Tap on day with no entry opens empty template
- [ ] Month navigation (prev/next) works

### Journal — Day-Closed (Cycle 2)

- [ ] "Close day" button on today's journal (manual trigger)
- [ ] Manual close works regardless of whether 3 properties are filled
- [ ] Auto-close does NOT fire if any of the 3 properties is empty after midnight
- [ ] Auto-close DOES fire after midnight once all 3 properties are filled (simulate by filling next morning)
- [ ] Closed day shows muted styling + "closed" badge
- [ ] Reopen button on closed journal view
- [ ] Reopening restores edit access

### Generic Notes (Cycle 2)

- [ ] "+ New Note" prompts for title
- [ ] Note list shows title, not raw text preview
- [ ] Tap title on list → rename inline (emits `GenericNoteRenamed`)
- [ ] Recency-sorted list
- [ ] Search tab filters by title + content

### Obsidian Import (Cycle 2)

- [ ] Settings → Import screen with file/folder picker
- [ ] Nested paths (e.g. `daily/2024-01-15.md`) classified as journal
- [ ] Non-date filenames classified as generic
- [ ] Diff preview shows each note: title/date/tags/body
- [ ] Accept/skip/edit actions per row
- [ ] Unknown YAML keys preserved (visible in edit view as `legacy_properties`)
- [ ] Commit creates events, visible in journal/notes lists after

### Obsidian Export (Cycle 2)

- [ ] Settings → Export button generates `.md` files
- [ ] Journal notes exported to `journal/YYYY-MM-DD.md`
- [ ] Generic notes exported to `notes/<title>.md`
- [ ] Frontmatter reconstructed (tags, dates, `legacy_properties` merged back)

### Routines — Tier 2 (Cycle 2)

- [ ] Daily Flow shows flat list of groups (no morning/afternoon/evening section headers)
- [ ] Drag-to-reorder groups on Daily Flow persists order (emits `RoutineGroupReordered`)
- [ ] New Group form has no Focus Window / time-of-day field
- [ ] Tap completed item → undo (reverts to incomplete)
- [ ] Tap skipped item → undo
- [ ] Routine item edit form: name, duration, unit, order
- [ ] Duration unit picker: min / hour
- [ ] Delete routine item (swipe or button, with confirmation)
- [ ] Delete routine group (button on group detail, with confirmation)
- [ ] Frequency picker: Daily, Weekly, Biweekly, Monthly, Custom-N-days
- [ ] Custom-N-days shows inline integer input
- [ ] Settings → "Wipe all data" button with two-step confirmation
- [ ] Data wipe clears local DB + emits `DataWiped` event

### Navigation Shell (Cycle 2)

- [ ] Mobile: bottom tab bar visible
- [ ] Desktop: sidebar visible (width > breakpoint)
- [ ] Feature tabs: Journal / Notes / Routines / Settings
- [ ] Within Journal: `Today` / `Calendar` second-level tabs
- [ ] Within Notes: `Recent` / `Search` second-level tabs
- [ ] Active tab + active sub-tab both visually distinguished
