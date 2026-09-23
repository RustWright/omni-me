# NEXT

✅ **Box done + widen done (2026-09-22).** Dev now runs **3 IMAP sources** (`gmail_personal`,
`gmail_work`, `yahoo`) — `sources=3 paused=0`, healthy; first ticks fetched real mail and ignored
non-receipts. Frontend dev dist built (bundles + tailwind), Playwright chromium cached.

## ▶ NEXT ACTION — dev image is building, then read a tick

⏳ **`build-dev-image.yml` run `35810758014`**, dispatched 2026-09-23 02:32Z, ~18min — overlay
`dev/untested-overlay` + public `dev/role-split-model-seats`.
⚠️ **Dispatch it with `--ref dev/untested-overlay`**; the default `main` builds the *wrong overlay*
and the run looks identical until it is too late.
⛔ It does NOT deploy, by design. When green: on the box pull the `dev-<priv>-<pub>` tag and run it
beside live (second dir, second port), then read a tick for `effective_confidence`,
`needs_manual_review`, `dropped_postings`, `warnings`. Then the APK — surface those warnings on
the draft form, send `x-filename` from the picker not synthesized in Rust.

## ✅ Decided
- **Salvage over strictness** (2026-09-21): drop the bad line item, never the document — each
  posting becomes its own self-balancing draft. Why: `docs/src/extraction.md`.
- **`verify` uses the `EmailBody` hint on IMAP**, deliberately not penalising a missing one.
  **`EXAMINE`, never `SELECT`.** **`.edu` PARKED** (needs XOAUTH2).
- **The dev box is adb-only**, credentials do not move. Its facts and its three traps (Windows
  `npm`, glibc/`dx`, adb re-auth) live in overlay `SETUP.md` § Machines — ⛔ not re-derived here.
- 📊 **Measured, do not re-measure:** `gmail_personal` ~**5 msg/day** (12 in 60h) — a real rate,
  not a ceiling. Cap 200/tick, label plain `INBOX`. Cold `cargo check` = 7m36s at JOBS=2.

## ⚠️ Open
- ⛔ **`UIDVALIDITY` is handled nowhere.** A renumbered mailbox stalls, cursor stuck. Real fix
  carries `uid_validity` in the cursor — trait + schema, own session.
- **The archive purge path** gates enrichment going back on. Enrichment stays OFF on dev.

## ⛔ Inherit
- 🔴 **Locally `fmt`, `clippy`, `check` ONLY — never the full suite.** CI runs it.
- 🔴 **NOTHING since the stamped release (`sha-9a0b1dd`) has run on live.** ⛔ Never deploy there.
- ⚠️ **`omni-me-private` is NOT committed by the session hooks** — current repo only, by hand.
- ⚠️ `auto:` commits carry real code, not just checkpoints — diff them, never filter them out.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
