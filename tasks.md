# Tasks — Cycle 3: Budget Feature

**Target:** End of Cycle 3 — three of omni-me's core features (notes, routines, budget) are implemented. Cycle 4 takes them to stable-v1 polish.

**Scope:**
- **Must-have (15):** A1 multi-currency, A2 business/personal tags, A3 chart-of-accounts (audited externally pre-import), A4 investments-distinct (handled via hledger account types) · D1 import existing hledger journal, D3 account audit at import-time · C1 email body, C2 PDF (incl. paystubs), C4 in-person photo + description, C5 file-attachment storage · W1 reconciliation, W2 multi-account, W3 recurring detection, W4 budget/forecast, W5 investment value updates (folded into W2/W3 capture) · R1 financial-health glance, R2 ad-hoc queries
- **Deferred to Cycle 4 / post-v1:** R3 self-employment tracking (depends on stored data shape), R4 tax form validation reports

**Strategy:** Sequential. No parallel worktrees (per `feedback_parallel_agents_cost.md`). Subagent default model = `opus` (per `feedback_subagent_default_model.md`).

**Architecture (decided in Session 4):**
- **Drop Paisa.** Custom Dioxus UI on both mobile and desktop. Original Cycle 1 decision overwritten — Paisa UI overwhelm was likely part of prior tracking-attempt failure; mobile compat unverified.
- **Keep hledger as live engine.** Events → projections write to hledger journal file → hledger CLI for queries → Dioxus UI. The journal file is itself a projection (regenerable from events); events stay source of truth for sync/audit.
- **Per `feedback_prefer_integration_over_rewrite.md`:** integrate Mindee + Frankfurter + hledger; don't reimplement bookkeeping logic, OCR, or FX scraping.
- **Mobile UI is a subset of desktop**, split along time-sensitive (capture, glance) vs session-work (reconciliation, budget setup, import) lines.
- **A2 tag-based** business/personal separation (`type:business`/`personal` posting tags). No virtual accounts.
- **Multi-currency:** native commodity per posting; CAD base for reports; inline `@` FX rates extracted from receipts when present, Frankfurter daily `P` directives as fallback.
- **Attachments:** content-addressable server-side blob store at `/blobs/<sha256>`, on-device LRU cache (~200MB cap) with manual clear in Settings. PDF/PNG/JPEG MVP. Single attachment per transaction.
- **LLM routing:** Mindee Receipts (photos), Mindee Invoices (PDFs incl. paystubs), Gemini Flash structured-output (text + fallback). Server-side only (per `feedback_llm_server_side.md`). Verification: line-item-sum == total, gross - deductions == net, confidence threshold gates.

Size tags: [XS] ≤30min · [S] ~1h · [M] ~2-3h · [L] ~4-6h

---

## Pre-Cycle-3 (USER, separate Claude session)

User runs an external Claude session reviewing existing hledger journal files BEFORE Phase 6 import:
- Identify and fix data issues / inconsistencies
- Improve chart-of-account names
- Retroactively backfill missed months (Paisa-format)
- Output: clean hledger journal ready for Phase 6 import

This work happens **in parallel with omni-me Phases 0-5**. Phase 6 unblocks once cleanup is complete.

---

## Phase 0: Risk Validation POCs [SERIAL, gates affected phases]

Each POC has an explicit go/no-go bar. Failure pivots affected phases.

- [ ] **0.1** POC-A: hledger on Android. Try: pre-built ARM binary search, GHC cross-compile via stack/cabal, Termux package, server-side fallback as last resort. Pass = `hledger balance` runs against a sample journal from inside Tauri Android shell. Fail → Phase 1 hledger projection becomes server-side; mobile reads via API. [M]
- [ ] **0.2** POC-B: blob round-trip on Android. Tauri-side write to app data dir, HTTP POST to local server `/blobs/<sha256>`, HTTP GET retrieve, render in webview. Pass = full round-trip works. Fail → reconsider attachment design (eager sync? OSS tool?). [S]
- [ ] **0.3** POC-C: Mindee API call from mobile-captured image. Photo on Android Tauri → upload to server → server calls Mindee Receipts API → response parsed → returned to client. Pass = end-to-end. Fail → Gemini multimodal becomes primary for photos. [S]
- [ ] **0.4** Document POC outcomes; commit go/no-go decisions; replan affected phases if any failed. [XS]

**Phase 0 must complete before Phase 1.**

---

## Phase 1: Core Foundation

Event schema, projections, Tauri commands. Mirrors Cycle 2 Phase 0 structurally.

- [ ] **1.1** Event types: `TransactionRecorded { id, account, postings: Vec<Posting>, date, description, attachment: Option<AttachmentRef>, tags: Vec<Tag> }` where `Posting { account, commodity, amount, fx_rate: Option<Rate> }` and `AttachmentRef { sha256, filename, mime_type, size }` [M]
- [ ] **1.2** Event types: `TransactionCategorized`, `TransactionTagged`, `TransactionUpdated`, `TransactionDeleted` [S]
- [ ] **1.3** Event types: `BudgetSet { category, amount, period }`, `BudgetUpdated`, `BudgetRemoved` [S]
- [ ] **1.4** Event types: `AccountAdded`, `AccountReconciled { account, statement_balance, cleared_through }` [S]
- [ ] **1.5** Event types: `RecurringTransactionDetected { pattern }`, `RecurringTransactionConfirmed`, `RecurringTransactionDismissed` [S]
- [ ] **1.6** hledger journal projection: append-on-event writes valid hledger entries to journal file (per-device, regenerable from events) [L]
- [ ] **1.7** SurrealDB projection: `transactions`, `accounts`, `budgets`, `recurring_patterns` tables; idempotent apply [M]
- [ ] **1.8** Tauri commands: `record_transaction`, `update_transaction`, `categorize_transaction`, `tag_transaction`, `delete_transaction`, `list_transactions(filter)` [M]
- [ ] **1.9** Tauri commands: `add_account`, `list_accounts`, `set_budget`, `list_budgets`, `confirm_recurring`, `list_recurring` [S]
- [ ] **1.10** Multi-currency `Posting` validation (commodity required; FX rate optional but commodity-pair must match base when present) [S]
- [ ] **1.11** Unit tests for event schema, projection idempotency, hledger output validity (golden file sample) [M]

---

## Phase 2: Server-Side Capture Pipeline

LLM extraction + verification + FX rate fetch + blob storage. All server-side.

- [ ] **2.1** Axum endpoint `POST /blobs/<sha256>` — accepts file upload, validates SHA-256 matches body, stores at configured blob path. Auth via existing sync-token (deferred from Cycle 1 still applies). [S]
- [ ] **2.2** Axum endpoint `GET /blobs/<sha256>` — streams stored file with correct MIME type [XS]
- [ ] **2.3** Server-side blob storage: configurable path (default `./blobs/`), atomic write (temp file + rename), no extra DB tracking — filesystem is the index [S]
- [ ] **2.4** Mindee Receipts API integration: receipt-photo input → structured `ReceiptExtraction` (date, total, line items, tax, vendor, currency) [M]
- [ ] **2.5** Mindee Invoices API integration: PDF input → structured `InvoiceExtraction` (same shape as receipts plus invoice metadata) [M]
- [ ] **2.6** Gemini Flash structured-output extraction: prompt + JSON schema for `TransactionDraft`; handles plain text (email body, paystub paste); fallback for any input Mindee can't process [L]
- [ ] **2.7** Verification pass: line-item-sum == total (receipts), gross - deductions == net (paystubs), confidence threshold gate; flag below-threshold drafts for manual review [M]
- [ ] **2.8** Frankfurter FX daily-rate fetcher (free, ECB-sourced, no API key); emits `P` directive into hledger journal projection [M]
- [ ] **2.9** Routing logic: input type → extractor selection (Mindee primary for photos/PDFs, Gemini for text + fallback) [S]
- [ ] **2.10** Integration tests: end-to-end capture for each of the 4 input modalities against real samples [M]

---

## Phase 3: Frontend Capture Flows

Custom Dioxus screens for capture. Mobile-first for photos; desktop-first for PDFs.

- [ ] **3.1** Photo capture screen (mobile primary): camera/file picker → upload progress → wait state → confirm-draft screen [L]
- [ ] **3.2** PDF upload (desktop file picker, mobile share-target) → confirm-draft screen [M]
- [ ] **3.3** Android share-target intent for PDFs/images (Tauri Android manifest + handler) [M]
- [ ] **3.4** Email body paste screen (text area + extract button) → confirm-draft [S]
- [ ] **3.5** Manual entry form (account, date, amount, commodity, category, tags, description) [M]
- [ ] **3.6** Confirm-draft screen — extracted fields editable inline, attachment thumbnail visible, Save commits `TransactionRecorded` [M]
- [ ] **3.7** Local attachment cache: app data dir + LRU eviction (200MB cap default); fetch-on-demand from `/blobs/<sha256>` [M]
- [ ] **3.8** Settings → Cache section: "Clear Attachment Cache" button + cache size displayed [XS]

---

## Phase 4: Transactions Surface + R1 Health Glance

Read paths. Both platforms.

- [ ] **4.1** Transaction list screen (mobile + desktop): paginated, sortable, filter by date / account / tag / category [L]
- [ ] **4.2** Transaction detail view + attachment viewer (PDF render, image render) [M]
- [ ] **4.3** Inline edit category + tag from list and detail views [S]
- [ ] **4.4** Account list screen — balances per commodity, last reconciled date [M]
- [ ] **4.5** R1 financial-health glance dashboard (mobile + desktop): recurring obligations summary, can-I-afford-X widget, monthly spending vs income trend [L]
- [ ] **4.6** Recurring obligations summary widget — pulls from confirmed recurring patterns (Phase 1.5 events) [M]

---

## Phase 5: Workflows (Desktop-Heavy)

W1, W3, W4. Desktop-only in MVP per Session 4 scope split.

- [ ] **5.1** W4 budget setup screen (desktop): per-category budget target, per-cycle (monthly default; weekly / biweekly options) [M]
- [ ] **5.2** W4 actual-vs-planned view: progress bars per category, over/under indicators, period rollover behavior [M]
- [ ] **5.3** W3 recurring detection scanner: nightly background pass over event log; emits `RecurringTransactionDetected` events for repeat patterns (same vendor + amount + cadence) [M]
- [ ] **5.4** W3 confirm-suggestion UI (desktop): list of detected patterns, accept/dismiss/edit per-row [S]
- [ ] **5.5** W1 statement import: CSV upload (desktop) — parse common bank-export formats (CIBC, RBC, etc. — pick whichever user actually uses) [M]
- [ ] **5.6** W1 reconciliation match UI (desktop): two-column, statement entries vs unreconciled transactions; click-to-match; bulk match by date+amount [L]
- [ ] **5.7** W1 hledger balance check after reconciliation: shell out to `hledger balance` (or direct read), compare against statement closing balance, flag discrepancy [S]

---

## Phase 6: Import (D1 + D3) — after user pre-cleanup

Runs once user has cleaned external hledger journal in their separate session.

- [ ] **6.1** D1 hledger journal parser → emits `TransactionRecorded` events for each posting (preserve original commodities + tags) [L]
- [ ] **6.2** D3 import preview screen (desktop): shows accounts + transaction count, accept/skip per account, basic edits (rename account, drop) [M]
- [ ] **6.3** Commit import — idempotent batch event append; re-run safe (dedup by transaction hash) [S]
- [ ] **6.4** Test import against synthetic hledger journal sample [S]
- [ ] **6.5** Run user's actual cleaned journal end-to-end (after pre-cleanup session is done) [S]

---

## Phase 7: R2 + Polish + Stretch (Backlog)

Last phase; first 4 are core, rest are stretch.

- [ ] **7.1** R2 ad-hoc query builder (desktop): category × date-range × tag filter UI → tabular result [L]
- [ ] **7.2** R2 hledger CLI shell-out for native query language support (e.g., user types `expenses tag:business` and gets the hledger query result) [M]
- [ ] **7.3** Settings: base currency picker (CAD default, list of common ISO codes) [XS]
- [ ] **7.4** Settings: cache management surface (size shown, clear button — extends 3.8) [S]

**BACKLOG (stretch, only if time):**

- [ ] **7.5** Daily Flow consistency visualizer redesign — frequency-aware (was 7-day hard-coded, broken for Weekly/Biweekly/Monthly/Custom routines) [M]
- [ ] **7.6** `BufferEvent::FlushFailed` consumer wiring into `StatusReporter` for user-visible "stuck buffer" indicator [S]
- [ ] **7.7** `editor.rs:179` release-build compile error fix (`editor_options(journal_mode)` arg-count mismatch — Cycle 1 read_only fix only landed on debug branch) [XS]
- [ ] **7.8** Configurable `FORCE_GENERIC_DIRS` (currently hardcoded to `Work/`) [S]
- [ ] **7.9** `auto_close_scheduler::AppState.event_store` move to `Arc<dyn EventStore>` for parity with main store [XS]
- [ ] **7.10** Seconds duration unit on routine items (Phase 7.2 deferred from Cycle 2; needs breaking event-schema change across 16 touch points) [M]

---

## Sequential Execution Map (Session 5)

```
Phase 0 (POCs, gating):  0.1 → 0.2 → 0.3 → 0.4
Phase 1 (foundation):    1.1 → 1.2+1.3+1.4+1.5 → 1.6 → 1.7 → 1.8+1.9 → 1.10 → 1.11
Phase 2 (server):        2.1+2.2 → 2.3 → 2.4+2.5 → 2.6 → 2.7 → 2.8 → 2.9 → 2.10
Phase 3 (capture UI):    3.1, 3.2 → 3.3 → 3.4 → 3.5 → 3.6 → 3.7 → 3.8
Phase 4 (read):          4.1 → 4.2 → 4.3 → 4.4 → 4.5+4.6
Phase 5 (workflows):     5.1 → 5.2 → 5.3 → 5.4 → 5.5 → 5.6 → 5.7
Phase 6 (import):        [WAIT for user pre-cleanup] → 6.1 → 6.2 → 6.3 → 6.4 → 6.5
Phase 7 (R2+polish):     7.1 → 7.2 → 7.3+7.4, then backlog as time permits
```

---

## Cycle 4 Backlog (locked at Session 4)

Cycle 4 is dedicated polish + stable-v1 release.

- [ ] **Editor parity with Obsidian** — close remaining gap so user prefers omni-me over Obsidian for daily journaling
- [ ] **App logo** — replace default Tauri assets across desktop + Android
- [ ] **Branch-gate process** — establish post-v1 workflow: feature branches + merge gates to protect stable
- [ ] **Stable v1 version stamp** — set semver, tag release
- [ ] **C1 email auto-fetch** (vs paste) if needed; W1 mobile reconciliation; W4 mobile budget edits; R3 self-employment dashboards; R4 tax form validation reports
- [ ] **PWA fallback** — still deferred from Cycles 1+2

---

## Meta: Validation Goals

- [ ] Track Mindee free-tier usage across Cycle 3 (250 credits/month cap)
- [ ] Track Gemini Flash free-tier usage
- [ ] Validate POC outcomes feed into actual phase plan (no architecture drift between Phase 0 and Phase 1+)
- [ ] User completes external journal cleanup in time for Phase 6 (rough target: by end of Phase 4)
