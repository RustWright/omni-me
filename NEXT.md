# NEXT

## ▶ NEXT ACTION — 🔴 [USER] the box's GHCR token expired; nothing can deploy until it is replaced
Pulls worked at ~14:09Z and were `denied` by ~16:35Z — including the **known-good tag** that had
pulled two hours earlier, so it is the credential, not the tag. 🔴 **`deploy/remote-deploy.sh`
pulls on the same stored login, so LIVE deploys are broken too.** ✅ Builds are unaffected (CI mints
its own). ⛔ The durable fix is a deploy job that authenticates the box from the Actions secret, not
a new hand-placed token that re-expires in 90 days — `tasks.md` § Release engineering.
✅ **Dev is healthy on `dev-1c827e2-019626e`** and `.env` was reverted to match the running
container, so a restart is safe. ⏳ **`dev-24808b0-a8bfd24` is built and waiting** — it carries the
total fix and is the only thing between here and the total being read on real mail.

## ⛔ Waiting on the user — only these
- 🔴 The GHCR token above. ⛔ Everything else below is unblocked.
- ✅ **Public-repo identity scrub AUTHORISED**, ⛔ only at the END of a session. `tasks.md` § Owed.

## ⛔ Inherit — settled 2026-09-26, do not re-derive
- ✅ **Bulk/archive backfill produces DOCUMENTS ONLY, never transactions.** Ledger starts clean
  **2026-10-01** from September statements, which state their own balances. The email backlog is in
  scope and is archive-only too.
- ✅ **Watermark: per account, on the document's transaction date.** A document may post
  transactions only after its own account's last statement date. ⚠️ Global would silently suppress
  real transactions for earlier-closing accounts. 🔴 **SC statements are the ONLY transaction source
  for those accounts** — "statements are verification" must never become a blanket assumption.
- 💭 **Closed-book framing is under consideration, not requested** (user). ⚠️ Hazard for later: a
  double-entry transaction straddling two accounts with different cutoffs cannot be half-posted.
- ✅ **Archive refinement lands BEFORE the tab** (option 2). Tags reuse the existing `Tag` type and
  query path; the reader writes them path-independently; purge is manual with a preview; per-tag
  retention opt-in once a tag has earned it. ⛔ Design session first.
- ✅ **Archive review = confirm-and-correct**, flipping `DocumentField.verified` to true.
  ✅ **Inbox management: track only, never write to the mailbox** — 🔴 but Gmail write-back is the
  user's DESTINATION once trust is built, not a rejected option.
- 🔴 **Suite deadlock now BLOCKS CI, so nothing merges to `main`.** Fix is its own session; the
  test passes alone in 3.36s. ⛔ `--test-threads=1` was tried and reverted — the leak is `+1 per
  connect()` and parallelism-independent.
- ✅ **Stage 6 triaged: 9 real gates, 10 ship-after** — `tasks.md` § TRIAGE. Three gates were
  promoted by today's decisions.
- 🔴 **`gmail_work` and `yahoo` are STILL PAUSED**; a paused source reports `health: healthy`.

## 🔴 Open — APK `1.1.11-dev` built, on the box, **not installed**. Then queue item 6, the wedge, and item 8's `pull_only` Vec. Phone answers adb at `100.109.41.53:5555`, stay-awake on.

## ⛔ Standing
Public `dev/role-split-model-seats` (draft PR #1; ⛔ never run the suite on this box) / overlay
`dev/untested-overlay`. Both pushed. ⛔ Live is never a deploy target here. ⚠️ `omni-me-private` is
NOT hook-committed. ⛔ No sender or vendor detail in the public repo.
