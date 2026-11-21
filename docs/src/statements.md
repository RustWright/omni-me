# Checking an import against the bank

A bulk import of a few hundred rows cannot be validated by a person reading them; pretending
otherwise produces rubber-stamping. The obvious automatic check does not work either: in
double-entry bookkeeping a transaction that was never recorded balances perfectly by being absent,
so "the books balance" proves nothing about whether they are complete. Real gaps of exactly that
kind, months of missing deductions and an account that existed in no ledger file, balanced
exactly and were wrong.

The only check that catches that class compares against something outside the books. A bank
statement is that oracle (`core/src/statement/`).

## Both counts and balances, per statement

For one account over one period, two things must match the statement: the number of
transactions and the closing balance. Neither is enough alone.

- Counts alone miss a wrong amount: the right number of rows, one of them incorrect.
- Balances alone miss an offsetting pair: one row dropped and another duplicated nets to zero.

Neither is enough in aggregate either. Summing a year hides a month where a missing debit and a
missing credit cancel, which is how a reconciliation that compared monthly totals let a
multi-year gap survive. So the check runs per statement, per account, with both halves.

A verdict is not a boolean. It names the rows that appear on one side and not the other, because
"the closing balance is off by 412.30" is not actionable and "these four transactions are in the
statement and not in the books" is.

## The oracle has to be trustworthy first

A verifier built on a parser that quietly skips rows proves nothing, so no parser may `continue`
past a line it does not understand. `StatementParse` carries a skip ledger and its own
accounting identity (every line seen is a row, structure, or a recorded skip), and the parse is
checked against the statement's own arithmetic before any of it is trusted: where a running
balance exists, each row must move it by exactly its amount. That one check catches an inverted
sign, a mis-mapped column, a row read twice and a row skipped, all of which otherwise produce a
plausible, balanced, wrong result.

## Rendered statements

Some banks publish no export, only a monthly PDF laid out for a person, where columns are
positions on a page. Position-based parsing is exactly what the CSV parsers avoid, and that
warning stands. What makes it safe here is that a statement written for a reader states figures
about itself: an opening and closing balance and, depending on the layout, per-row running
balances, totals or transaction counts. Boilerplate misread as a transaction breaks the declared
count, and a transaction misread as boilerplate breaks it the other way, so failures are caught by
arithmetic the bank published rather than a rule the parser invented.

The text must come from a layout-preserving extraction (`pdftotext -layout`). Without it every
amount collapses into the same position and the parse becomes confident nonsense rather than a
loud failure.

A bank can change its statement design partway through the record, sharing no structure between
the two forms. An earlier importer handled only the newer one and silently skipped years of
statements, which then looked like an account that opened mid-life with an unexplained balance.
So a file matching no known layout is rejected outright rather than parsed partially: an
unreadable statement is a visible problem, and a half-read one is not.
