# NEXT

✅ **A partial extraction is salvaged instead of discarded, and email-sourced drafts are verified
for the first time.** A `""` amount used to fail the whole document and lose every good posting
with it; it now drops that line alone and `verify` flags the draft. 1335 passed / 0 failed.

▶ **NEXT ACTION: the widen, which needs your hands.**
1. ⛔ **Blocked for me** — the auto-mode classifier refuses the credentials append, and every
   workaround means handling the app passwords directly. Run it server-side yourself:
   `sudo tail -n +31 /etc/omni-me/credentials.toml | sudo tee -a /etc/omni-me/credentials-dev.toml > /dev/null`
   then `sudo docker restart omni-me-dev`. ⛔ That append IS the scope control — no per-account flag.
   Backups taken first: `/var/omni-snapshots/dev-pre-widen-20260921-0400.tgz` + credentials copy.
2. Then deploy this branch to dev and read a tick: handler logs now carry `effective_confidence`,
   `needs_manual_review`, `dropped_postings`, `warnings`.
3. Then the APK: show those warnings on the draft form, and send `x-filename` with the picker's
   real filename rather than synthesizing one in Rust.

## 📊 Measured — do not re-measure
`gmail_personal` runs **~5 messages/day** (12 in 60h: 6 drafts, 5 unrouted, 1 failed, all 12 archived). Cap is 200/tick and the label is plain `INBOX`, so that is a real rate, not a ceiling.

## ✅ Decided
- **Salvage over strictness** (his call, 2026-09-21): drop the bad line item, never the document.
  Safe because each posting becomes its own self-balancing draft. Why: `docs/src/extraction.md`.
- **`verify` uses the `EmailBody` hint on the IMAP path** — cross-checks a total when present,
  deliberately not penalising a missing one. **`EXAMINE`, never `SELECT`.**
- **`.edu` (`uwaterloo.ca`) PARKED** — needs XOAUTH2 + an Azure app registration.

## ⚠️ Open
- ⛔ **`UIDVALIDITY` is handled nowhere.** A renumbered mailbox stalls with the cursor stuck; only
  a warning names it. Real fix carries `uid_validity` in the cursor — trait + schema, own session.
- **The archive purge path** is the open question before enrichment goes back on. Archiving stays
  unconditional; enrichment stays OFF (`OMNI_ENRICH_ENABLED=0` on dev, verified this session).
- **Nothing reads the new `source_metadata` keys yet** — the draft form will inherit the spelling
  coupling `email_document_id` already warns about in `finances.rs`.

## ⛔ Inherit
- 🔴 **NOTHING since the stamped release (`sha-9a0b1dd`) has run on live.** ⛔ Never deploy there.
- ⚠️ **`omni-me-private` is NOT committed by the session hooks** — they cover the current repo only.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Cargo in a capped systemd unit, all three crates in
  ONE invocation. ⛔ Branches: public `dev/role-split-model-seats`, private `dev/untested-overlay`.
