# NEXT

**Next action: Push 2 of finance integrity — convention conformance.** Push 1 (make drops loud
+ build and run the statement verifier) is DONE; Push 2 fixes what sources *emit* now that what
they *drop* is visible. Agreed for a fresh session (user, 2026-09-04).

> **This repo is public (fictional names); institution detail is in the overlay.** Read
> `omni-me-private/ACCOUNT_MAPPING.md` + `SETUP.md` before any importer/mapping work; the
> ledger's `CONVENTIONS.md` is CANONICAL.

## Decisions in force — inherit these
- **BOTH bank sources stay OFF** — re-enabling is pointless while the imported base has holes.
- **The blocker is UPSTREAM in paisa, not omni-me** — paystub expansion stops 2023-11-30 (~2.7
  years of CPP/EI/tax absent, 66 PDFs unimported); `Assets:Pension:DC` exists only in
  `CONVENTIONS.md`. **That work belongs to `Documents-paisa`.**
- **Acceptance test = replay the statement: count AND closing balance.** Built in
  `core/src/statement/`, proven on real files, needs no `ledger` binary. Result: 10 of 12
  brokerage statements clean on both halves, all 5 transfer statements clean on balance.
- **Finance events CAN be wiped independently** — projections are domain-scoped, budget event
  types a closed set; only a type-filtered purge is missing. ⚠️ The hazard is **sync**, not the
  delete: last-write-wins, no tombstones, so another device re-seeds a wiped one.

## Do NOT re-survey
Verified statement→account mapping: `omni-me-private/examples/statement_audit_manifest.toml`
(joined on the `account:` posting tag = the id in brokerage filenames). Engine row-accounting
contract: `SUBPROCESS_SOURCE_CONTRACT.md` § disposition.

## Push 2 scope
1. **`institution`/`product` tags** on every source still emitting `tags: vec![]` —
   `auto_import/csv.rs`, `rest.rs`, `import_chequing_csv` in the app's `budget.rs`.
2. **Transfer-service fees — the open question is ANSWERED.** Statement lists the fee as its own
   row, amount **exclusive** of it; the ledger records it **net**, no `Expenses:Fees` posting.
   Verified: `-30.02` + `-0.42` = the `-30.44` recorded. Every count mismatch there is this.
3. **Startup validation of configured accounts against the real roster** — the "known gap" in
   `ACCOUNT_MAPPING.md`; makes a wrong account name loud rather than silent.

## Open threads
**Crypto is modelled monthly — ask whether that is intended** before changing it: ~60 daily
statement rows vs 4 monthly ledger aggregates, balances exact, only counts differ. · Memory
prune owed · mobile keyboard/scroll needs the test phone · format bar SHELVED behind finances.
