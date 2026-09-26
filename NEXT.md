# NEXT

## ▶ NEXT ACTION — the Walmart delivery mail, then `pull_only`
🔴 **A live test is waiting on the dev box.** He ordered from Walmart late on 2026-09-25 and the
confirmation arrived; the **delivery mail lands 09:00–11:00 on 2026-09-26** with the final cost.
That pair is the first real mail to hit the retired gate, the `Other` fix, the printed-key parse
and the sender-auth flag all at once. Read the review item it produces before building anything.
Then `tasks.md` § AUTONOMOUS QUEUE item 8's remaining half: **`pull_only` holds every pulled event
in one Vec** (up to 100k). Located, fix known, needs a progress callback first — the entry says why.

## ⛔ Waiting on the user — only these
- 🔴 **Inbox management after extraction** and **reviewing the email archive** — product calls.
  ✅ Ledger catch-up and the finances tab move to the END of testing. ⛔ Not blockers.
- 🔴 **The five remaining on-device confirmations** (queue item 6). ✅ `pm clear` is free — no
  token to re-enter.

## ⛔ Inherit — settled, do not re-derive
- ✅ **The gate inversion is SIGNED OFF AND SHIPPED** (2026-09-26). § 5.5 → **option 2**: accept
  the exposure, flag an unauthenticated sender on the draft. § 5.6 → **no**: no candidate keys,
  `group_key()`'s bounds unchanged; the stability parse shipped alone. ⛔ Do not re-derive any of
  it and do not re-run the corpus — overlay `GATE_INVERSION_DESIGN.md` § 7 is the record.
- ⚠️ **The receipt handler must stay LAST in the dispatch list.** It claims everything now, and
  first-match-wins order is the only thing giving the SC handlers their own mail.
- ✅ **Queue items 1–5 and 7 are DONE; 6 needs the phone; 8 is half done.** ⚠️ Items 2, 4 and 5
  were **stale or wrong as written** — read the entry before believing any premise in it.
- 🔴 **`cargo clippy -p omni-me-core` compiles NONE of `auto_import`** — pass `--features
  auto-import`, or local lint greens without reading the IMAP stack.
- ✅ **Newest-wins on a collapse is CORRECT** — a confirmation is only an estimate; the delivery
  receipt is authoritative. ⛔ Never rank by confidence instead.
- ✅ **Dev runs with NO auth token** and `POST /auto_import/tick?source=<name>` needs none.
  Recipe: overlay `DEV_TESTING.md`.
- ✅ **The Int stepper is CONFIRMED working** on the dev phone (user, 2026-09-26): greys out at
  both bounds, follow sets and resets. ⛔ Closed, do not re-ask.

## 🔴 Open — **phone wedge on cold backfill.** Force-stop recovers it, volume disproven, location
unknown. ⚠️ Core's logs **do** reach Android; the APK that looked silent predated that fix, so
rebuild the APK before reading anything into a quiet logcat.

## ⛔ Standing
Branches: `dev/role-split-model-seats` (public; draft PR #1 open, so a push runs full CI — ⛔ never
run the suite on this box, it OOMs) / `dev/untested-overlay`. ⛔ Live is never a deploy target here.
⚠️ `omni-me-private` is NOT hook-committed: commit it by hand. ⛔ **IMAP work is
private-repo-tracked** — no sender or vendor detail in the public repo.
