# NEXT

## ▶ NEXT ACTION — 🔴 **ALL THREE DEV POLLERS ARE PAUSED. Resume them or nothing imports.**
`POST /auto_import/sources/{gmail_personal,gmail_work,yahoo}/resume` — the pause **persisted**, so
it survives restarts, and ⚠️ `/auto_import/status` still reports `health: healthy` while fetching
nothing. Paused 12:16Z 2026-09-26 to hold a real order's receipt off the stale image; `EXAMINE` + a
UID cursor mean nothing is lost and `resume` does an immediate fresh pull.
**Sequence:** image build `36243674536` → swap `OMNI_IMAGE` in `~/omni-dev/.env` → `pull && up -d` →
smoke → **resume** → the pair collapses on the NEW path. Then install APK `1.1.11-dev` (dispatched)
and work queue item 6 on the phone. Then item 8's other half: **`pull_only` holds every pulled
event in one Vec**; needs a progress callback first.

## ⛔ Waiting on the user — only these
- 🔴 **Inbox management after extraction** and **reviewing the email archive** — product calls.
  ✅ Ledger catch-up and the finances tab move to the END of testing. ⛔ Not blockers.
- ✅ **Public-repo identity scrub AUTHORISED** (2026-09-26), but ⛔ only at the END of an autonomous
  session. Sites + traps: `tasks.md` § Owed at cycle close.

## ⛔ Inherit — settled, do not re-derive
- ✅ **Queue item 6 is NOT blocked on the user.** 2026-09-26: phone answers adb at
  `100.109.41.53:5555`, AC-powered, stay-awake on. ⛔ `adb` is `/usr/bin/adb`, NOT `~/android-sdk/`.
- ✅ **Dev builds are dispatchable** — `.claude/settings.local.json` allows workflow ids
  `359907704` (dev image) and `359972206` (dev APK) only; `303640807` is LIVE and stays blocked.
- ✅ **The gate inversion is SIGNED OFF AND SHIPPED** (2026-09-26). § 5.5 → **option 2**, § 5.6 →
  **no**. ⛔ Do not re-derive it or re-run the corpus — overlay `GATE_INVERSION_DESIGN.md` § 7.
- ⚠️ **The receipt handler must stay LAST in the dispatch list** — first-match-wins order is the
  only thing giving the SC handlers their own mail, now that it claims everything.
- ⚠️ **This deploy does NOT test the paged replay.** No projection version bump and no schema
  change since `a0ad51a` (checked), so boot is a plain restart. Proving paging needs a bump.
- ✅ **Newest-wins on a collapse is CORRECT** — a confirmation is only an estimate; never rank by
  confidence.
- 🔴 **`cargo clippy -p omni-me-core` compiles NONE of `auto_import`** — pass `--features auto-import`. ✅ Dev runs with NO auth token. ✅ The Int stepper is CONFIRMED; ⛔ do not re-ask.

## 🔴 Open — **phone wedge on cold backfill**; force-stop recovers it, volume disproven, location unknown. ⚠️ Core's logs reach Android now, so use `1.1.11-dev` — the silent APK predated that fix.

## ⛔ Standing
`dev/role-split-model-seats` (public; draft PR #1 open, so a push runs full CI — ⛔ never run the
suite on this box, it OOMs) / `dev/untested-overlay` (pushed). ⛔ Live is never a deploy target
here. ⚠️ `omni-me-private` is NOT hook-committed: commit it by hand. ⛔ **IMAP work is
private-repo-tracked** — no sender or vendor detail in the public repo.
