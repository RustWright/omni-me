# Tasks — Cycle 3: Budget Feature

**Target:** End of Cycle 3 — three of omni-me's core features (notes, routines, budget) are implemented. Cycle 4 takes them to stable-v1 polish.

**Status:** Phase 0 POCs closed 2026-05-09. Plan revised post-POC 2026-05-15 (this document is the canonical revised plan). Phase 1 unblocked.

**Scope:**
- **Must-have (15):** A1 multi-currency, A2 business/personal tags, A3 chart-of-accounts (audited externally pre-import), A4 investments-distinct (handled via hledger account types) · D1 import existing hledger journal, D3 account audit at import-time · C1 email body, C2 PDF (incl. paystubs), C4 in-person photo + description, C5 file-attachment storage · W1 reconciliation, W2 multi-account, W3 recurring detection, W4 budget/forecast, W5 investment value updates (folded into W2/W3 capture) · R1 financial-health glance, R2 ad-hoc queries
- **NEW in scope post-replan:** 3 auto-import paths (WealthSimple unofficial API, Wise official API, Email IMAP poller dual-purposed for Standard Chartered NGN statements + online-purchase receipts) — promoted from "post-MVP nice-to-have" because manual sync killed the prior tracking attempt.
- **Deferred to Cycle 4 / post-v1:** R3 self-employment, R4 tax form validation, Veryfi `DocumentExtractor` impl (Gemini multimodal is sole Cycle 3 extractor), ExchangeRate-API auto-rates for NGN, LLM-translated NL queries for R2.

**Strategy:** Sequential. No parallel worktrees (per `feedback_parallel_agents_cost.md`). Subagent default model = `opus` (per `feedback_subagent_default_model.md`).

**Architecture (post-replan 2026-05-15):**
- **Path B — pure-Rust embedded PTA engine.** `ledger-parser` v6 + `ledger-utils` v0.6 in-process. No hledger CLI anywhere. Validated desktop + Android against user's 5,826-transaction production journal in POC 0.1b/0.1c.
- **Drop Paisa.** Custom Dioxus UI on both mobile and desktop.
- **Journal file is per-device, regenerable from events.** Events stay source of truth (sync, audit, replay). File is a local cache projection — matches Notes/SurrealDB pattern.
- **A2 tag-only with Phase 6 import rewriter.** Existing journal uses `Expenses:Business:*` hierarchy; Phase 6.6 walks parsed postings, strips the `Business:` segment, emits `type:business` posting tag. Past "business on personal card" transactions stay untagged until user retags in normal use.
- **Per `feedback_prefer_integration_over_rewrite.md`:** integrate `ledger-utils` + Frankfurter + Gemini; don't reimplement bookkeeping logic, OCR, or FX scraping.
- **Mobile UI is a subset of desktop**, split along time-sensitive (capture, glance) vs session-work (reconciliation, budget setup, import) lines.
- **Multi-currency:** native commodity per posting; CAD base for reports; inline `@` FX rates extracted from receipts when present; Frankfurter daily `P` directives as fallback for CAD/USD/EUR; **NGN = manual entry per Standard Chartered statement** at import review (stored as hledger `P` directive).
- **Attachments:** content-addressable server-side blob store at `/blobs/<sha256>` (PUT/GET shipped in POC 0.2); on-device LRU cache (~200MB cap) with manual clear in Settings. PDF/PNG/JPEG MVP. Single attachment per transaction.
- **Document extraction:** Gemini Flash multimodal sole extractor in Cycle 3. `DocumentExtractor` trait + routing scaffold stays in code so Veryfi swap-in is a single-implementation drop-in in Cycle 4. Routing policy: hybrid (IMAP sender-based + photo defaults to receipt + PDF requires user pick + email body → Gemini text mode). Server-side only (per `feedback_llm_server_side.md`). Verification: line-item-sum == total, gross - deductions == net, confidence threshold gates.
- **Auto-import:** WealthSimple unofficial API (covers 95% of user volume), Wise official API (multi-currency), Email IMAP poller for Standard Chartered NGN monthly statements (password-protected PDF decrypt → Gemini multimodal) AND online-purchase email receipts.

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

## Phase 0: Risk Validation POCs [DONE 2026-05-09]

- [x] **0.1** POC-A: hledger on Android. **PIVOTED — Path B chosen.** Mini-research POC 0.1.0 found `ledger-parser` v6 + `ledger-utils` v0.6 pure-Rust path; POC 0.1b parsed user's 5,826-txn production journal cleanly; POC 0.1c cross-compiled to `aarch64-linux-android` and produced byte-identical results on Galaxy S9. hledger CLI no longer required. [M]
- [x] **0.2** POC-B: blob round-trip on Android. Tauri-side SHA-256 + `PUT/GET /blobs/{hash}` over Tailscale on Galaxy S9 → 201 PUT + 200 GET, bytes match. Tasks 2.1/2.2/2.3 landed as part of this POC. [S]
- [x] **0.3** POC-C (REFRAMED 2026-05-09): curl-validate against real receipt. Veryfi general endpoint mis-classified brokerage statement; Gemini 2.5 Flash multimodal PASS on FHSA statement; Nanonets DROPPED. Final: Gemini multimodal as sole Cycle 3 extractor; Veryfi deferred to Cycle 4; PaddleOCR backlog escape hatch (7.11). [XS]
- [x] **0.4** Document POC outcomes; commit go/no-go decisions; replan affected phases. All decisions in `project.md` session log + `plans/2026-05-15-cycle-3-replan.md` + `MEMORY.md`. [XS]

---

## Phase 1: Core Foundation (~13 tasks)

Event schema, projections, Tauri commands. Mirrors Cycle 2 Phase 0 structurally. **Adjustments vs Session 4:** drop top-level `account` from `TransactionRecorded` (postings carry their own); tags live on `Posting`, not transaction; decide `amount` representation up front (Learn-by-Doing on Posting struct); codify A2 helpers here even though rewriter runs in Phase 6; wire `ledger-utils` into core as in-process query primitive.

- [ ] **1.1** Event type: `TransactionRecorded { id, postings: Vec<Posting>, date, description, attachment: Option<AttachmentRef> }` where `Posting { account, commodity, amount, fx_rate: Option<Rate>, tags: Vec<Tag> }` and `AttachmentRef { sha256, filename, mime_type, size }`. Likely **Learn-by-Doing on `amount` representation** (rust_decimal vs string vs other) — cardinal data-type decision rippling through every downstream phase. [M]
- [ ] **1.2** Event types: `TransactionCategorized`, `TransactionTagged`, `TransactionUpdated`, `TransactionDeleted` [S]
- [ ] **1.3** Event types: `BudgetSet { category, amount, period }`, `BudgetUpdated`, `BudgetRemoved` [S]
- [ ] **1.4** Event types: `AccountAdded`, `AccountReconciled { account, statement_balance, cleared_through }` [S]
- [ ] **1.5** Event types: `RecurringTransactionDetected { pattern }`, `RecurringTransactionConfirmed`, `RecurringTransactionDismissed` [S]
- [ ] **1.14** Event type: `TransactionsMerged { primary_id, merged_ids: Vec<TxnId>, combined_postings: Vec<Posting>, combined_description, combined_attachment: Option<AttachmentRef>, balancing_posting: Option<Posting> }`. Supports unified reconciliation (Phase 5.6/5.7). Lineage-preserving: original `TransactionRecorded` events stay in the log; projection (1.7) shows the merged result. `balancing_posting` slot exists for hidden-fee resolution (e.g., user adds Expenses:Bank-Fees +$1.50 to close a non-zero `Unmatched` balance after merging a WS→Wise transfer). [S]
- [ ] **1.15** Event type: `TransactionCleared { txn_id, statement_source: String, cleared_date: Date }`. Records that an authoritative source (statement import OR user-confirmed paper-statement reconciliation) has confirmed a transaction occurred. Orthogonal to `TransactionsMerged` — a transaction can be cleared without merge (statement row with no pre-existing match), merged without clearing (auto-import-only sources), both, or neither. Projection (1.7) flips a `cleared` boolean on the corresponding row. [S]
- [ ] **1.6** Journal-file projection: append-on-event writes valid hledger entries to per-device journal file (regenerable from events; matches Notes/SurrealDB pattern) [L]
- [ ] **1.7** SurrealDB projection: `transactions`, `accounts`, `budgets`, `recurring_patterns` tables; idempotent apply. Pattern mirror of `notes_projection.rs`/`routines_projection.rs`. Merge semantics: `TransactionsMerged` (1.14) atomically supersedes the `merged_ids` rows with a single projected row carrying `combined_postings`; original event rows preserved in the event log for audit. Cleared semantics: `TransactionCleared` (1.15) flips a `cleared` boolean on the corresponding row, recording `statement_source` + `cleared_date` for audit. [M]
- [ ] **1.8** Tauri commands: `record_transaction`, `update_transaction`, `categorize_transaction`, `tag_transaction`, `delete_transaction`, `list_transactions(filter)` [M]
- [ ] **1.9** Tauri commands: `add_account`, `list_accounts`, `set_budget`, `list_budgets`, `confirm_recurring`, `list_recurring` [S]
- [ ] **1.10** Multi-currency `Posting` validation (commodity required; FX rate optional but commodity-pair must match base when present) + **`Unmatched` placeholder convention**: auto-import sources that lack the other half of a transaction post to/from a single top-level `Unmatched` account (no type prefix); `Unmatched.balance == 0` is the steady-state invariant — non-zero balance signals reconciliation pending or hidden fee. See [[project-unmatched-account-pattern]]. [S]
- [ ] **1.11** A2 helpers: `BUSINESS_HIERARCHY_PREFIX = "Expenses:Business:"` const + `strip_business_prefix(account) -> (stripped, was_business)` helper. Lives in core for both Phase 1 event-time validation and Phase 6.6 import-time rewriter. [XS]
- [ ] **1.12** Wire `ledger-utils` into core as the in-process query/balance primitive. Foundational for Phase 4 (R1), Phase 5.7 (W1 balance check), Phase 7.2 (R2 filter DSL). Cross-platform smoke test (POC 0.1c already validated Android). [M]
- [ ] **1.13** Unit tests: event schema, projection idempotency, journal-file output validity (golden sample), A2 helper correctness [M]

---

## Phase 2: Server-Side Capture + Auto-Import (~14 tasks, all backend done; 2.12b UI half folded into 3.10)

### 2A. Document Extraction (Gemini-only for Cycle 3)

- [x] **2.1** Axum endpoint `PUT /blobs/<sha256>` — accepts file upload, validates SHA-256 matches body, stores at configured blob path. Done 2026-05-09 during POC 0.2.
- [x] **2.2** Axum endpoint `GET /blobs/<sha256>` — streams stored file with `infer`-detected MIME type. Done 2026-05-09 during POC 0.2.
- [x] **2.3** Server-side blob storage: `BLOB_DIR` env var, atomic ULID-named-temp + rename, idempotent, `BlobError` typed-error enum. Done 2026-05-09 during POC 0.2.
- [x] **2.4** Gemini Flash multimodal `DocumentExtractor` trait impl — handles all input modalities (photos, PDFs, text, paystubs, bank/brokerage statements). Cycle 3's sole extractor. [L] — done 2026-05-17 (`core/src/extraction/gemini.rs`)
- [x] **2.5** `DocumentExtractor` trait + routing scaffold — `core::extraction::DocumentExtractor`; hybrid dispatch: IMAP sender pre-routing + photo→receipt-mode + PDF requires user pick + email body→text mode. Single Gemini impl for now; ready for Cycle 4 Veryfi drop-in. [S] — done 2026-05-17 (`core/src/extraction/mod.rs`)
- [x] **2.6** Verification pass: line-item-sum == total (receipts), gross - deductions == net (paystubs), confidence threshold gate; flag below-threshold drafts for manual review [M] — done 2026-05-17 (`core/src/extraction/verify.rs`)
- [x] **2.7** Frankfurter FX daily-rate fetcher (free, ECB-sourced, no API key) for CAD/USD/EUR; emits `P` directive into journal projection. NGN is manual entry per Phase 2.13 / import flow. [M] — done 2026-05-17 (`core/src/fx.rs`)
- [x] **2.8** Integration tests: end-to-end Gemini capture for each modality against real samples (paper receipt, brokerage statement, paystub, email body) [M] — done 2026-05-17 (`core/tests/extraction_integration.rs`)

### 2B. Auto-Import Sources (NEW post-replan)

- [x] **2.9-spike** SnapTrade coverage spike — done 2026-05-17. **SnapTrade rejected**: the connect-account flow errored out under maintenance AND the error explicitly named `WealthSimpleTrade` as the target broker (i.e., only the trading-account subset, not the chequing + crypto accounts that hold 95% of volume). Two strikes → fallback chosen. [XS]
- [x] **2.9** WealthSimple data path — done 2026-05-17 via the **fallback** subprocess path: `gboudreau/ws-api-python` (active community library, GraphQL-based, ~bi-monthly auth-regex hotfixes inherited via `pip install --upgrade`). Python runtime added as server-side dep. Port to native Rust slated for Cycle 4 if subprocess proves stable; immediate rewrite avoided per `feedback_prefer_integration_over_rewrite.md`. Live-verified against real account (7 sub-accounts + 297 txns pulled). [L] (`core/src/auto_import/wealthsimple.rs`)
- [x] **2.10** Wise official API client — done 2026-05-17. Token-based auth, `/transfers`/`/statements` endpoints, multi-currency transaction mapping (CAD/USD/EUR), dedup. Live-verified. [M] (`core/src/auto_import/wise.rs`)
- [x] **2.11** IMAP poller infrastructure — done 2026-05-17. Connection (TLS, app-password creds), label-watching loop, sender-pattern dispatch. Live-verified against real Gmail INBOX. [L] (`core/src/auto_import/imap.rs` + `imap_real.rs` + `imap_source.rs`)
- [x] **2.12a** IMAP handler — Standard Chartered NGN (decrypt + extract): detect statement email → fetch attached PDF → decrypt with stored account-derived password → Gemini multimodal extraction → emit structured transaction list. Done 2026-05-17, live-verified against real password-protected USD statement. [M] (`core/src/auto_import/sc_ngn.rs`)
- [ ] **2.12b** IMAP handler — Standard Chartered NGN (batch draft + manual FX prompt): wrap 2.12a output into a batch draft with manual NGN→CAD FX rate prompt at review time. **NOT shipped** — `sc_ngn::handle` currently emits `TransactionRecorded` events directly via `statement_extraction_to_events`, bypassing the user-designed review gate. Fix shape: introduce a `AutoImportBatchProposed { source, draft_events }`-style event the scheduler emits instead of bare records; 3.10's review screen consumes the proposal and Save fans out actual `TransactionRecorded`s. Coupled to 3.10's implementation. [S]
- [x] **2.13** IMAP handler — online-purchase receipts: parse email body or attached PDF → Gemini extraction → single-transaction draft. Done 2026-05-17. [M] (`core/src/auto_import/receipts.rs`)
- [x] **2.14** Auto-import scheduling — background fetcher with configurable interval per source, exponential backoff on failure, status reporting hook into `StatusReporter`. Pattern mirrors `auto_close_scheduler.rs`. Done 2026-05-18 in auto-end commit `214a785` (333 lines). [M] (`core/src/auto_import_scheduler.rs`)

---

## Phase 3: Frontend Capture Flows (~10 tasks, 5 done + 1 partial)

Custom Dioxus screens for capture. Mobile-first for photos; desktop-first for PDFs. **UI invariant:** sort commodity displays explicitly (POC 0.1c finding — HashMap iteration order non-deterministic).

- [x] **3.1** Photo capture screen (mobile primary): camera/file picker → upload progress → wait state → confirm-draft screen [L] — done 2026-05-17 (mock-verified end-to-end, real wiring in place, live Gemini round trip unobserved)
- [x] **3.2** PDF upload (desktop file picker, mobile share-target) → confirm-draft screen [M] — done 2026-05-17 as a shared `DocumentCapture` component with PDF hint picker
- [x] **3.3** Android share-target intent for PDFs/images (Tauri Android manifest + handler) [M] — done 2026-05-18. Durable home in `tauri-app/src-tauri/android-overrides/` (manifest + `MainActivity.kt`); `build.rs::apply_android_overrides` copies into `gen/android/` on every build so overrides survive `tauri android init`. `MainActivity.kt` catches `ACTION_SEND`, stashes bytes + meta sidecar to `filesDir` (MIME normalized via 3-step `intent.type ?? contentResolver.getType(uri) ?? "application/octet-stream"`). `take_pending_share_intent` Tauri command + `invoke_take_pending_share_intent` bridge wrapper drain on app mount; `classify_share_mime` routes by MIME + filename-extension rescue (permissive by design — Gemini downstream is the actual validator; failure surfaces in existing `CaptureState::Error` retry path). `DocumentCapture` accepts a `preloaded` prop + renders "Use shared file" confirm step (per user-chosen UX: always show capture view first, reviewable + symmetric). 7 unit tests cover classifier branches (image subtypes, case-insensitivity, legacy MIMEs, stripped-MIME rescue, text-family rejection, unknown subtypes). **Live Android APK round-trip still unobserved** — pending `cargo tauri android dev` against device.
- [x] **3.4** Email body paste screen (text area + extract button) → confirm-draft [S] — done 2026-05-17 (`EmailCapture` component, sends body as `text/plain` with hint `email_body`)
- [x] **3.5** Manual entry form (account, date, amount, commodity, category, tags, description) [M] — done 2026-05-17 as the shared `TransactionForm` (initial=None for manual)
- [x] **3.6** Confirm-draft screen — extracted fields editable inline, attachment thumbnail visible, Save commits `TransactionRecorded` [M] — done 2026-05-17 as the shared `TransactionForm` (initial=Some(draft) post-extraction); **attachment thumbnail skipped** — see Phase 3 Known Gaps. _(logbook bundle 3.1+3.2+3.3+3.4+3.5+3.6: "Capture a transaction via photo, PDF, share-target, email paste, or manual entry"; tags: dioxus, tauri, mobile-development, multimodal, ux)_
- [x] **3.7** Local attachment cache: app data dir + LRU eviction (200MB cap default); fetch-on-demand from `/blobs/<sha256>` [M] — done 2026-05-17. Server `/documents/extract` now accepts `?attach=true`; client mirrors bytes to `<app_data>/attachments/<sha256>` and threads `AttachmentRef` onto the `TransactionRecorded` event. Bundled the Phase 3 Known-Gaps "dropped attachment bytes" item. `commands::attachments` ships `fetch_attachment` / `attachment_cache_size` / `clear_attachment_cache` (the last two fuel 3.8). 5 unit tests covering roundtrip / miss / LRU eviction order / invalid-hash rejection / clear-keeps-dir. mtime LRU (atime unreliable on noatime mounts), touch-on-hit for residency.
- [x] **3.8** Settings → Cache section: "Clear Attachment Cache" button + cache size displayed [XS] — done 2026-05-18 as `CacheSection` in `tauri-app/frontend/src/pages/settings.rs:234` (mounted at `:204`); reads via `invoke_attachment_cache_size`, clears via `invoke_clear_attachment_cache`, formats with binary units to match the 200 MiB cap.
- [x] **3.9** Settings → Auto-Import Sources section: per-source connect/disconnect, last-fetch timestamp, status indicator, manual-fetch-now button. Pattern follows `ImportExportSection` + Danger Zone. [M] — done 2026-05-18 as `AutoImportSection` in `tauri-app/frontend/src/pages/settings.rs:494` (mounted at `:207`); per-source rows render health badge + relative-time last tick + outcome summary + manual-tick trigger via `invoke_trigger_auto_import_tick`. The "WS session expired" surface noted in Phase 2 known limitations resolves through the same row's health indicator.
- [x] **3.10** Auto-import review screen — batch preview with dedup info, per-row accept/skip/edit, manual FX rate prompt for NGN imports, commit triggers `TransactionRecorded` event batch [L] — done 2026-05-18 (3.10.5 + 3.10.6 below) _(logbook bundle 2.9+2.10+2.11+2.12a+2.12b+2.13+2.14+3.9+3.10: "Auto-import transactions from WealthSimple, Wise, and email IMAP, with a unified review screen"; §4 cross-links to the capture-flow entry; tags: tauri, imap, email, auto-import, wealthsimple, wise)_
  - [x] **3.10.1** `MANUAL_FX_CURRENCIES` const in `core/src/fx.rs` (single source of truth for Frankfurter-uncovered currencies; `needs_manual_fx(commodity)` helper) — done 2026-05-18 (4 unit tests)
  - [x] **3.10.2** `AutoImportBatch{Proposed,Committed,Dismissed}` event variants + payload structs in `core/src/events/types.rs` — done 2026-05-18 (4 roundtrip tests; `DraftTransaction` helper struct introduced)
  - [x] **3.10.3** Refactored 4 auto-import handlers (`sc_ngn`, `receipts`, `wise`, `wealthsimple`) to produce drafts wrapped into one `AutoImportBatchProposed` event per non-empty tick. `core/src/auto_import/mod.rs::to_proposed_event` is the shared wrap helper; `core/src/extraction/event_mapper::{statement,receipt}_extraction_to_drafts` are the per-flavor draft builders. **Legacy `*_to_events` helpers retained** for handlers not migrated; remove once all callers gone. Per-source dedup_key shape: IMAP-driven sources (`sc_ngn`, `receipts`) use stable `{source}-uid-{message_uid}`; polling sources (`wise`, `wealthsimple`) use per-tick ULID — polling-source scheduler-level dedup left for Cycle 4 polish. Done 2026-05-18 (65 auto_import tests pass).
  - [x] **3.10.4** `AutoImportProjection` in `core/src/events/auto_import_projection.rs` over a new `pending_auto_import_batches` table. Record id = `{source}-{dedup_key}` so duplicate Proposed events (same IMAP UID re-fetched) collapse via UPSERT into one row. **Dismissed batches stay dismissed across re-proposal** — verified by dedicated test. Registered in Tauri client's `ProjectionRunner` (server intentionally projects nothing per existing pattern). Done 2026-05-18 (4 projection tests pass; 348/348 workspace tests pass; clippy clean).
  - [x] **3.10.5** Tauri commands `list_pending_batches`, `commit_batch(batch_id, accepted_indices, fx_rate?, fx_commodity?)` (fans out `TransactionRecorded` × N + optional `ExchangeRateRecorded` + `AutoImportBatchCommitted`), `dismiss_batch(batch_id, reason?)`. **No server routes added** — projection lives client-side only; events flow up via the normal sync path. `pick_fx_event_date` chooses **latest accepted date** for the single `P` directive (user decision 2026-05-18). Validation: indices in-range + dedup, fx_pair both-or-neither, rate > 0, status==pending gate. Done 2026-05-18 (4 unit tests on `pick_fx_event_date` + 1 cross-crate queries integration test). _Server routes from the original wording dropped — see 3.10.6 architecture note._
  - [x] **3.10.6** Finances UI — pending-count banner on Home (click → batch list) + per-batch review screen with per-row accept checkbox, source-metadata `<details>`, manual-FX rate prompt that gates the Commit button when the batch has any commodity in `MANUAL_FX_CURRENCIES_UI` (today: NGN), Dismiss button. New `FinancesView::{BatchList, BatchReview}` variants; selected `batch_id` lives in a sibling `Signal<Option<String>>` so the enum stays `Copy`. Pending-count signal refreshes via `use_resource` keyed on the Home view (cheap recount on every Home landing — including post-commit/dismiss return). Done 2026-05-18.
  - [x] **3.10 followup** Remove legacy `statement_extraction_to_events` + `receipt_extraction_to_events` helpers once 3.10.6 ships; re-grow the `let _ = db;` placeholder assertions in `wise`/`wealthsimple` tests to query `pending_auto_import_batches`. — done 2026-05-20. Dropped both `*_to_events` fns + their `build_event` helper + the unused `EventType`/`NewEvent`/`TransactionRecordedPayload`/`Utc` imports from `core/src/extraction/event_mapper.rs`; mapper tests rewritten against `*_to_drafts` so coverage shape is preserved (same input → posting accounts + sign + idempotent ids). `pub use` cut from `core/src/extraction/mod.rs`. Both auto-import projection tests (`wise.rs::pull_projects_proposed_batch_with_unmatched_mirror` + `wealthsimple.rs::pull_projects_proposed_batch_with_unmatched_mirror`) now register `AutoImportProjection` alongside `BudgetProjection` in `test_db_and_runner` and assert source name + dedup-key prefix + pending status + per-draft external_id format + Unmatched-mirror sign convention against the `pending_auto_import_batches` row. **Phase 3 fully closed.**

---

## Phase 4: Transactions Surface + R1 Health Glance (~6 tasks)

Read paths. Both platforms. **R1 reads from in-process PTA engine** (`ledger-utils::balance::calculate_account_balances`), not hledger CLI.

- [x] **4.1** Transaction list screen (mobile + desktop): paginated, sortable, filter by date / account / tag / category [L] _(logbook bundle 4.1+4.2+4.3: "Browse, view, and edit recorded transactions"; tags: dioxus, tauri, surrealdb, ux)_ — DONE 2026-05-21 (`bbd7d0e`): backend `TxnFilter` + dynamic WHERE with SurrealDB `array::any`, filter chips UI (date range, account substring, category, tag), 6 new backend filter tests.
- [x] **4.2** Transaction detail view + attachment viewer (PDF render, image render) [M] — DONE 2026-05-21 (`5012083`): `get_transaction` Tauri command, full metadata + posting list, attachment viewer with object-URL rendering + RAII guard (image/PDF/other classifier), 5 pure-fn tests for attachment helpers.
- [x] **4.3** Inline edit category + tag from list and detail views [S] — DONE 2026-05-21 (`9db909d`): `EditableCategoryChip` + `EditableTagList` reusable components with `stop_propagation` for nested-click conflict; optimistic local-signal updates via `invoke_categorize_transaction` + `invoke_tag_transaction`.
- [ ] **4.4** Account list screen — balances per commodity, last reconciled date. Multi-currency aggregation uses Frankfurter rates (CAD/USD/EUR) + stored `P`-directive NGN rates. [M] _(logbook 4.4: "Account list with multi-currency balances aggregated to CAD"; Frankfurter + P-directive NGN converge here; tags: dioxus, ledger, multi-currency, fx)_
- [ ] **4.5** R1 financial-health glance dashboard (mobile + desktop): recurring obligations summary, can-I-afford-X widget, monthly spending vs income trend, **`Unmatched.balance` reconciliation-pending widget** (renders the non-zero gap as a single glanceable number; click-through routes to 5.7 unified reconciliation review). Reads from in-process `ledger-utils` balance calc. [L] _(logbook bundle 4.5+4.6: "R1 financial-health glance dashboard"; §4 names Path B as the in-process payoff; tags: dioxus, ledger, dashboard, mobile-development)_
- [ ] **4.6** Recurring obligations summary widget — pulls from confirmed recurring patterns (Phase 1.5 events) [M]

---

## Phase 5: Workflows (Desktop-Heavy) (~8 tasks)

Budget (W4), recurring detection (W3), and unified reconciliation. Desktop-only in MVP per Session 4 scope split. W1/W6 naming dropped per [[feedback-shared-ui-shape-is-a-tell]] — statement import and cross-source merging share the same matching engine; one workflow, two trigger points.

- [ ] **5.1** W4 budget setup screen (desktop): per-category budget target, per-cycle (monthly default; weekly / biweekly options) [M] _(logbook bundle 5.1+5.2: "W4 budget setup and actual-vs-planned view"; tags: dioxus, budgeting, ux)_
- [ ] **5.2** W4 actual-vs-planned view: progress bars per category, over/under indicators, period rollover behavior [M]
- [ ] **5.3** W3 recurring detection scanner: nightly background pass over event log; emits `RecurringTransactionDetected` events for repeat patterns (same vendor + amount + cadence) [M] _(logbook bundle 5.3+5.4: "W3 recurring transaction detection and confirmation"; feeds R1 dashboard's recurring-obligations widget via 4.6; tags: dioxus, event-sourcing, dashboard)_
- [ ] **5.4** W3 confirm-suggestion UI (desktop): list of detected patterns, accept/dismiss/edit per-row [S]
- [ ] **5.5** Statement CSV parser (CIBC primary; extensible to other bank-export formats). Each parsed row emits a `TransactionRecorded` with one real-account posting (e.g., `Assets:CIBC:Chequing`) + one `Unmatched` placeholder + `statement_source` metadata; the resulting events feed the unified matching engine (5.6). [M]
- [ ] **5.6** Unified matching engine — signal-only candidate scoring over `Unmatched`-touching transactions; heuristics: same amount, ±N days, opposite sign on `Unmatched` posting, optional fuzzy description match; produces ranked candidate list with confidence indicator per match. Source-agnostic: pairs any two `Unmatched`-touching events regardless of origin (auto-import × auto-import, auto-import × statement, statement × IMAP receipt, etc.). Per [[project-unmatched-account-pattern]]. [M]
- [ ] **5.7** Unified reconciliation review UI (desktop) — two-column candidate display with confidence indicators, click-to-confirm merge (emits `TransactionsMerged`), optional "add balancing posting" affordance for hidden-fee resolution (e.g., wire-transfer fee, FX spread), optional `cleared` flag (emits `TransactionCleared` when one merged side traces back to a statement source via 5.5 metadata). Also handles the no-match path: statement-sourced events without a candidate accept a manual category fill-in (emits `TransactionUpdated` replacing `Unmatched`) + `TransactionCleared`. Reachable from R1 `Unmatched.balance` widget (4.5). [L] _(logbook bundle 1.14+1.15+5.5+5.6+5.7+5.8+`Unmatched`-widget-in-4.5: "Reconciliation — unified matching across auto-import sources, statement imports, and captured receipts via the `Unmatched` clearing account"; tags: dioxus, ledger, event-sourcing, reconciliation)_ _(demo) — pure-transform WASM-island candidate: input two fake transaction arrays with `Unmatched` placeholders → ranked match candidates → user picks → merged transaction; teaches the balance-zero invariant interactively._
- [ ] **5.8** Balance check — sum of `cleared` transactions for an account compared against statement closing balance (recorded via 5.5 metadata); flags discrepancy. Uses `ledger-utils::balance::calculate_account_balances` in-process. **No shell-out.** [S]

---

## Phase 6: Import (D1 + D3) — after user pre-cleanup (~6 tasks)

Runs once user has cleaned external hledger journal in their separate session.

- [ ] **6.1** D1 hledger journal parser → emits `TransactionRecorded` events for each posting (preserve original commodities + tags). Reuse POC 0.1b parser harness at `.archive/poc/ledger-parse/src/main.rs`. [L]
- [ ] **6.2** D3 import preview screen (desktop): shows accounts + transaction count, accept/skip per account, basic edits (rename account, drop) [M] _(logbook bundle 6.1+6.2+6.3+6.4+6.5+6.6: "Import existing hledger journal — parse, preview, commit, validate against real pre/post-cleanup data; includes A2 business-hierarchy→tag migration"; §4 covers the migration decision (why posting tags over account hierarchy for business/personal separation); tags: dioxus, ledger, migration)_
- [ ] **6.3** Commit import — idempotent batch event append; re-run safe (dedup by transaction hash) [S]
- [ ] **6.4** Pre-cleanup import test against `.reference/paisa/` (user's actual historical journal through Sept 2025; 5,826 transactions validated through POC 0.1b parser). Exercises the parser AND the projection round-trip (event emission → SurrealDB + journal-file projection) on real-world-messy data before the cleanup pass touches it. [S]
- [ ] **6.5** Post-cleanup import end-to-end against user's cleaned journal (after external pre-cleanup session is done). Same data as 6.4 but tidied — together the pair validates that the import handles both the messy and clean states. [S]
- [ ] **6.6** **A2 hierarchy→tag rewriter** — walk parsed postings; if `account.starts_with("Expenses:Business:")` use Phase 1.11 helper to strip the segment and append `type:business` posting tag. ≈50 LOC + tests covering: business posting rewritten, plain posting unchanged, deeply-nested `Expenses:Business:Subscriptions:Adobe` flattened correctly. [S]

---

## Phase 7: R2 + Settings (~4 active tasks, 1 done)

Last phase. All stretch/backlog items deferred to Cycle 4 per Session 5 scope decision 2026-05-16: Cycle 3 is already substantial (~70 active tasks) and Cycle 4 is dedicated polish — better to land Cycle 3's core firmly than to chase backlog.

- [ ] **7.1** R2 GUI query builder (desktop) **EXPANDED** — category × date-range × tag filter + amount threshold + description-contains + OR/AND combinator → tabular result [L]
- [ ] **7.2** R2 **Rust filter DSL on top of `ledger-utils`** (~150-200 LOC; parser + filter pipeline + tests). Examples: `account:Expenses:Food tag:business date:2026-04`. GUI builder (7.1) produces equivalent DSL output. **No hledger CLI shell-out** — replaces Session-4's shell-out plan. LLM-translated NL queries deferred to Cycle 4. [L] _(logbook bundle 7.1+7.2: "Ad-hoc transaction queries — GUI builder + filter DSL over `ledger-utils`"; tags: rust, dsl, dioxus, ledger)_ _(demo) — pure-transform WASM-island: input (query string + tiny txn array) → output (filtered subset); sits alongside the reconciliation merge engine as the cycle's second demoable feature._
- [ ] **7.3** Settings: base currency picker (CAD default, list of common ISO codes) [XS]
- [ ] **7.4** Settings: cache management surface (size shown, clear button — extends 3.8) [S]

**Cycle-history marker:**

- [x] **7.7** `editor.rs:179` release-build compile error fix — done 2026-05-09 during POC 0.2 build

---

## Sequential Execution Map (Session 5, post-replan)

```
Phase 0 (POCs, gating):    [DONE]
Phase 1 (foundation):      1.1 → 1.2+1.3+1.4+1.5+1.14+1.15 → 1.6 → 1.7 → 1.8+1.9 → 1.10 → 1.11 → 1.12 → 1.13
Phase 2A (extraction):     [2.1+2.2+2.3 done] → 2.4 → 2.5 → 2.6 → 2.7 → 2.8
Phase 2B (auto-import):    2.9-spike → 2.9 → 2.10 → 2.11 → 2.12a → 2.12b → 2.13 → 2.14
Phase 3 (capture UI):      3.1 → 3.2 → 3.3 → 3.4 → 3.5 → 3.6 → 3.7 → 3.8 → 3.9 → 3.10
Phase 4 (read):            4.1 → 4.2 → 4.3 → 4.4 → 4.5+4.6
Phase 5 (workflows):       5.1 → 5.2 → 5.3 → 5.4 → 5.5 → 5.6 → 5.7 → 5.8
Phase 6 (import):          [WAIT for user pre-cleanup] → 6.1 → 6.2 → 6.3 → 6.4 → 6.6 → 6.5
Phase 7 (R2+settings):     7.1 → 7.2 → 7.3+7.4 (no backlog this cycle — all stretch items deferred to Cycle 4)
```

**Note on Phase 6 ordering:** 6.6 (A2 rewriter) runs before 6.5 (real-journal end-to-end) so the rewriter is exercised on actual data.

---

## Cycle 4 Backlog (locked at Session 4 + grown post-replan 2026-05-15)

Cycle 4 is dedicated polish + stable-v1 release.

- [ ] **Editor parity with Obsidian** — close remaining gap so user prefers omni-me over Obsidian for daily journaling
- [ ] **App logo** — replace default Tauri assets across desktop + Android
- [ ] **Branch-gate process** — establish post-v1 workflow: feature branches + merge gates to protect stable
- [ ] **Stable v1 version stamp** — set semver, tag release
- [ ] **C1 email auto-fetch** (vs paste) if needed; W1 mobile reconciliation; W4 mobile budget edits; R3 self-employment dashboards; R4 tax form validation reports
- [ ] **PWA fallback** — still deferred from Cycles 1+2
- [ ] **Veryfi `DocumentExtractor` impl** — `(receipt, paper)` → `/documents/`; `(paystub, w2s)` → `/w2s/`; `(bank_statement, *)` → `/bank_statements/`. Trait + routing scaffold already in place from Phase 2.5.
- [ ] **ExchangeRate-API auto-rates for NGN** (and any non-Frankfurter currencies) — replaces Cycle 3's manual-per-statement entry
- [ ] **LLM-translated NL queries for R2** — evaluation only; ship only if real usage demands it
- [ ] **PaddleOCR sidecar** — moved from Cycle 3 backlog (7.11) since Gemini was sole Cycle 3 extractor

**User-managed declared-accounts list (Cycle 3 Phase 4.4 surfaced 2026-05-23):**

- [ ] **Lift `core::balances::LISTABLE_ACCOUNTS` into the `accounts` SurrealDB table** (currently a code constant; edits require a recompile). The `accounts` table + `AccountAdded` events already exist from Phase 1.9 but no UI manages them — Cycle 4 adds a Settings page section "Declared Accounts" with add/remove rows. `is_listable_account` then becomes a DB lookup over declared rows. Optional: a new `AccountRemoved` event so the soft-delete is reversible. [S]

**Cross-submodule state management (Cycle 3 Phase 3 gap surfaced 2026-05-17):**

- [ ] **Tab-switch protection for in-flight captures.** Today's symptom: user kicks off a photo upload + Gemini extract round trip, then taps another top-level tab before it returns; `FinancesPage` unmounts and the captured bytes + draft progress vanish silently with no recovery path. Same shape applies to any other long-running per-tab work added later. **Fix scope:** lift in-flight captures (and any future "started here but takes a while" state) into a top-level shared store keyed by capture-id so navigating away doesn't kill the future; show a "you have an in-flight capture" affordance on Home if the user wanders back. This is the broader "sub-module state isolation hurts cross-tab continuity" rework — applies anywhere a page is the de-facto owner of work that outlives its own mount. [M]

**Stretch items deferred from Cycle 3 Phase 7 (Session 5 decision 2026-05-16):**

- [ ] **Daily Flow consistency visualizer redesign** — frequency-aware (was 7-day hard-coded, broken for Weekly/Biweekly/Monthly/Custom routines; deferred from Cycle 3 task 7.5) [M]
- [ ] **`BufferEvent::FlushFailed` → `StatusReporter` wiring** — user-visible "stuck buffer" indicator (deferred from Cycle 3 task 7.6) [S]
- [ ] **Configurable `FORCE_GENERIC_DIRS`** — currently hardcoded to `Work/` (deferred from Cycle 3 task 7.8) [S]
- [ ] **`auto_close_scheduler::AppState.event_store` → `Arc<dyn EventStore>`** — parity with main store (deferred from Cycle 3 task 7.9) [XS]
- [ ] **Seconds duration unit on routine items** — breaking event-schema change across 16 touch points (deferred from Cycle 2 and again from Cycle 3 task 7.10) [M]

---

## Cycle 5+ filed

- Inbox management feature (user's "far future dream")
- Open Banking Canada evaluation (when bank adoption matures)

---

## Meta: Validation Goals

- [ ] Track Gemini Flash free-tier usage across Cycle 3
- [ ] Validate POC outcomes feed into actual phase plan (no architecture drift between Phase 0 and Phase 1+)
- [ ] User completes external journal cleanup in time for Phase 6 (rough target: by end of Phase 4)
- [ ] **Path B parity at Cycle 3 close:** replay existing journal (5,826 txns) through new event store + projection → re-emitted journal file balances byte-match POC 0.1b totals (modulo HashMap ordering, sorted UI display)
- [ ] **A2 rewriter correctness:** synthetic fixture of mixed `Expenses:Business:*` + plain-`Expenses:*` postings → all Business postings have stripped account AND `type:business` tag; non-Business postings unchanged
- [ ] **R2 filter DSL:** `account:Expenses:Food tag:business date:2026-04` returns expected subset; GUI builder produces equivalent filter
- [ ] **CAD/USD/EUR/NGN multi-currency:** R1 dashboard aggregates correctly using Frankfurter rates for the three majors and manually-entered `P` directives for NGN
