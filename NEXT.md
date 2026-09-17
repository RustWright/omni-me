# NEXT

✅ **Capture (A) verified on the dev phone**, and its three follow-ups are DONE IN CODE but
UNSHIPPED: printed-date ambiguity check, role-C ceilings, IMAP `EXAMINE`. 1327 tests / 0 failed,
clippy clean, pushed as `f6074376` + `142eb3af`.

▶ **NEXT ACTION: deploy the new dev image, then turn IMAP on for `gmail_personal` only.**
1. Deploy the image from run **35279629133** (public `142eb3af` + overlay `e3b6936`), per
   `DEV_TESTING.md` § 4. Snapshot first.
2. Copy the three `[imap.*]` blocks from `/etc/omni-me/credentials.toml` into
   `/etc/omni-me/credentials-dev.toml` — server-side, never printing the values. ⛔ Live's own
   file is read-only here; dev mounts its own.
3. ⚠️ Set `OMNI_ENRICH_ENABLED=0` **before** `OMNI_ENABLE_IMAP=1`, so the first ticks archive with
   zero model calls and the volume gets measured. `gmail_personal` alone.
4. Then widen to `gmail_work` + `yahoo`; then an APK for `verify` warnings on the draft form and
   `x-filename` (thread the real picker filename, don't synthesize one).

## ✅ Decided by the user 2026-09-17 (second half)
- **IMAP uses `EXAMINE`, never `SELECT`** — his idea, and it replaced my test-mailbox plan
  wholesale. Server-enforced read-only, so polling his REAL INBOX has no side effect. His three
  app passwords already exist on the box from the live release; all three watch `INBOX`.
- **Scope: `gmail_personal` first.** ⛔ Never flip `OMNI_ENABLE_IMAP` wider without him.
- **`.edu` (`uwaterloo.ca`) PARKED** — "talk about it later". Verified Microsoft 365, so IMAP
  needs XOAUTH2 + an Azure app registration. ⚠️ He wants it for long-term **inbox management and
  assistant scheduling, NOT receipts** — forwarding would not serve it, so OAuth is the real cost.

## 🔴 Decide before enrichment goes back on
**Archiving is unconditional.** `poll_once` archives EVERY fetched message plus one per
attachment regardless of which handler claims it (overlay `main.rs:304`); the 11 sender patterns
gate transaction extraction only. With enrichment on, every email costs ~2 model calls and the
archive fills with newsletters, and there is still no purge path. Bounds: the first run fetches
only the newest message, and `MAX_UIDS_PER_TICK` = 200.

## Open, not yet worked
- R20 closed for C1 and C3 (both 8192, measured). ⚠️ No wiremock test for transcription's ceiling.
- Long identifiers unreliable from C2/C3; one C2 answer had CJK brackets inside its JSON.
- Split Wise charges: two drafts share one `external_id` (possible silent drop across batches).

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live** (`sha-9a0b1dd`). ⛔ Never deploy there.
  Snapshot `/var/omni-snapshots/` before every dev mutation.
- 🔴 I drive the first pass, HE drives the final one. ⛔ No build with crashes in it.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Every cargo run in a capped systemd unit with
  `CARGO_PROFILE_DEV_DEBUG=0`, all three crates in ONE invocation or `auto-import` is silently
  excluded. ⛔ Branches: public `dev/role-split-model-seats`, private `dev/untested-overlay`.
