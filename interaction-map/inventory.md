# Interaction inventory (generated)

Regenerate with `scripts/interaction-inventory.py > interaction-map/inventory.md`.
What the columns mean, and where this is wrong: the script's header.

## `pages/archive.rs`

### ArchivePage

| handler | label nearby | effect |
|---|---|---|
| `onclick` |  | STATE |
| `on_done` | Filed. It reads as a receipt, so it is also waiting in Finan | STATE, VIEW |
| `on_extracted` | Filed in the archive. | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_purged` |  | STATE, VIEW |
| `on_back` | Archive | STATE, VIEW |
| `on_corrected` | Archive | STATE |
| `on_open` | Archive | STATE, VIEW |
| `onclick` | + Photo | STATE, VIEW |
| `onclick` | + PDF | STATE, VIEW |
| `onchange` | All kinds | STATE |
| `onchange` | All tags | STATE |
| `onclick` | Purge \"{tag}\"… | STATE, VIEW |
| `onclick` |  | STATE, VIEW |
| `onclick` |  | STATE |
| `onclick` |  | STATE |
| `on_review` | Loading the archive… | STATE, VIEW |
| `on_open` | Couldn't load more: {msg} | STATE, VIEW |
| `onclick` |  | STATE |

### DocumentCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {doc.display_name()} | VIEW |

### DocumentDetail

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Archive | VIEW |
| `on_open` |  | VIEW |
| `on_open` | The file for this entry hasn't arrived on this device yet. | VIEW |
| `on_saved` |  | STATE |
| `on_saved` |  | STATE |

### ParentLink

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Arrived inside {name} | VIEW |

### EmailView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {kid.display_name()} | VIEW |

### RetentionPanel

| handler | label nearby | effect |
|---|---|---|
| `onchange` | Pick a tag… | STATE |
| `onclick` | give a number of days, at least 1 | STATE |
| `onclick` | Keep forever | — |
| `onclick` | Review {group.count}… | — |

### PurgeGroup

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Archive | VIEW |
| `onchange` |  | — |
| `onclick` |  | CALL confirm_document_purge, STATE |

### TagPanel

| handler | label nearby | effect |
|---|---|---|
| `on_add` |  | — |
| `on_remove` | Couldn't save tags: {msg} | — |

### FieldPanel

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Confirming… | — |
| `on_saved` |  | — |

### FieldRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Saving… | — |
| `onclick` | Cancel | STATE |
| `onclick` | {field.value} | STATE |
| `onclick` |  | — |

## `pages/assistant.rs`

### AssistantPage

| handler | label nearby | effect |
|---|---|---|
| `on_open` |  | STATE, VIEW |
| `on_open_inbox` |  | STATE, VIEW |
| `on_open_memory` |  | STATE, VIEW |
| `on_open_permissions` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |

### ThreadList

| handler | label nearby | effect |
|---|---|---|
| `on_open` | What it believes about you | — |
| `onclick` | What it believes about you | — |
| `onclick` | What it may do without asking | — |
| `on_open` | Hide archived | VIEW |
| `on_archive` | Hide archived | — |
| `on_delete` | Hide archived | — |
| `onclick` | Hide archived | STATE |
| `on_open` |  | VIEW |
| `on_archive` |  | — |
| `on_delete` |  | — |
| `on_send` |  | CALL ask_assistant, STATE, VIEW |

### NoticeRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {text} | VIEW |

### ApprovalsElsewhere

| handler | label nearby | effect |
|---|---|---|
| `on_open` | Untitled conversation | NAV |

### ThreadRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {title} | VIEW |
| `onclick` | Restore | — |
| `onclick` | Delete? | STATE |

### ProposalInbox

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Back | VIEW |
| `on_decided` | what did it used to think? | — |

### MemoryView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Back | VIEW |
| `onclick` | Hide retired | STATE |

### PermissionsView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Back | VIEW |
| `on_changed` |  | CALL list_action_records, STATE |

### PermissionRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Ask me again | — |
| `onclick` | Stop asking me about this | — |

### ThreadDetail

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Back | VIEW |
| `on_decided` |  | CALL read_thread_proposals, STATE |
| `on_send` |  | CALL ask_assistant, CALL read_assistant_thread, STATE, VIEW |

### Composer

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Ask | — |

## `pages/finances.rs`

### FinancesPage

| handler | label nearby | effect |
|---|---|---|
| `on_select` | Add | STATE, VIEW |
| `onclick` | Add | STATE, VIEW |
| `on_review_seen` |  | STATE |
| `on_resume_capture` |  | STATE, VIEW |
| `on_open_batches` |  | STATE, VIEW |
| `on_open_suggestions` |  | STATE, VIEW |
| `on_open_transactions` |  | STATE, VIEW |
| `on_open_accounts` |  | STATE, VIEW |
| `on_open_reconciliation` |  | STATE, VIEW |
| `on_open_dashboard` |  | STATE, VIEW |
| `on_open_accounts` |  | STATE, VIEW |
| `on_open_budgets` |  | STATE, VIEW |
| `on_open_recurring` |  | STATE, VIEW |
| `on_open_reconciliation` |  | STATE, VIEW |
| `on_open_balance_check` |  | STATE, VIEW |
| `on_open_query` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_open_photo` |  | STATE, VIEW |
| `on_open_pdf` |  | STATE, VIEW |
| `on_open_email` |  | STATE, VIEW |
| `on_open_manual` |  | STATE, VIEW |
| `on_open_statement_import` |  | STATE, VIEW |
| `on_open_journal_import` |  | STATE, VIEW |
| `on_done` |  | STATE, VIEW |
| `on_extracted` |  | STATE, VIEW |
| `on_done` |  | STATE, VIEW |
| `on_extracted` |  | STATE, VIEW |
| `on_done` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_open_batch` |  | STATE, VIEW |
| `on_done` | No batch selected.  | STATE, VIEW |
| `onclick` | Back to list | STATE, VIEW |
| `on_back` | No transaction selected.  | STATE, VIEW |
| `onclick` | Back to list | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_open_unmatched` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_open_txn` |  | STATE, VIEW |

### HubLinkRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {title} | — |

### NetWorthHero

| handler | label nearby | effect |
|---|---|---|
| `on_select` |  | — |

### InstitutionsCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Institutions | VIEW |

### ReviewInboxCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Auto-imported batches | — |
| `onclick` | Assistant suggestions | — |
| `onclick` | Unmatched to reconcile | — |

### RecentActivityCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Recent activity | VIEW |

### OverviewView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Resume capture in progress | — |
| `on_range` |  | STATE |
| `on_open` |  | — |
| `on_shown` |  | — |
| `on_open_batches` |  | — |
| `on_open_suggestions` |  | — |
| `on_open_reconciliation` |  | — |
| `on_open` |  | — |

### BudgetSnapshotCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Budgets | VIEW |

### AnalyzeHubView

| handler | label nearby | effect |
|---|---|---|
| `on_open` | Tools | — |
| `on_click` | Accounts | — |
| `on_click` | Recurring | — |
| `on_click` | Reconcile | — |
| `on_click` | Balance check | — |
| `on_click` | Query | — |
| `on_click` |  | — |

### AddMenuView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `on_click` | PDF | — |
| `on_click` | Email | — |
| `on_click` | Manual | — |
| `on_click` | Import | — |
| `on_click` | Import journal | — |
| `on_click` |  | — |

### CaptureTile

| handler | label nearby | effect |
|---|---|---|
| `onclick` | none | — |

### DocumentCapture

| handler | label nearby | effect |
|---|---|---|
| `onclick` | none | VIEW |
| `on_select` |  | STATE |
| `onclick` | Use shared file | — |
| `onchange` | Choose photos | — |
| `onchange` | Pick a PDF | — |
| `onchange` | document | — |
| `on_remove` |  | — |
| `on_read` |  | — |
| `onclick` | Retry | — |

### CapturePageList

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Remove | — |
| `onclick` | Read {noun} | — |

### HintRadio

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {label} | VIEW |

### EmailCapture

| handler | label nearby | effect |
|---|---|---|
| `onclick` | none | VIEW |
| `onclick` | Extract | — |
| `onclick` | Retry | — |

### TransactionForm

| handler | label nearby | effect |
|---|---|---|
| `onclick` | none | VIEW |
| `on_input` | Description | STATE |
| `onclick` | + Add posting | STATE |
| `on_input` |  | STATE |
| `onclick` | Remove | STATE |
| `onclick` | Saving… | — |

### SuggestionsView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `on_decided` | Auto-import review | STATE |

### BatchListView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `on_open` |  | — |

### BatchListRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {headline} | VIEW |

### SourceEmailPanel

| handler | label nearby | effect |
|---|---|---|
| `onclick` |  | STATE |

### BatchReviewView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back to list | VIEW |
| `onclick` | ← List | VIEW |
| `on_submit` |  | STATE |
| `on_cancel` |  | STATE |
| `on_toggle` |  | — |
| `on_edit` | The reader found no transactions in this message. Enter it f | STATE |
| `on_revert` | The reader found no transactions in this message. Enter it f | — |
| `onclick` | Remove | — |
| `on_submit` | + Add transaction | STATE |
| `on_cancel` | + Add transaction | STATE |
| `onclick` | + Add transaction | STATE |
| `onclick` |  | — |
| `onclick` | Dismiss failed: {e} | CALL dismiss_batch, STATE, VIEW |

### DraftRow

| handler | label nearby | effect |
|---|---|---|
| `onchange` | #{idx + 1} | — |
| `onclick` | Undo | — |
| `onclick` | Edit | — |

### EditableCategoryChip

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Enter | — |
| `onclick` | Save | STATE |
| `onclick` | Cancel | STATE |
| `onclick` | {c} | STATE |

### EditableTagList

| handler | label nearby | effect |
|---|---|---|
| `onclick` | #{tag} | — |
| `onclick` |  | — |
| `onclick` | + tag | STATE |

### LedgerView

| handler | label nearby | effect |
|---|---|---|
| `on_back` |  | — |
| `on_open_txn` |  | STATE |
| `on_back` | Unmatched | STATE |
| `on_mutated` | Unmatched | STATE |

### TransactionListView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `on_apply` | Failed to load transactions: {msg} | STATE |
| `on_clear` | Failed to load transactions: {msg} | STATE |
| `on_click` | Loading… | — |
| `onclick` | Load more | — |

### QueryBuilderView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `onclick` | ALL | STATE |
| `onclick` | ANY | STATE |
| `onchange` | Account | — |
| `on_input` | true | — |
| `onchange` |  | — |
| `onchange` |  | — |
| `onclick` | + Add filter | — |
| `onclick` | + Add filter | — |
| `onclick` | Run query | — |
| `on_click` |  | — |

### FilterBar

| handler | label nearby | effect |
|---|---|---|
| `on_input` | To | STATE |
| `on_input` | Account contains | STATE |
| `onclick` | Apply | — |
| `onclick` | Clear | — |

### TransactionListRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {snapshot.date} | — |
| `on_save` | Reconciled against a statement | — |
| `on_save` | Reconciled against a statement | — |

### TransactionDetailView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← List | VIEW |

### TransactionDetailBody

| handler | label nearby | effect |
|---|---|---|
| `on_cancel` |  | STATE |
| `on_saved` | delete failed: {e} | STATE |
| `onclick` | Edit | STATE |
| `on_save` | ✓ Cleared | — |
| `on_save` | ✓ Cleared | — |
| `onclick` | Cancel | STATE |
| `onclick` | Deleting… | — |
| `onclick` | Delete transaction | STATE |

### TxnFieldsForm

| handler | label nearby | effect |
|---|---|---|
| `on_input` | Description | STATE |
| `onclick` | + Add posting | STATE |
| `on_input` |  | STATE |
| `onclick` | Remove | STATE |
| `on_save` | {msg} | STATE |
| `onclick` | Cancel | — |
| `onclick` | Saving… | — |

### AccountListView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |

### AccountSummaryCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | none | STATE |
| `onclick` | {label} | STATE |

### DashboardView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `on_click` | Net worth | — |

### UnmatchedCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Unmatched balance | — |

### RecurringObligationRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | monthly | — |

### BudgetListView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `onclick` | Cancel edit | — |
| `on_input` | Target amount | STATE |
| `onchange` | {label} | STATE |
| `onclick` | Saving… | — |
| `on_edit` |  | — |
| `on_remove` |  | — |

### BudgetRowCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Edit | — |
| `onclick` | Remove | — |

### RecurringReviewView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `onclick` | Scanning… | — |
| `on_confirm` | {f} → {l} | — |
| `on_dismiss` | {f} → {l} | — |

### RecurringRowCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Hide transactions | — |
| `onclick` | Dismiss | — |
| `onclick` | Confirm | — |

### StatementImportView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `on_input` | Statement label | STATE |
| `onchange` | Chequing (no header row) | STATE |
| `onchange` | {msg} | — |
| `onclick` | Import anyway | — |

### ReconciliationReviewView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `on_cancel` |  | STATE |
| `on_confirm` |  | — |
| `onclick` | Accept {suggested.len()} suggested categories… | STATE |
| `onclick` | Accept {suggested.len()} suggested categories… | STATE |
| `on_merge` |  | — |
| `on_dismiss` | Without a pair ({row_count}) | — |
| `on_resolve` |  | — |

### BulkConfirm

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Cancel | — |
| `onclick` | {verb} {count} | — |

### NoMatchRowCard

| handler | label nearby | effect |
|---|---|---|
| `on_input` | Resolve | STATE |
| `onclick` | Resolve | — |

### CandidateCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Skip | — |
| `onclick` | Merging… | — |

### BalanceCheckFormView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `on_input` | Commodity | STATE |
| `on_input` | Checking… | STATE |
| `onclick` | Checking… | — |

### JournalImportView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back | VIEW |
| `onclick` | Previewing… | — |
| `onchange` | Apply A2 business→tag rewrite (recommended) | STATE |
| `onclick` | Committing… | — |
| `onclick` | Cancel | STATE |
| `onclick` | Back to Finances | STATE, VIEW |

### AccountRow

| handler | label nearby | effect |
|---|---|---|
| `onchange` | {account_for_classes} | — |
| `on_input` | {txn.date} · {txn.description} | — |

## `pages/import_export.rs`

### ImportFlow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Scan Vault | CALL preview_import, STATE |
| `onclick` | Cancel | STATE |
| `onclick` | error | STATE |
| `onclick` | Done | STATE |

### ImportPreviewRowItem

| handler | label nearby | effect |
|---|---|---|
| `onchange` | {row.kind} | — |

### ExportFlow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Check target | CALL preview_obsidian_export, STATE |
| `onclick` | Overwrite and export | CALL export_obsidian, STATE |
| `onclick` | Cancel | STATE |
| `onclick` | Done | STATE |

## `pages/journal.rs`

### JournalPage

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Calendar | STATE |
| `on_back_to_today` |  | STATE |
| `on_select` |  | STATE |
| `on_close` |  | STATE |

### DayView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ← Back to today | — |
| `onclick` |  | CALL get_journal_by_date, CALL reopen_journal_entry |
| `onclick` |  | — |
| `onclick` |  | CALL update_journal_entry, STATE |
| `on_change` |  | — |
| `on_change` |  | STATE |
| `on_cursor` | {status} | STATE |
| `on_ready` | {status} | STATE |

### JournalPropertiesPanel

| handler | label nearby | effect |
|---|---|---|
| `on_add` |  | — |
| `on_remove` |  | — |
| `on_input` |  | — |
| `onclick` | Raw properties | STATE |

### CalendarDrawer

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Calendar | — |
| `onclick` | Previous month | — |
| `onclick` | {month_label} | STATE |
| `onclick` | M | STATE |
| `onclick` | {day_num} | VIEW |

## `pages/notes.rs`

### NotesPage

| handler | label nearby | effect |
|---|---|---|
| `on_switch` |  | STATE |
| `on_edit` |  | STATE, VIEW |
| `on_new` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_back` | recent | STATE, VIEW |

### NotesSubNav

| handler | label nearby | effect |
|---|---|---|
| `on_select` |  | — |

### NotesListRouter

| handler | label nearby | effect |
|---|---|---|
| `on_select` |  | — |

### RecentView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | New Note | — |
| `on_click` |  | — |

### SearchView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Type to search your notes | STATE |
| `on_click` | {note.title} | VIEW |

### NoteCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {note.title} | — |

### NoteEditor

| handler | label nearby | effect |
|---|---|---|
| `onclick` |  | VIEW |
| `onclick` |  | CALL rename_generic_note, CALL update_generic_note, STATE |
| `on_change` |  | — |
| `on_change` | {status} | STATE |
| `on_cursor` | {status} | STATE |

### NotePropertiesPanel

| handler | label nearby | effect |
|---|---|---|
| `on_add` |  | — |
| `on_remove` |  | — |
| `onclick` | Raw properties | STATE |

## `pages/routines.rs`

### RoutinesPage

| handler | label nearby | effect |
|---|---|---|
| `on_manage` |  | STATE, VIEW |
| `on_add` |  | STATE, VIEW |
| `on_select` |  | STATE, VIEW |
| `on_back` |  | STATE, VIEW |
| `on_remove` |  | CALL remove_routine_group |
| `on_back` |  | STATE, VIEW |
| `on_save` |  | STATE, VIEW |
| `on_cancel` |  | STATE, VIEW |

### DailyChecklistView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Manage | — |

### ChecklistGroup

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Undo failed: {e} | CALL undo_completion, STATE |
| `onclick` | Undo failed: {e} | CALL undo_skip, STATE |
| `onclick` | Could not tick that off: {e} | CALL complete_routine_item, STATE |
| `onclick` | SKIP | STATE |
| `on_pick` | Skip failed: {e} | CALL skip_routine_item, STATE |
| `on_cancel` | No time | STATE |

### SkipReasonPicker

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {reason} | — |
| `onclick` | Cancel | — |
| `onclick` | Skip | — |

### GroupListView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Routine Library | VIEW |
| `onclick` | New Group | — |
| `onclick` | {group.name} | VIEW |
| `onclick` | Confirm? | STATE |
| `onclick` | Cancel | STATE |
| `onclick` | Remove | STATE |

### AddGroupView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | New Group | — |
| `onclick` | Saving... | CALL create_routine_group, STATE |
| `on_input` | Frequency | STATE |
| `onchange` | Daily | STATE |

### GroupDetailView

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {group_name} | VIEW |
| `onchange` | min | STATE |
| `onclick` | name | STATE |
| `onclick` | Cancel | STATE |
| `onclick` | Confirm? | CALL list_routine_items, CALL remove_routine_item, STATE |
| `onclick` | Cancel | STATE |
| `onclick` | Edit | STATE |
| `onclick` | Remove | STATE |
| `onchange` | min | STATE |
| `onclick` |  | CALL add_routine_item, STATE |

## `pages/settings.rs`

### SettingsPage

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Server configuration updated. | CALL update_server_url, STATE |
| `onclick` | Server token saved. Restart the app so background sync picks | CALL update_server_token, STATE |
| `onclick` | Sync successful: {pulled} down, {pushed} up. | CALL trigger_sync, STATE |
| `onclick` | Timezone updated. | CALL update_timezone, STATE |
| `onclick` | Error: {e} | CALL get_timezone, CALL update_timezone, STATE |

### ConfigSection

| handler | label nearby | effect |
|---|---|---|
| `on_toggle` | {e} | STATE |
| `on_change` | {e} | — |

### ConfigRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {entry.label} | — |
| `on_select` |  | — |
| `on_select` |  | — |
| `on_select` |  | — |
| `onclick` | Follow shared | — |
| `on_select` | Follow uses the shared value. Takes effect immediately. | — |

### Stepper

| handler | label nearby | effect |
|---|---|---|
| `onclick` | {label} | — |
| `onclick` | {lo}–{hi} | — |

### EditorSection

| handler | label nearby | effect |
|---|---|---|
| `on_select` | Applies to the note and journal editors. Takes effect immedi | — |

### BaseCurrencySection

| handler | label nearby | effect |
|---|---|---|
| `onchange` | Base currency set to {code}. | CALL update_base_currency, STATE |

### UpdatesSection

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Checking… | — |
| `onclick` | Working… | — |

### CacheSection

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Cleared {} from cache. | CALL clear_attachment_cache, STATE |
| `onclick` | Refresh size | — |

### DangerZone

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Wipe all data… | STATE |
| `onclick` | All local data wiped. | CALL wipe_all_data, STATE |
| `onclick` | Cancel | STATE |

### AutoImportSection

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Refresh | — |
| `onclick` | + Add source | STATE |
| `on_saved` | Saved — applied live. | STATE |
| `on_cancel` | Loading… | STATE |
| `on_edit` | Removed — applied live. | STATE |
| `on_removed` | Removed — applied live. | STATE |
| `on_toggle_paused` |  | CALL set_source_paused, STATE |
| `on_tick` |  | CALL trigger_auto_import_tick, STATE |
| `on_reauth_success` |  | CALL list_auto_import_sources, STATE |

### AccountsSection

| handler | label nearby | effect |
|---|---|---|
| `on_changed` | Liquid | CALL list_detected_accounts, STATE |

### AccountOverrideRow

| handler | label nearby | effect |
|---|---|---|
| `onchange` |  | CALL set_account_override, STATE |
| `onclick` | Liquid | CALL set_account_override, STATE |
| `onclick` | Unhide | CALL set_account_override, STATE |

### AutoImportRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Resuming… | — |
| `onclick` | Fetching… | — |
| `onclick` | Reconnect | STATE |
| `onclick` | status | CALL reauth_source, STATE |
| `onclick` | Cancel | STATE |

### ConfiguredSourceRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Edit | — |
| `onclick` | Removing… | CALL remove_source_config, STATE |

### AddSourceForm

| handler | label nearby | effect |
|---|---|---|
| `onchange` | CSV file | STATE |
| `onchange` | File has a header row | STATE |
| `onclick` | Saving… | — |
| `onclick` | Cancel | — |

## `main.rs`

### ?

| handler | label nearby | effect |
|---|---|---|
| `on_pop` |  | — |

### App

| handler | label nearby | effect |
|---|---|---|
| `on_switch` |  | STATE |
| `on_feedback` |  | STATE |
| `onclick` | none | STATE |
| `on_switch` |  | STATE |
| `on_close` |  | STATE |
| `on_feedback` |  | STATE |
| `on_close` |  | STATE |

## `components/nav.rs`

### NavDrawer

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Omni-Me | — |
| `onclick` | none | — |
| `on_activate` |  | — |

### FeedbackRow

| handler | label nearby | effect |
|---|---|---|
| `onclick` | none | — |

### SideNav

| handler | label nearby | effect |
|---|---|---|
| `onclick` | none | — |
| `on_activate` |  | — |

## `components/proposal_card.rs`

### ProposalCard

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Accept | — |
| `onclick` | Decline | — |

## `components/attachment_viewer.rs`

### AttachmentViewer

| handler | label nearby | effect |
|---|---|---|
| `onclick` | ⤢ Full screen and zoom | STATE |
| `onclick` |  | STATE |
| `on_close` | true | STATE |

### DocumentLightbox

| handler | label nearby | effect |
|---|---|---|
| `onclick` | in | — |
| `onclick` | reset | — |
| `onclick` | Close | — |
| `onclick` | outside | STATE |

## `components/feedback.rs`

### FeedbackModal

| handler | label nearby | effect |
|---|---|---|
| `onclick` | Report a problem | — |
| `onclick` | Report filed. It syncs with everything else. | — |
| `onclick` | Done | — |
| `onclick` | {e} | STATE |
| `onclick` | Cancel | — |
| `onclick` | Sending… | — |

_467 handlers._
