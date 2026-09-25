# NEXT

## ▶ NEXT ACTION — he signs off on two calls, then I build § 5.1 + § 5.2
✅ **The gate-inversion design is WRITTEN and MEASURED**: overlay `GATE_INVERSION_DESIGN.md`. It
answers the 2026-09-25 ruling (the sender list must stop being a gate; ⛔ never add a vendor to fix a
miss). ⛔ Do not re-derive it and do not re-run the corpus.
**Recommendation: delete the gate outright and add no filter.** On 54 real emails the gate scores 82%
recall / 67% precision; no gate at all scores **94/94**, and 100% with § 5.3. Filtering nothing costs
**$0.10/month**, so cost was never the argument. Then `tasks.md` § AUTONOMOUS QUEUE — **only item 8's
whole-log-in-memory half is left**, release-blocking at 1.3 GB peak on the S9, and a fresh install's
first sync hits it too.

## ⛔ Waiting on the user — only these
- 🔴 **§ 5.5 the injection posture** — accept it (my pick), add SPF/DKIM as a draft *signal*, or keep
  a gate. A risk posture rather than an engineering trade, so it is his.
- 🔴 **§ 5.6 candidate grouping keys** — yes or no. ⛔ No is defensible; I did not build it.
- 🔴 **The Int stepper, Settings → Assistant** (check-in hour 0–23, max turns 3–50). What to check:
  `−`/`+` are tappable, the number is centred and does not reflow the row as it widens, both buttons
  grey out at the bounds, and the `0–23` / `3–50` hint is legible.
- 🔴 **Inbox management after extraction** and **reviewing the email archive** — product calls.
  ✅ Ledger catch-up and the finances tab move to the END of testing. ⛔ Not blockers.

## ⛔ Inherit — settled, do not re-derive
- ✅ **Queue items 1, 3, 4, 5, 7 are DONE; 2 is blocked on § 5.6.** ⚠️ Three of those entries were
  **stale or wrong as written** — read the entry in `tasks.md` before believing any premise there.
- 🔴 **`cargo clippy -p omni-me-core` compiles NONE of `auto_import`** — non-default feature, so
  local lint greens without reading the IMAP stack. Pass `--features auto-import`.
- ✅ **Newest-wins on a collapse is CORRECT** — a confirmation is only an estimate; the delivery
  receipt is authoritative. ⛔ Never rank by confidence instead.
- ✅ **Dev runs with NO auth token** and `POST /auto_import/tick?source=<name>` needs none — seconds
  per mail test, and `pm clear` is safe. Recipe: overlay `DEV_TESTING.md`.

## 🔴 Open — **phone wedge on cold backfill.** Force-stop recovers it, volume disproven, location
unknown. ⚠️ Core's logs **do** reach Android; the APK that looked silent predated that fix, so
rebuild the APK before reading anything into a quiet logcat.

## ⛔ Standing
Branches: `dev/role-split-model-seats` (public; draft PR #1 open, so a push runs full CI — ⛔ never
run the suite on this box, it OOMs) / `dev/untested-overlay`. ⛔ Live is never a deploy target here.
⚠️ `omni-me-private` is NOT hook-committed: commit it by hand. ⛔ **IMAP work is
private-repo-tracked** — no sender or vendor detail in the public repo.
