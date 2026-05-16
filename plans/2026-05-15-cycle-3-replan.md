# Cycle 3 Plan Revision — Post-POC Replan

## Context

Cycle 3 Session 4 (2026-05-02) scoped the budget feature on the assumption that omni-me would shell out to the `hledger` CLI for queries and projections, with Mindee + Veryfi + Nanonets + PaddleOCR as parallel extractor candidates. Phase 0 POCs (Session 5, 2026-05-09) invalidated several premises:

- **Path B (pure-Rust embedded PTA engine) confirmed** desktop + Android byte-identical against the user's 5,826-transaction production journal. hledger CLI no longer in the architecture.
- **Existing journal uses `Expenses:Business:*` account hierarchy**, not posting tags — directly contradicts the Session-4 A2 tag-based decision and needed reconciliation.
- **Veryfi's general `/documents/` endpoint mis-classifies bank/brokerage statements** as receipts; routing must be `(document_type, provider)` explicit.
- **Nanonets dropped, Mindee dropped** — Phase 2 extractor count down from 4 → 2 candidates.
- **Phase 2 tasks 2.1/2.2/2.3 already shipped** during POC 0.2.

Additionally, this replanning session surfaced two material facts that Session 4 didn't have:

- **The user has three additional accounts not in the existing journal** — CIBC chequing (CAD), Wise (CAD/USD/EUR + flexible), Standard Chartered Nigeria (NGN). They never made the journal because manual sync was too laborious — *the same friction that killed the prior tracking attempt.*
- **95% of the user's transaction volume happens in WealthSimple** — single broker for chequing + 3 registered + crypto.

These two facts together moved auto-import from "post-MVP nice-to-have" to "without it, Cycle 3 ships and stops getting touched." The replan therefore promotes three auto-import paths into Cycle 3 scope despite the size cost.

## Locked decisions (this session)

1. **Journal file location** — per-device only, regenerated from events. Source of truth = events (already synced). File is a regenerable local cache.
2. **A2 business/personal model** — tag-only with Phase 6 import rewriter. Existing `Expenses:Business:*` postings get rewritten at import: strip the `Business:` segment, emit `type:business` posting tag. Past "business on personal card" transactions stay untagged until user retags them in normal use.
3. **R2 query path** — expand 7.1 GUI builder (amount threshold, description-contains, OR/AND combinator) **and** build a small Rust filter DSL on top of `ledger-utils` (≈150-200 LOC). LLM-translated NL queries deferred to Cycle 4 backlog.
4. **Auto-import in Cycle 3** — all three: WealthSimple unofficial API, Wise official API, Email IMAP poller. IMAP dual-purposed for Standard Chartered NGN monthly statements (password-protected PDF decrypt → Gemini multimodal extraction) and online-purchase email receipts. Scope cost accepted.
5. **Document-type routing** — hybrid policy (IMAP sender-based pre-routing + photo defaults to receipt + PDF requires user pick + email body → Gemini text mode). **Cycle 3 ships Gemini multimodal as sole extractor**; Veryfi implementation deferred to Cycle 4. Extractor trait + routing scaffold stays in code so swapping Veryfi in later is a single-implementation drop-in.
6. **NGN FX rate source** — manual entry per Standard Chartered statement at import review; rate stored as hledger `P` directive. ExchangeRate-API integration deferred to Cycle 4.
7. **Scope level** — no further cuts; ~75 task target accepted.

## Revised phase plan

### Phase 0 — POCs *(complete, closed 2026-05-09)*

### Phase 1 — Core Foundation (~12 tasks)

Event schema, projections, Tauri commands. Adjustments vs Session 4 plan:

- `TransactionRecorded` payload: drop top-level `account` (postings carry their own); tags live on `Posting`, not on transaction (per A2 = tag-based posting tags).
- Decide `amount` representation up front (rust_decimal vs string vs other) — this is the cardinal data-type decision that ripples through every downstream phase.
- Codify A2 helpers in Phase 1 even though the rewriter runs in Phase 6 (constants like `BUSINESS_HIERARCHY_PREFIX = "Expenses:Business:"`, helper `strip_business_prefix(account)`).
- **Wire `ledger-utils` into core** as the in-process query/balance primitive. Foundational for Phases 4 (R1), 5 (W1 balance check), 7 (R2 filter DSL).
- 1.6 journal-file projection: writes a per-device journal file regenerable from events (matches Notes/SurrealDB pattern).

### Phase 2 — Server-Side Capture + Auto-Import (~15 tasks)

**2A. Document extraction (Gemini-only for Cycle 3):**
- 2.1-2.3 ✅ done in POC 0.2 (blob endpoints, blob storage)
- ~~2.4 Veryfi~~ — **deferred to Cycle 4**
- 2.5 Gemini multimodal `DocumentExtractor` impl — handles all input modalities for Cycle 3
- 2.6 `DocumentExtractor` trait + routing scaffold — single Gemini impl for now; ready for Veryfi drop-in
- 2.7 Verification pass (line-item-sum == total, gross - deductions == net, confidence gate)
- 2.8 Frankfurter FX daily-rate fetcher (CAD/USD/EUR; emits `P` directives)
- 2.9 Integration tests for the Gemini path (paper receipt, brokerage statement, paystub, email body)

**2B. Auto-import sources (NEW):**
- 2.10 WealthSimple unofficial API client — auth (incl. OTP flow), per-account statement fetch, transaction mapping, dedup-hash strategy
- 2.11 Wise official API client — token-based auth, `/transfers`/`/statements` endpoint, multi-currency transaction mapping, dedup
- 2.12 IMAP poller infrastructure — connection (TLS, app-password creds in secure storage), label-watching loop, sender-pattern dispatch table
- 2.13 IMAP — Standard Chartered NGN handler: detect statement email → fetch PDF → decrypt with stored password → Gemini multimodal extraction → batch draft transactions
- 2.14 IMAP — online-purchase receipt handler: parse email body or attached PDF → Gemini extraction → single-transaction draft
- 2.15 Auto-import scheduling — background fetcher (configurable interval per source), exponential backoff on failure, status reporting hook into `StatusReporter`

### Phase 3 — Frontend Capture Flows (~10 tasks)

- 3.1-3.8 mostly as planned (photo capture, PDF upload, share-target, email paste, manual entry, confirm-draft, local attachment cache, cache-clear button)
- UI invariant: sort commodity displays explicitly (POC 0.1c finding — HashMap iteration order non-deterministic)
- **NEW 3.9** — Settings: Auto-Import Sources section. Per-source connect/disconnect, last-fetch timestamp, status indicator, manual-fetch-now button
- **NEW 3.10** — Auto-import review screen. Batch preview with dedup info, per-row accept/skip, commit triggers `TransactionRecorded` event batch

### Phase 4 — Transactions Surface + R1 (~6 tasks)

- 4.1-4.6 as planned (transaction list, detail+attachment viewer, inline edit, account list, R1 dashboard, recurring obligations widget)
- R1 reads from in-process PTA engine (`ledger-utils::balance::calculate_account_balances`), not hledger CLI
- Multi-currency aggregation in R1 uses Frankfurter rates for CAD/USD/EUR and stored P-directive NGN rates

### Phase 5 — Workflows (Desktop-Heavy) (~7 tasks)

- 5.1-5.6 as planned (W4 budget setup, actual-vs-planned, W3 recurring detection, W3 confirm UI, W1 CSV import, W1 reconciliation match UI)
- **5.7 rewritten** — balance check uses `ledger-utils::balance::calculate_account_balances` in-process; no shell-out

### Phase 6 — Import (D1 + D3) (~6 tasks)

- 6.1-6.5 as planned (hledger journal parser, preview screen, commit, synthetic test, real-journal end-to-end)
- **NEW 6.6** — A2 hierarchy→tag rewriter: walk parsed postings, if `account.starts_with("Expenses:Business:")` strip the segment and append `type:business` posting tag. ≈50 LOC + tests.

### Phase 7 — R2 + Polish + Stretch (~12 tasks)

- **7.1 EXPANDED** — GUI builder gains amount threshold, description-contains, OR/AND combinator dimensions
- **7.2 RESHAPED** — Rust filter DSL on top of `ledger-utils` (~150-200 LOC; parser + filter pipeline + tests). Examples: `account:Expenses:Food tag:business date:2026-04`
- 7.3 Base currency picker
- 7.4 Cache management surface
- 7.5-7.11 Backlog stretch (Daily Flow visualizer redesign, FlushFailed StatusReporter wiring, ✅ 7.7 done, FORCE_GENERIC_DIRS config, scheduler `Arc<dyn EventStore>`, seconds duration unit, ~~PaddleOCR sidecar~~ → Cycle 4)

## Cycle 4 grown commitments

Original Cycle 4 items still locked (editor parity, logo, branch-gate, stable v1, PWA fallback). Added:
- Veryfi `DocumentExtractor` implementation + routing for `(receipt, paper)` and `(paystub, w2s endpoint)` cases
- ExchangeRate-API auto-rates for NGN (and any non-Frankfurter currencies)
- LLM-translated NL queries for R2 (Cycle 4 evaluation; ship only if real usage demands it)

## Cycle 5+ filed

- Inbox management feature (user's "far future dream")
- Open Banking Canada evaluation (when bank adoption matures)

## Critical files for next session

| Path | Purpose in revised plan |
|---|---|
| `core/src/events/types.rs` | Phase 1.1 entry point — add `TransactionRecorded`, `Posting`, `AttachmentRef`, `Tag` types here following the existing pattern (EventType enum + Display/FromStr + payload struct + `validate_payload` arm + tests). Existing pattern at types.rs:1-512. |
| `core/src/events/projection.rs` + new `core/src/events/transactions_projection.rs` | Phase 1.7 SurrealDB projection — mirrors `notes_projection.rs` / `routines_projection.rs` patterns |
| `core/src/extraction/` (new module) | Phase 2.5-2.6 `DocumentExtractor` trait + Gemini impl + routing dispatcher |
| `core/src/import/` (existing) | Phase 6.1 parser + Phase 6.6 A2 rewriter. Existing import module at `core/src/import.rs` |
| `core/src/auto_import/` (new module) | Phase 2.10-2.15 WS + Wise + IMAP clients |
| `tauri-app/src-tauri/src/commands/` | Phase 1.8-1.9 transaction/account/budget commands; Phase 2 auto-import commands |
| `tauri-app/frontend/src/pages/` | Phase 3 capture flows, Phase 4 transactions surface, Phase 5 workflows, Phase 7 R2 |
| `tauri-app/frontend/src/pages/settings.rs` | Phase 3.9 Auto-Import Sources section (follows pattern of `ImportExportSection` + Danger Zone) |
| `tasks.md` | Rewrite with revised phase structure on next session |
| `project.md` | Add session-log entry for this replan |
| `MEMORY.md` + related memory files | Capture A2 decision, auto-import staging, account list, scope cuts, FX strategy |

## Reused existing patterns

- **Event-type pattern** (EventType enum + Display/FromStr + payload struct + `validate_payload`) — `core/src/events/types.rs:1-512`
- **Projection pattern** (apply_event with idempotent SurrealDB writes) — `core/src/events/notes_projection.rs`, `routines_projection.rs`
- **Tauri command pattern** — see `tauri-app/src-tauri/src/commands/import.rs` for `commit_import` style (state-bearing, error-typed, returns struct)
- **Settings UI section pattern** — `pages::import_export::ImportExportSection` for the Auto-Import Sources UI
- **Background scheduler pattern** — `tauri-app/src-tauri/src/auto_close_scheduler.rs` for the auto-import scheduler infrastructure
- **POC 0.1b parser harness** at `.archive/poc/ledger-parse/src/main.rs` — reusable for Phase 6.4 synthetic-journal test
- **`SyncBuffer` + `StatusReporter` pattern** — auto-import status surfacing should hook the existing reporter

## Verification (how we know the plan is sound)

When Cycle 3 closes, these checks should pass:

1. **Path B parity:** replay your existing journal (5,826 txns) through the new event store + projection → re-emitted journal file balances byte-match POC 0.1b totals (modulo HashMap ordering, sorted UI display).
2. **A2 rewriter correctness:** synthetic fixture of mixed `Expenses:Business:*` + plain-`Expenses:*` postings → all Business postings have stripped account AND `type:business` tag; non-Business postings unchanged.
3. **WS auto-import end-to-end:** real WS account → trigger fetch → review screen lists new transactions with dedup against existing → commit → transactions visible in list view.
4. **Wise auto-import end-to-end:** same flow against your Wise account; multi-currency postings correctly tagged with native commodity.
5. **IMAP statement intake:** Standard Chartered email arrives → poller detects → PDF decrypts → Gemini extracts → batch draft for review → commit emits NGN transactions with manual rate prompt.
6. **R1 reads in-process:** dashboard widget renders from `ledger-utils` balance calc; no shell-out anywhere in the codebase.
7. **R2 filter DSL:** query string `account:Expenses:Food tag:business date:2026-04` returns expected subset; GUI builder produces equivalent filter.
8. **CAD/USD/EUR/NGN multi-currency:** R1 dashboard aggregates correctly using Frankfurter rates for the three majors and manually-entered `P` directives for NGN.

## Next session

Pick up cold from this plan file. First actions:
1. Update `tasks.md` to reflect the revised phase structure (~75 tasks across Phases 1-7).
2. Add session-log entry to `project.md` summarizing this replan.
3. Update `MEMORY.md` files with the locked decisions.
4. Begin Phase 1.1 — `TransactionRecorded` event type. Likely Learn-by-Doing on the `Posting` struct shape (`amount` representation choice is the cardinal data-type decision).
