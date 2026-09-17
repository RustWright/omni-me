# NEXT

✅ **Capture (A) verified end to end on the dev phone** (1.1.4-dev + server `baf9b7af`): the photo
is archived with the C1 reading as its fields, the draft carries an `Unmatched` leg sized from the
printed total, Save refuses an unbalanced draft, and the saved transaction links `document_id`.

▶ **NEXT ACTION: finish the capture path's small follow-ups, then continue the chain.**
1. C1 reads `09/03/26` day-first (a September receipt filed in March), fix it.
2. Show `verify` warnings on the draft form; make the balance refusal name the line items when
   an `Unmatched` leg is already present.
3. The app sends no `x-filename`, so every capture is archived as `attachment`.
Then: finance propose → IMAP on dev → ledger catch-up. Full findings: overlay `DEV_TESTING.md`.

## ✅ Decided by the user 2026-09-17 (reasoning: overlay `DEV_TESTING.md`)
- **Server keeps `documents`, and push projects.** Object storage / separate agent = FUTURE only.
- **Capture → archive is option (A).** 2 model calls per receipt; transaction links the document.
- **Ledger catch-up: decide when we reach it.** Local `.reference/paisa-ledger/`, newest txn
  2026-08-28; he supplies newer statements when asked.
- 🔴 **Test with REAL documents** (ZDR DeepInfra chosen for that). ⛔ Never a non-ZDR endpoint.
- Privacy eye (hide all numbers) is BACKLOG, sized in `tasks.md` § Finances.

## Open findings, not yet worked
- R20: C1 (measured 251 tokens) and C3 (38–356/page) still uncapped; C2 capped at 2048.
- Long identifiers unreliable from C2/C3; one C2 answer had CJK brackets inside the JSON.
- Split Wise charges: two drafts share one `external_id` (possible silent drop across batches).
- `/sync/pull` treats a zero-fraction `since` as the end of that second (clients unaffected).

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live** (`sha-9a0b1dd`, OTA 1.1.2). ⛔ Never
  deploy there. Snapshot `/var/omni-snapshots/` before every dev mutation.
- 🔴 I drive the first pass, HE drives the final one. ⛔ Don't hand him a build with crashes in it.
- ⚠️ Any sync change runs `cargo test -p omni-me-server`; core `sync::` alone missed a regression.
- ⚠️ Dev phone over USB `5431324d59563398`; wireless drops when it leaves Wi-Fi. Drive the app via
  `adb forward` + scratchpad `cdp.mjs`; re-forward after every relaunch (socket named by PID).
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Every cargo run in a capped systemd unit with
  `CARGO_PROFILE_DEV_DEBUG=0`. The public repo's privacy guard rejects real institution names.
- ⛔ Branches: public `dev/role-split-model-seats`, private `dev/untested-overlay`.
