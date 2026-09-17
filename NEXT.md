# NEXT

✅ **Capture (A) verified on the phone**; its follow-ups are DONE IN CODE but UNSHIPPED — printed-date
check, role-C ceilings, IMAP `EXAMINE`. 1327 tests / 0 failed, clippy clean, `f6074376` + `142eb3af`.

▶ **NEXT ACTION: deploy the new dev image, then turn IMAP on for `gmail_personal` only.**
1. Snapshot, then deploy run **35279629133** (`142eb3af` + overlay `e3b6936`), `DEV_TESTING.md` § 4.
2. Copy the three `[imap.*]` blocks from `/etc/omni-me/credentials.toml` into
   `credentials-dev.toml`, server-side and never printing the values. ⛔ Live's file is read-only.
3. ⚠️ Set `OMNI_ENRICH_ENABLED=0` **before** `OMNI_ENABLE_IMAP=1`, so the first ticks archive with
   zero model calls and the volume gets measured. `gmail_personal` alone.
4. Then widen to `gmail_work` + `yahoo`; then an APK for `verify` warnings on the draft form and
   `x-filename` (thread the picker's real name, don't synthesize).

## ✅ Decided by the user 2026-09-17 (second half)
- **IMAP uses `EXAMINE`, never `SELECT`** — his idea, and it replaced my test-mailbox plan
  wholesale. Server-enforced read-only, so polling his REAL INBOX has no side effect. His three
  app passwords already exist on the box from the live release; all three watch `INBOX`.
  **Scope: `gmail_personal` first**; ⛔ never flip `OMNI_ENABLE_IMAP` wider without him.
- **`.edu` (`uwaterloo.ca`) PARKED** — "talk about it later". Verified Microsoft 365, so IMAP needs
  XOAUTH2 + Azure app registration. ⚠️ He wants it for **inbox management and assistant scheduling,
  NOT receipts**, so forwarding would not serve it.

## 🔴 Decide before enrichment goes back on
**Archiving is unconditional.** `poll_once` archives EVERY fetched message plus one per attachment
regardless of which handler claims it (overlay `main.rs:304`); the 11 sender patterns gate
transaction extraction only. So with enrichment on every email costs ~2 model calls, the archive
fills with newsletters, and there is no purge path. Bounds: first run takes only the newest
message; `MAX_UIDS_PER_TICK` = 200.

## Open (full list in `DEV_TESTING.md` § 7)
R20 closed for C1 and C3 (both 8192, measured); ⚠️ no wiremock test for transcription's ceiling.
Long identifiers unreliable from C2/C3; CJK brackets once appeared inside a C2 JSON answer.

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live** (`sha-9a0b1dd`). ⛔ Never deploy there;
  snapshot `/var/omni-snapshots/` before every dev mutation. 🔴 I drive the first pass, HE the final.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Cargo goes in a capped systemd unit, all three
  crates in ONE invocation or `auto-import` is silently excluded (memory holds the recipe).
- ⛔ Branches: public `dev/role-split-model-seats`, private `dev/untested-overlay`.
