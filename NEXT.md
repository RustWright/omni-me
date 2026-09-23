# NEXT

✅ **Dev runs the new code (2026-09-23)** — `dev-091bdb4-fc665d7`, healthy, 3 IMAP sources, all
four LLM seats wired. Rollback `/var/omni-snapshots/dev-pre-verify-20260923-0257.tgz`. ⛔ Full
evidence is in overlay `DEV_TESTING.md` § 7 and is not repeated here.

## ▶ NEXT ACTION — test the new email prompt on the box

⏳ **Dev image build `35816027924`** (dispatched 03:52Z, ~21min) carries the conditional
`EmailBody` prompt + the `already_balanced` fix. When green: repoint `~/omni-dev/.env`, then run
`extract3.sh` (in this session's scratchpad; rewrite if gone — three bodies, `hint=email_body`).
**Predictions to check:** clean → 6 line items, total 29.14, no warning · mismatch → items 38.68
vs total 61.84, **warning fires** · Netflix → ONE posting, **positive** 12.99, `total` null.
Then the APK: `build-dev-apk.yml`, surface `warnings`/`needs_review` on the draft form (code is
written and pushed, never run on hardware).

## ✅ Decided
- **`EmailBody` gets a conditional prompt, not sender routing** (user, 2026-09-23). Itemised →
  line items + printed grand total; single amount → one posting, `total` null. Why: most of his
  senders are single-amount, so a uniform `total` rule makes a check that can never fail — and
  the sender list is meant to grow by approval, so per-sender config compounds. See memory
  `project-receipt-sender-list-should-learn`.
- **Salvage over strictness** (2026-09-21): drop the bad line item, never the document.
- **`EXAMINE`, never `SELECT`.** **`.edu` PARKED** (needs XOAUTH2).
- 📊 **Measured, do not re-measure:** `gmail_personal` ~**5 msg/day**; cap 200/tick, label `INBOX`.
  At JOBS=2: cold `cargo check` 7m36s · core clippy (`auto-import`) 6m42s · app clippy 7m08s.

## ⚠️ Open
- 🔴 **`events::assistant_projection::tests::arrival_order_does_not_change_a_threads_clocks` hangs
  intermittently** (~1 in 5; run `35811685375` sat >1h, 4 others pass at ~1m50s). Named by diffing
  completed tests against a green run — it is the only one that started and never reported. It is
  the rare test calling `test_db()` **twice**, the `tempdir + mem::forget` fixture already blamed
  for the 2026-05-16 flake. `timeout-minutes: 45` now caps both repos so a repeat fails loud.
- ⛔ **The unattended IMAP path has still never run end-to-end.** All ticks `fetched:0`. Needs a
  real receipt email in a watched mailbox — **ask the user to forward one**.
- ⛔ **`UIDVALIDITY` is handled nowhere.** Real fix carries `uid_validity` in the cursor; own session.

## ⛔ Inherit
- 🔴 **Locally `fmt`, `clippy`, `check` ONLY — never the full suite.** CI runs it.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⛔ Never deploy there. ⚠️ `omni-me-private`
  is NOT committed by the session hooks — commit it by hand, current repo only.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
