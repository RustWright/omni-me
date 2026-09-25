# NEXT

🔴 **RULING 2026-09-25 — the sender list must stop being a GATE.** No editing a list, no rebuilding
the app, no forwarding mail to himself for a new vendor to be processed. ⛔ **Never add a vendor to
fix a miss.** ⛔ Manual forwarding is a testing workaround, not a design.

## ▶ NEXT ACTION — design the gate inversion
`tasks.md` § "THE TRANSACTION HALF WAS NEVER ANSWERED" holds the shape and the evidence.
⚠️ **Design-first, its own session** — it changes the ingestion path.
⛔ Read it before proposing anything: the 2026-09-12 ruling already required this and was only ever
half-implemented, so this is an unfinished decision, not a new idea.

## ⛔ Waiting on the user
- **Fresh Walmart/Instacart forwards sent ~14:05Z 09-25** — poll runs ~:19/:20/:22 past the half
  hour. Read the tick, then the phone over CDP. This is the outstanding **collapse** test.
- **At the next box reset: remove `[server].auth_token` from the DEV credentials file.** ⛔ Never
  touch the live one. ✅ Safe — both instances bind the tailnet address, never `0.0.0.0`, so the
  tailnet is the auth boundary. Doing it sooner unblocks `pm clear` on the phone.

## ⛔ Inherit — settled, do not re-derive
- ⛔ **Grouping runs PER DEVICE** (`build_projections_server` registers only `documents`).
  ✅ Verified on the phone; the authoring half is proven. Only the **collapse** is unproven.
- 🔴 **Grouping merges only sometimes** — the key is model-derived. ✅ It fails safe and the
  4-char floor catches junk; ⛔ do not loosen those bounds. Why: memory
  `project-model-derived-keys-are-nondeterministic`.
- ⚠️ He forwards `gmail_personal` → `gmail_work`; vendor mail *arrives* at `gmail_personal`.

## 🔴 Open
- 🔴 **Phone wedge on cold backfill** — recovers on force-stop; volume disproven; stall location
  unknown. Measurements: overlay `DEV_TESTING.md`. Repro needs `pm clear` plus the token.
- 🔴 Rebuild defects · projections swallow statement errors · suite deadlock — all in `tasks.md`.

## ⛔ Standing
Branches: `dev/role-split-model-seats` (public, draft PR #1 open so a push runs full CI) /
`dev/untested-overlay`. ⚠️ `omni-me-private` is NOT hook-committed — commit it by hand.
