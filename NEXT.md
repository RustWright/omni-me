# NEXT

▶ **NEXT ACTION: deploy + verify capture option (A) end to end on the dev phone.** Server
`182fc867` and app `c240b325` are committed, NOT yet deployed or installed (image and 1.1.4-dev
APK building in CI). On the phone: Finances → Add → Photo with a real receipt, then check
(1) the draft carries an `Unmatched` counter leg, (2) Save refuses an unbalanced draft,
(3) the capture appears in Archive with the extraction's kind/title/total and is NOT re-read by
the C2 pass, (4) the saved transaction's attachment carries `document_id`.
Then continue the chain: finance propose → IMAP on dev → ledger catch-up.

## ✅ Decided by the user 2026-09-17 (reasoning: overlay `DEV_TESTING.md`)
- **Server keeps `documents`, and push projects.** A table lives where the work reading it
  runs. Object storage for blobs / agent on its own machine are FUTURE goals only.
- **Capture → archive is option (A)**: one request archives the photo and records the C1
  reading as its fields (2 model calls/receipt, transaction links to the document).
- **Ledger catch-up: decide when we reach it.** Local copy `.reference/paisa-ledger/`, newest
  txn 2026-08-28; he supplies newer statements when asked.
- 🔴 **Test with REAL documents.** DeepInfra was chosen for zero retention precisely so real
  material can be used. ⛔ Never route real content to a non-ZDR endpoint.
- Privacy eye (hide all numbers) is BACKLOG, sized in `tasks.md` § Finances.

## Open findings, not yet worked (full list: overlay `DEV_TESTING.md` PROGRESS LOG)
- C1 reads `09/03/26` day-first (filed a September receipt in March).
- `verify()` (receipt total cross-check) is never called outside its tests.
- Long identifiers unreliable from both C2 and C3; C2 output once had CJK brackets inside JSON.
- Split Wise charges: two drafts share one `external_id` (possible silent drop across batches).
- R20: C1 and C3 still have no `max_tokens` (C2 capped at 2048 from measurements).

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live** (`sha-9a0b1dd`, OTA 1.1.2). ⛔ Never
  deploy there; `/var/omni-updates` is its store. Snapshot `/var/omni-snapshots/` before dev mutations.
- 🔴 I drive the first pass, HE drives the final one. ⛔ Don't hand him a build with crashes in it.
- ⚠️ Any sync change runs `cargo test -p omni-me-server`; core `sync::` alone missed a regression.
- ⚠️ Dev phone: USB serial `5431324d59563398` works; wireless drops when it leaves Wi-Fi. CDP via
  `adb forward` + scratchpad `cdp.mjs`; the socket is named by PID, re-forward after relaunch.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Every cargo run in a capped systemd unit with
  `CARGO_PROFILE_DEV_DEBUG=0`. Public repo privacy guard rejects real institution names.
- ⛔ Branches: public `dev/role-split-model-seats`, private `dev/untested-overlay`.
