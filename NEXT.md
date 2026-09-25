# NEXT

🔴 **RULING 2026-09-25 — the sender list must stop being a GATE.** No editing a list, no rebuilding
the app, no forwarding mail to himself for a new vendor to be processed. ⛔ **Never add a vendor to
fix a miss.** ⛔ Manual forwarding is a testing workaround, not a design.

## ▶ NEXT ACTION — design the gate inversion, then work down the stretch
`tasks.md` § "THE TRANSACTION HALF WAS NEVER ANSWERED" holds the shape and the evidence.
⚠️ **Design-first** — it changes the ingestion path, and he signs off before I build.
⛔ Read it first: the 2026-09-12 ruling already required this and was only half-implemented, so this
is an unfinished decision, not a new idea.

**Then work `tasks.md` § "AUTONOMOUS QUEUE" top-down** — eight items, none of them blocked on him.

## ⛔ Waiting on the user — only these
- 🔴 **The Int stepper in Settings** — native control, so Playwright is not faithful. He will look
  after this stretch; ⚠️ **tell him exactly what to check** (see `tasks.md` § Awaiting on-device).
- 🔴 **Sign-off on the gate-inversion design** once written.
- 🔴 **Inbox management after extraction** and **reviewing the email archive** — open since
  2026-09-11, both product calls, neither blocking now.
- ✅ **Ledger catch-up and turning the tab on move to the END of testing** (user, 2026-09-25) — they
  are live-environment decisions to take once we are confident in the app. ⛔ Not blockers.

## ⛔ Inherit — settled, do not re-derive
- ✅ **Model exploration is OPEN** (user, 2026-09-25): if inference quality is the problem, try better
  models — ⛔ but only **within the established ZDR / privacy framework**, extra hops included.
- ✅ **Newest-wins on a collapse is CORRECT** — a confirmation is only an estimate (weight-priced and
  unavailable items); the delivery receipt is authoritative. ⛔ Never rank by confidence instead.
- ⛔ **Grouping runs PER DEVICE.** ✅ Fully verified on the phone, collapse included. ⛔ Done.
- 🔴 **Grouping merges only sometimes** — the key is model-derived. ✅ Fails safe; ⛔ do not loosen
  `group_key()`'s bounds. Why: memory `project-model-derived-keys-are-nondeterministic`.
- ✅ **Dev runs with NO auth token** and `POST /auto_import/tick?source=<name>` needs no auth —
  **3.4s per mail test, not 30 minutes.** `pm clear` is safe now. Recipe: overlay `DEV_TESTING.md`.

## 🔴 Open
- 🔴 **Phone wedge on cold backfill** — force-stop recovers it, volume disproven, location unknown;
  repro now costs nothing. Measurements: overlay `DEV_TESTING.md`.

## ⛔ Standing
Branches: `dev/role-split-model-seats` (public; draft PR #1 open, so a push runs full CI — ⛔ never
run the suite on this box, it OOMs) / `dev/untested-overlay`. ⚠️ `omni-me-private` is NOT
hook-committed: commit it by hand.
