# NEXT

**Next action: a planning-first session on finance data integrity** — cross-cutting (possible
finance-event reset, statement reconciliation, receipt/paystub matching legs), so it gets its
own session. Do NOT start it piecemeal. **Requirement, in the user's words:** auto-import must
emit entries matching the established ledger conventions, with confidence that **nothing is
silently dropped**; matching legs come from receipts (purchases) and paystubs (salary).

> **Institution detail lives in the PRIVATE overlay** — this repo is public and uses
> fictional names. Read `omni-me-private/ACCOUNT_MAPPING.md` and `SETUP.md` before touching
> any importer, mapping, or box assumption; the ledger's `CONVENTIONS.md` is CANONICAL.

## Decisions in force — inherit these
- **The brokerage subprocess source is verified but deliberately NOT enabled.** A dry run
  proved the mapping (150 new + 145 pre-cutoff = 295, correctly tagged); it stays off until
  reconciliation can verify it — 150 rows can't be hand-reviewed and would be rubber-stamped.
- **Acceptance test for a bulk import: replay the statements; COUNT and CLOSING BALANCE must
  match per statement.** `bal=0` never proves no loss (always 0 in double-entry), and an
  un-emitted row is invisible to `bal -B` and to a file manifest — only closing-balance
  reconciliation catches it. (Learned in the ledger project; re-derived today.)
- **Fix the class, not the instance** — now a global rule. Exactly one bank source dedups
  against the ledger; every other source has no dedup at all.
- 1.0.5 shipped. Ledger-refresh fix committed, awaiting the next release.

## Findings 2026-09-04 — verified, not inferred (detail in the private docs)
- **Paystub expansion stops 2023-11-30.** Later deposits record NET pay as gross; ~2.7 yrs of
  payroll deductions absent, and 66 source PDFs sit unused. **The gap is upstream, in the
  ledger project — not in omni-me's import.**
- **Two accounts the conventions doc describes have never existed in any ledger file.**
- **One statement source maps to an account tree with 0 occurrences.** Fix needs config AND
  tags together — `statement_extraction_to_drafts` emits `tags: vec![]`, so the pooled
  account would otherwise make that money unattributable.
- **Security/crypto orders arrive as the CASH leg** — security balances never update.
- **`pause` is not durable** (500; read-only config mount) — hold a source off by removing
  its account map. **Edit box config with `cp`, never `sed -i`** (single-file bind mount:
  an inode swap makes the edit invisible to the container until a restart).

## Open threads
Mobile keyboard/scroll + floating insertion handle (Android 15 disproves the API-29 theory;
the test phone IS API 29, so both must work) · contextual format bar on selection ·
engine-side dedup for subprocess sources · global rules tier is 152/150 lines.
