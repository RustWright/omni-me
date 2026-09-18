# NEXT

✅ **Dev runs the new image with IMAP ON for `gmail_personal` alone**, enrichment OFF. Turning it on
surfaced a duplication bug within two ticks; fixed in `e810ff3e`, 1329 tests / 0 failed.

▶ **NEXT ACTION: deploy run 35306171163, then confirm an idle tick fetches nothing.**
1. Snapshot, set `OMNI_IMAGE` to the new `dev-<priv>-e810ff3e` tag, `docker compose pull && up -d`.
2. Confirm an idle tick logs `fetched=0`. Before the fix EVERY tick re-archived the newest message.
3. Leave it a day, enrichment still OFF, and read archive volume off the tick counts.
4. Then widen to `gmail_work` + `yahoo` (append their `[imap.*]` blocks); then the APK for `verify`
   warnings on the draft form and `x-filename` (thread the picker's real name, don't synthesize).

## ✅ Decided
- **`EXAMINE`, never `SELECT`** (his call) — server-enforced read-only, so polling the real INBOX
  has no side effect. His three app passwords already live in live's `credentials.toml`.
- **Scope is the config file, not a flag** — `build_imap_sources` loops every `[imap.*]` block and
  there is no per-account toggle. ⛔ Never widen without him.
- **`.edu` (`uwaterloo.ca`) PARKED** — Microsoft 365, so IMAP needs XOAUTH2 + an Azure app
  registration. ⚠️ He wants it for inbox management and scheduling, NOT receipts.

## 🔴 Decide before enrichment goes back on
**Archiving is unconditional** — every fetched message is archived plus one per attachment whatever
the handlers do (verified: an `ignored=1` message still produced a document). Enrichment therefore
costs ~2 model calls per email, the archive fills with newsletters, and there is no purge path.
Designing that purge path is the open question. ⚠️ `ingest_one` mints a fresh ULID per ingest by
design: blobs dedupe by sha256, document records never do.

## ⚠️ Found today, keep
- **The overlay's own logs are invisible by default.** The filter is
  `omni_me_server=debug,omni_me_core=info,tower_http=debug` and the overlay binary logs under
  `omni_me_private`, matching nothing. Dev's compose now sets `RUST_LOG`. A failed `ImapSource::new`
  would otherwise appear only as `sources=0`, with no reason given.
- ⚠️ His AnyConnect VPN registers `~.` with resolved, killing ALL DNS when its resolvers are
  unreachable; quitting the GUI does not disconnect it. Tailnet-by-IP works throughout.

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live** (`sha-9a0b1dd`). ⛔ Never deploy there;
  snapshot before every dev mutation. Restore point: `dev-pre-imap-20260918-035321.tgz`.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Cargo goes in a capped systemd unit, all three crates
  in ONE invocation. ⛔ Branches: public `dev/role-split-model-seats`, private `dev/untested-overlay`.
