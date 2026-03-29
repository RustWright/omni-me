# UI Interaction Checklist

Manual verification checklist for omni-me UI functionality.
Will serve as the basis for automated Playwright tests later.

Reference screenshots: `.reference/` directory

---

## Navigation

- [ ] Bottom nav shows three tabs: Journal, Routines, Settings
- [ ] Active tab is visually distinguished from inactive tabs
- [ ] Clicking each tab switches to the correct page
- [ ] Clicking the already-active tab does nothing (no reload/flicker)

## Journal — List View (empty state)

- [ ] Shows "Journal" title, "Search" button, "+ New Note" button
- [ ] Empty state shows "No notes yet" with helper text
- [ ] Search button navigates to search view
- [ ] + New Note button navigates to editor view

## Journal — List View (with notes)

- [ ] Notes are grouped by date (Today / Yesterday / Older)
- [ ] Each note card shows preview text, date, tag count, mood badge
- [ ] Clicking a note opens it in the editor for editing
- [ ] List refreshes after saving a new or edited note

## Journal — Search View

- [ ] Shows Back button and search input field
- [ ] Empty query shows "Type to search notes" (no results, not all notes)
- [ ] Typing a query shows results or "No results found"
- [ ] Search fires automatically as you type (debounced)
- [ ] Search state persists when navigating Back and returning
- [ ] Clear/X button in search field to reset query
- [ ] Back button returns to Journal list view
- [ ] Clicking a search result opens the note for editing

## Journal — New Note Editor

- [ ] Shows Back button, "New Note" title, Save button
- [ ] CodeMirror editor loads (no "Loading editor..." stuck state)
- [ ] Text wraps naturally (no horizontal scrolling)
- [ ] No line numbers displayed
- [ ] Can type freely in the editor
- [ ] Save button saves the note and returns to list view
- [ ] New note appears in the list after saving
- [ ] Back with unsaved changes prompts confirmation or autosaves
- [ ] Re-entering New Note after Back starts with empty editor

## Journal — Edit Note

- [ ] Editor pre-populated with existing note content
- [ ] Shows "Process with AI" button (only for saved notes)
- [ ] Save button persists changes
- [ ] Process with AI triggers server-side LLM processing
- [ ] LLM results display: tags, mood, tasks, dates, expenses, summary

## Routines — Daily Checklist (empty state)

- [ ] Shows "Today's Routines" title and "Manage" button
- [ ] Empty state shows "No routines yet" with helper text
- [ ] Manage button navigates to group list view

## Routines — Daily Checklist (with routines)

- [ ] Groups displayed by time of day (morning/afternoon/evening)
- [ ] Each group shows items with checkboxes
- [ ] Tapping checkbox marks item complete (visually distinct)
- [ ] Skip button marks item skipped (visually distinct from completed)
- [ ] Progress indicator per group (e.g. 3/5 done)

## Routines — Group List (Manage view)

- [ ] Shows Back button, "Routine Groups" title, "+ Add Group" button
- [ ] Empty state shows "No routine groups" message
- [ ] Back button returns to daily checklist
- [ ] + Add Group navigates to new group form
- [ ] Existing groups shown as cards with name, frequency, time of day
- [ ] Clicking a group opens group detail view

## Routines — New Group Form

- [ ] Shows Cancel button, "New Group" title, Save button
- [ ] Name field: free text input with placeholder
- [ ] Frequency dropdown: Daily, Weekly, Custom
- [ ] Time of Day dropdown: Morning, Afternoon, Evening
- [ ] Cancel with unsaved changes prompts confirmation or discards
- [ ] Save creates the group and returns to group list
- [ ] Re-entering form after Cancel starts with empty/default values

## Routines — Edit Group

- [ ] Form pre-populated with existing group values
- [ ] Save persists changes
- [ ] Cancel with unsaved changes prompts confirmation or discards

## Routines — Group Detail

- [ ] Shows group name, items list, add item form
- [ ] Can add items with name and estimated duration
- [ ] 7-day history grid at bottom
- [ ] Grid cells visually distinguish completed, skipped, and not done states

## Settings

- [ ] Shows "Settings" title
- [ ] Placeholder text displayed (not yet implemented)

---

## Known Issues

Issues found during testing are tracked in `tasks.md` (current cycle) or as Cycle 2 backlog items.
See `tasks.md` and the Cycle 2 Backlog section for details.

---

## Test Environment

- **Dev command:** `cargo tauri dev` from `tauri-app/`
- **Build pipeline:** `npm run build` (installs deps, bundles editor JS, builds WASM, copies assets)
- **Database:** SurrealDB embedded, stored in OS app data dir (`~/.local/share/com.omni-me.app/`)
- **Screenshots:** `.reference/` directory (numbered by flow)
