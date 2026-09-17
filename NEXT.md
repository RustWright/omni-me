# NEXT

▶ **NEXT ACTION: deploy `9d61cfbb` to DEV and prove enrichment on REAL documents.** The server
now keeps the `documents` projection and `/sync/push` folds into it. Snapshot the dev volume
first, deploy, confirm ticks stop failing, then archive real receipts and PDFs and watch them
get catalogued. Then continue the chain: phone receipt capture → archive → finance propose.

## ✅ Decided by the user 2026-09-17 (full reasoning: overlay `DEV_TESTING.md` § ANSWERED)
- **Server keeps `documents`, and push projects.** A table lives where the work that reads it
  runs. Design for today: agent and sync server co-located. Object storage for blobs and a
  separate agent machine are FUTURE goals. ⛔ Neither may complicate current design.
- **Ledger catch-up: decide when we reach it.** Files run to 2026-08-28 and are local at
  `.reference/paisa-ledger/`; tell him what's needed (newer statements) when that leg starts.
- 🔴 **Test with REAL documents.** DeepInfra was chosen for zero retention precisely so real
  material can be used. `~/omni-spike-images/`, `.reference/paisa-ledger/`; ask for more.
  ⛔ Real content never goes to a non-ZDR endpoint. IMAP on dev follows the enrichment fix.

## ▶ SCOPE — agreed 2026-09-16
**Goal: a fully functional app, ending with the FINANCES TAB re-enabled** — the finish line.
🔴 **I drive the first pass; HE drives the final pass.** Early obvious bugs would sour his
judgement before the feedback that matters. ⛔ Don't hand him a build until the crashes are out.
⛔ `tasks.md` § DEFINITION OF READY is the chain. 🔴 Documents are on the critical path.
✅ Dev data is disposable. Snapshot `/var/omni-snapshots/` before each dev mutation.

## ⛔ Inherit — do not re-derive
- 🔴 **NOTHING since the stamped release has run on live.** Live is `sha-9a0b1dd`; OTA store
  serves **1.1.2**. ⛔ Never deploy there; `/var/omni-updates` is its store.
- ✅ **`feature.finances = true` on DEV ONLY.** ⛔ Not the finish line; live still reads `false`.
- ✅ **Dev APKs carry `OMNI_WEBVIEW_DEBUG=1`** — drive the page over CDP. ⚠️ The socket is named
  by PID; re-forward after every app launch.
- ⚠️ **Strip ANSI before grepping docker logs** — colour codes sit inside tokens.
- ⚠️ **The sync badge is push-only** — reads "Synced" during a fresh install's catch-up.
  ⛔ Design call, not a patch from `pull_only`.
- 🔴 ⛔ **Every cargo run in a systemd unit with `CARGO_PROFILE_DEV_DEBUG=0`**; never reclaim
  `target/` by mtime.
- ⛔ **Branches:** public `dev/role-split-model-seats`, private `dev/untested-overlay`.
