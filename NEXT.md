# NEXT

✅ **IMAP is ON for `gmail_personal` on dev, enrichment OFF, and the duplicate-archiving bug it
exposed is fixed and verified in production** — an idle tick logs `fetched=0`; before the fix every
tick re-archived the newest message. Image `dev-ff53bb7-1fbcb00`, 1329 tests / 0 failed.

▶ **NEXT ACTION: let it measure a day, then widen.**
1. Read volume off `auto-import tick ... fetched=N` in `docker logs omni-me-dev` (strip ANSI).
   ⚠️ The 3 document records against 1 blob predate the fix and are NOT volume.
2. Then widen: append the `[imap.gmail_work]` and `[imap.yahoo]` blocks from live's
   `credentials.toml` into `credentials-dev.toml`, server-side, never printing a value; restart.
   ⛔ That append IS the scope control — there is no per-account flag.
3. Then the APK: show `verify` warnings on the draft form, and send `x-filename` threading the
   picker's real filename rather than synthesizing one in Rust.

## ✅ Decided
- **`EXAMINE`, never `SELECT`** (his call) — server-enforced read-only, so polling the real INBOX
  has no side effect. It replaced the whole test-mailbox plan; `tasks.md` item 2 is now resolved.
- **`.edu` (`uwaterloo.ca`) PARKED** — Microsoft 365, so IMAP needs XOAUTH2 + an Azure app
  registration. ⚠️ He wants it for inbox management and scheduling, NOT receipts.

## 🔴 Decide before enrichment goes back on
**Archiving is unconditional** — every fetched message is archived plus one per attachment whatever
the handlers do (verified: an `ignored=1` message still produced a document). Enrichment therefore
costs ~2 model calls per email, the archive fills with newsletters, and there is no purge path.
Designing that purge path is the open question. ⚠️ `ingest_one` mints a fresh ULID per ingest by
design: blobs dedupe by sha256, document records never do.

## ⚠️ Open work this surfaced
- ⛔ **`UIDVALIDITY` is handled nowhere, and the dedupe fix changed its failure mode.** A renumbered
  mailbox used to self-heal through the `n:*` quirk; now it stalls with the cursor stuck, and only a
  warning names it. Real fix: carry `uid_validity` in the cursor — trait + schema change, own session.
- **The overlay's logs are invisible by default** — the public fallback filter omits
  `omni_me_private`, so a dead source reads as `sources=0`. Dev sets `RUST_LOG`; the public fix is open.

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live** (`sha-9a0b1dd`). ⛔ Never deploy there;
  snapshot before every dev mutation. Restore points: `dev-pre-imap-20260918-035321.tgz`, `-dedupe-044658`.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Cargo goes in a capped systemd unit, all three crates
  in ONE invocation. ⛔ Branches: public `dev/role-split-model-seats`, private `dev/untested-overlay`.
