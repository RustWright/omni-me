# NEXT

## ▶ NEXT ACTION — he signs off on two calls, then I build § 5.1 + § 5.2
✅ **The gate-inversion design is WRITTEN and MEASURED**: overlay `GATE_INVERSION_DESIGN.md`. It is
the answer to the 2026-09-25 ruling (the sender list must stop being a gate; ⛔ never add a vendor to
fix a miss). ⛔ Do not re-derive it and do not re-run the corpus.
**Recommendation: delete the gate outright and add no filter.** On 54 real emails the gate scores 82%
recall / 67% precision; no gate at all scores **94/94**, and 100% recall with § 5.3. A filter-free
path costs **$0.10/month**, so cost was never the argument for one.
Meanwhile: `tasks.md` § AUTONOMOUS QUEUE items **3–8** (1 and 2 are answered below).

## ⛔ Waiting on the user — only these
- 🔴 **§ 5.5 the injection posture** — accept it (my pick), add SPF/DKIM as a draft *signal*, or keep
  a gate. A risk posture rather than an engineering trade, so it is his.
- 🔴 **§ 5.6 candidate grouping keys** — yes or no. ⛔ No is defensible; I did not build it.
- 🔴 **The Int stepper, Settings → Assistant** (check-in hour 0–23, max turns 3–50). What to check:
  `−`/`+` are tappable, the number is centred and does not reflow the row as it widens, both buttons
  grey out at the bounds, and the `0–23` / `3–50` hint is legible.
- 🔴 **Inbox management after extraction** and **reviewing the email archive** — product calls.
- ✅ **Ledger catch-up and the finances tab move to the END of testing.** ⛔ Not blockers.

## ⛔ Inherit — settled, do not re-derive
- ✅ **Queue item 1 is ANSWERED + shipped.** Not a failing document class — one document flapping
  `0,0,0,4,6,7` postings on identical bytes, while reading its own total right every time.
  `extract_reconciled` re-asks when the line items miss a stated total. ⛔ Not a model problem.
- ✅ **Queue item 2's premise is WRONG** — the receipt prints no order URL, and the delivery platform
  prints two ids for one order. Labelled text already works; grouping needs § 5.6. ⛔ Don't build it.
- ✅ **Newest-wins on a collapse is CORRECT** — a confirmation is only an estimate; the delivery
  receipt is authoritative. ⛔ Never rank by confidence instead.
- ✅ **Dev runs with NO auth token** and `POST /auto_import/tick?source=<name>` needs none — 3.4s per
  mail test, and `pm clear` is safe. Recipe: overlay `DEV_TESTING.md`.

## 🔴 Open
- 🔴 **Phone wedge on cold backfill** — force-stop recovers it, volume disproven, location unknown.

## ⛔ Standing
Branches: `dev/role-split-model-seats` (public; draft PR #1 open, so a push runs full CI — ⛔ never
run the suite on this box, it OOMs) / `dev/untested-overlay`. ⚠️ `omni-me-private` is NOT
hook-committed: commit it by hand. ⛔ **IMAP work is private-repo-tracked** — no sender or vendor
detail in the public repo.
