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
- **BOTH bank sources are OFF (user, 2026-09-04)** — account maps removed, which is the only
  durable hold. The ledger import AND the auto-import must both match the established
  conventions before either runs again; the user does not currently trust the financial data.
  UI polish on the finances screens is explicitly shelved behind this.
- **Acceptance test for a bulk import: replay the statements; COUNT and CLOSING BALANCE must
  match per statement.** `bal=0` never proves no loss (always 0 in double-entry) and an
  un-emitted row is invisible to `bal -B` and to a file manifest.
- **Fix the class, not the instance** — now a global rule. One bank source dedups against the
  ledger; every other source has none. 1.0.5 shipped; ledger-refresh fix awaits a release.

## Do NOT re-survey — it is already written down
The 2026-09-04 findings (what each source emits wrongly, the dedup limits, the box-edit
pitfalls, the verified-but-held config) are in `ACCOUNT_MAPPING.md`. The upstream ledger gaps
are in the ledger project's memory (`project_ledger_completeness_gaps`). Read, don't rederive.

## Open threads
**Finish the memory prune** (118 files / 295KB; the 143-line index is paid every session and
is the retrieval surface, so stale lines misdirect). Judge by CONTENT — notes that look like
repo duplicates by name are often non-derivable design records.
Mobile keyboard/scroll + floating insertion handle — **ask the user to connect the test phone
in-session**, else defer to a session where they have it (Android 15 disproves the API-29
theory; the test phone IS API 29, so both must work) · format bar SHELVED behind finances ·
engine-side dedup for subprocess sources · rules tier 152/150 · ledger-level verification
needs the `ledger` binary installed (it lived on the offline device; importers are Python).
