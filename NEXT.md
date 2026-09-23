# NEXT

✅ **All three parked decisions answered 2026-09-23** and the answerable half of each is built,
clippy-clean and pushed. ⛔ Full evidence lives in overlay `DEV_TESTING.md` § 8 — not repeated here.
Rollback `/var/omni-snapshots/dev-pre-kindgate-20260923-1412.tgz`.

## ▶ NEXT ACTION — validate the new prompt against a real model

🔴 **The `EmailBody` prompt changed and NOTHING has run against a model yet.** Dev image build
`35872622803` was in flight at handoff. Deploy it (overlay `DEV_TESTING.md` § 4), tick the
mailbox, and check on the real Walmart mails that `document_kind` and `order_ref` come back
sensibly. ⚠️ A previous prompt rewrite silently degraded account hints, so check the OLD fields
too, not just the new ones. Unit tests cover the gate, never the model's answers.

## ✅ Decided 2026-09-23
- **Receipts: the model decides, keyed on the vendor's order number** (user). Over-communication
  is universal — Amazon, Oxio, Instacart and Walmart all send confirm/update/ship/feedback mail.
  ⛔ NOT `References:`: a forward starts a new thread, which breaks exactly the case in dev.
- **`/config` bug: ships with the branch, live is NOT patched** (user). Live has no sources
  running, so a non-persisting off-switch there is not worth a deploy.
- **`verify` records when it verified nothing** (`TotalCheck`), no confidence change.

## ⚠️ Open — needs the user, do not guess
- 🔴 **What groups a vendor's mail with no order number?** My rec: nothing — a duplicate is
  visible and costs a dismissal, a wrong merge is invisible and loses a transaction.
- 🔴 **What does a revision do to an already-committed batch?** My rec: a new review item that
  never touches committed books. ⛔ Amending them directly crosses the autonomy line.
- ⛔ **Grouping stays unwritten until both are answered** — dedup key, revision semantics and the
  mapper's draft shape all move together.
- ⛔ **`UIDVALIDITY` handled nowhere.** Real fix carries `uid_validity` in the cursor; own session.
- 🔴 **`assistant_projection::tests::arrival_order_…_clocks` hangs ~1 in 5.** `timeout-minutes: 45`
  caps it in both repos; the hang itself is undiagnosed.

## ⛔ Inherit
- 🔴 **Locally `fmt`/`clippy`/`check` ONLY.** CI's ONE invocation naming core+server+agent is what
  compiles core with `auto-import` — a narrower `cargo test -p omni-me-core` runs ZERO of those
  tests and looks like a pass.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⛔ Never deploy there. ⚠️ `omni-me-private`
  is NOT committed by the session hooks — by hand, current repo only.
- ⚠️ **Search the trackers before writing a bug up as new** — the `/config` bug was already in
  `tasks.md` from 2026-09-07 and got re-derived as a discovery.
- ⚠️ Strip ANSI before grepping docker logs. **`EXAMINE`, never `SELECT`.** Salvage over
  strictness. ⛔ Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
