# NEXT

✅ **WSL2 migration, round 1 of 2 done.** `omni-me-private/migration/` (pushed, `dev/untested-overlay`):
read-only `preflight.sh`, `HANDOFF.md` for a cold session there, `surface`'s report as the baseline.

## ▶ NEXT ACTION — two independent threads, either order

**A. The machine move — needs the WSL2 box.** There: pull the overlay, read `migration/HANDOFF.md`,
run `bash migration/preflight.sh`, then commit + push the report **by hand** (⛔ the session hooks do
NOT commit the overlay). `setup.sh` is written here against that report.

**B. The widen — needs your hands, unchanged.** ⛔ Blocked for me: the classifier refuses the
credentials append. Server-side:
`sudo tail -n +31 /etc/omni-me/credentials.toml | sudo tee -a /etc/omni-me/credentials-dev.toml > /dev/null`
then `sudo docker restart omni-me-dev`. ⛔ The append IS the scope control — no per-account flag.
Backups: `/var/omni-snapshots/dev-pre-widen-20260921-0400.tgz` + credentials copy. Then deploy to dev,
read a tick (`effective_confidence`, `needs_manual_review`, `dropped_postings`, `warnings`), then the
APK — show those warnings on the draft form, send `x-filename` from the picker not synthesized in Rust.

## ✅ Decided
- **Salvage over strictness** (2026-09-21): drop the bad line item, never the document — each posting
  becomes its own self-balancing draft. Why: `docs/src/extraction.md`.
- **`verify` uses the `EmailBody` hint on IMAP** — cross-checks a total when present, deliberately not
  penalising a missing one. **`EXAMINE`, never `SELECT`.** **`.edu` PARKED** (needs XOAUTH2).
- **The WSL2 box is adb-only** — CI builds and signs the APK, so no SDK/NDK/JDK there. Credentials do
  not move: pull `credentials-dev.toml` from the box if a bench run needs them.
- 📊 **Measured, do not re-measure:** `gmail_personal` runs **~5 messages/day** (12 in 60h: 6 drafts,
  5 unrouted, 1 failed, all archived). Cap 200/tick, label plain `INBOX` — a real rate, not a ceiling.

## ⚠️ Open
- ⛔ **`UIDVALIDITY` is handled nowhere.** A renumbered mailbox stalls with the cursor stuck. Real fix
  carries `uid_validity` in the cursor — trait + schema, own session.
- **The archive purge path** gates enrichment going back on. Enrichment stays OFF on dev.

## ⛔ Inherit
- 🔴 **Locally `fmt`, `clippy`, `check` ONLY — never the full suite.** CI runs it: public PR #1
  (`ci.yml` fires only on `main`) plus the overlay's own `ci.yml`. Why: `dev-machines-and-build-limits`.
- 🔴 **NOTHING since the stamped release (`sha-9a0b1dd`) has run on live.** ⛔ Never deploy there.
- ⚠️ **`omni-me-private` is NOT committed by the session hooks** — current repo only.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
