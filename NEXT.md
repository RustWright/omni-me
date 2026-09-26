# NEXT

## ▶ NEXT ACTION — 🔴 decide before 13:00Z: redeploy dev, or the live pair tests nothing new
A real order's confirmation is archived on dev and its **receipt lands 13:00–15:00Z 2026-09-26**.
⚠️ Dev runs `dev-90c9465-a0ad51a`, which PREDATES everything shipped today, so that pair exercises
the OLD path unless dev is redeployed first. Detail + the offline checks: overlay `DEV_TESTING.md`.
Then `tasks.md` § AUTONOMOUS QUEUE item 8's other half: **`pull_only` holds every pulled event in
one Vec** (up to 100k). Located, fix known, needs a progress callback first — the entry says why.

## ⛔ Waiting on the user — only these
- 🔴 **Inbox management after extraction** and **reviewing the email archive** — product calls.
  ✅ Ledger catch-up and the finances tab move to the END of testing. ⛔ Not blockers.
- 🔴 **Queue item 6, the five on-device confirmations.** ✅ `pm clear` is free — no token now.

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
- ✅ **Dev runs with NO auth token**; `POST /auto_import/tick?source=<name>` needs none. Recipe:
  overlay `DEV_TESTING.md`.
- ✅ **The Int stepper is CONFIRMED working** on the dev phone (user, 2026-09-26). ⛔ Do not re-ask.

## 🔴 Open — **phone wedge on cold backfill.** Force-stop recovers it, volume disproven, location
unknown. ⚠️ Core's logs **do** reach Android; the silent APK predated that fix, so rebuild first.

## ⛔ Standing
Branches: `dev/role-split-model-seats` (public; draft PR #1 open, so a push runs full CI — ⛔ never
run the suite on this box, it OOMs) / `dev/untested-overlay`. ⛔ Live is never a deploy target here.
⚠️ `omni-me-private` is NOT hook-committed: commit it by hand. ⛔ **IMAP work is
private-repo-tracked** — no sender or vendor detail in the public repo.
