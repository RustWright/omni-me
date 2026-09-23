# NEXT

✅ **Three decisions answered 2026-09-23**, answerable half of each built, new prompt run against
real mail on dev. ⛔ Evidence in overlay `DEV_TESTING.md` § 8. Dev on `dev-1b3ef42-25b55ed`,
healthy. Rollback `/var/omni-snapshots/dev-pre-kindgate-20260923-1412.tgz`.

## ▶ NEXT ACTION — settle the two grouping questions, then build grouping
⛔ **Both need the user. Do not guess either.**
- 🔴 **What groups a vendor's mail with no order number?** Rec: nothing — a duplicate is visible
  and costs a dismissal, a wrong merge is invisible and loses a transaction.
- 🔴 **What does a revision do to an already-committed batch?** Rec: a new review item that never
  touches committed books. ⛔ Amending them directly crosses the autonomy line.

## ✅ Proven on dev against real mail
- **Config persistence FIXED.** Paused a source → 200 (was 500), survived a restart (`paused=0`
  before, `paused=1` after), resumed cleanly. The off-switch persists for the first time.
- **`document_kind` right on all 8** real emails; **order numbers exact**; hints did NOT regress.
- **Unreadable `total` no longer kills a document**: 8/8 extract, was 2/8.

## ⚠️ Open
- 🔴 **"One Walmart order produced SIX batches" is now UNCONFIRMED.** The two archived mails carry
  **different** order numbers (`…118762884` vs `…124202519`), so some of those six may be separate
  purchases. ⛔ Re-check against the mailbox before designing grouping on that premise.
- 🔴 **`order_ref` is junk on non-order mail** (subject lines, `.`, a UUID). ⛔ Never group on it
  without first checking the kind is charge-bearing.
- ⚠️ **The survey/upsell mails were never tested** — not archived. They are the real culprits.
- ⛔ **`29695a4` is not in the deployed image**; dev runs one commit behind.
- 🔴 **Suite deadlock REPRODUCED** (1 in 6 runs): 312 leaked `surrealkv-commit` threads from the
  `test_db()` fixture. Mechanism + fix plan in `tasks.md`; the fix touches 217 call sites.
- ⛔ **`UIDVALIDITY` handled nowhere.** Own session.

## ⛔ Inherit
- 🔴 **CI's ONE invocation naming core+server+agent** is what compiles core with `auto-import` —
  a narrower `cargo test -p omni-me-core` runs ZERO of those tests and looks like a pass.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⛔ Never deploy there; live has no sources
  running. ⚠️ `omni-me-private` is NOT committed by the hooks — by hand, current repo only.
- ⚠️ **Search the trackers before writing a bug up as new** — the `/config` bug was in `tasks.md`
  from 2026-09-07 and got re-derived as a discovery.
- ⚠️ Strip ANSI before grepping docker logs. **`EXAMINE`, never `SELECT`.** ⛔ Branches:
  `dev/role-split-model-seats` / `dev/untested-overlay`.
