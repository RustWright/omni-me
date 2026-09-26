# NEXT

## ▶ NEXT ACTION — audit the pre-2026-09-26 batches for the un-reconcilable shape
🔴 Batches committed before today made one draft **per line item**, and `find_match_candidates` is
pairwise, so not one of them can ever reconcile against a bank statement. 12 batches still sit in
the dev review queue (2026-09-19 … 09-25, several with 12 transactions). Per batch: re-import under
the fixed mapper, or leave it for the Oct-1 clean ledger to supersede. ⛔ Ask before re-importing —
whether pre-October data is worth the churn is the user's call, not a derivable one.

## ⛔ Waiting on the user — only these
- 🔴 **How to get the 8 line items a vendor's mail does NOT contain** (they sit behind a link).
  ⛔ Ask before building — following it means fetching remote content from an email.
- 🔴 **May I copy the dev LLM credential to run an agent locally?** It is
  `/etc/omni-me/credentials-dev.toml` on the box, and `OMNI_AGENT_SERVER_URL` overrides the
  localhost pin, so a local agent can point at dev. Unblocks auto-approval **and** the untested
  half of `note.revise`. ⛔ Never copy a credential unasked.
- ✅ **Public-repo identity scrub AUTHORISED**, ⛔ only at the END of a session. `tasks.md` § Owed.

## ⛔ Inherit — settled, do not re-derive
- ✅ **Dev runs `dev-a0c4d31-3d7d6bc`** (one-draft-per-purchase mapper); `/health` ok, 3 pollers
  healthy. ⛔ `sudo docker pull` cannot reach GHCR — pull as `deploy`, no sudo, and pull *before*
  rewriting `OMNI_IMAGE`. ⛔ Read an image tag from the run log; never construct it.
- ✅ **`note.revise` CONFIRMED on the phone, both paths.** Accepted: a one-line event diff,
  frontmatter byte-identical. Refused: readable sentence, **no event written**, card still
  decidable. ⛔ Do not re-test. ⚠️ Seeded via `/sync/push` (no feature guard) because there is **no
  agent anywhere**, so the model *choosing* the action is still unexercised.
- ✅ **Total takes precedence over line items** — it balances the bank charge; one draft per purchase.
- ✅ **Bulk/archive backfill = DOCUMENTS ONLY.** Ledger opens clean **2026-10-01** from September
  statements, which state their own balances. ⚠️ Email backlog in scope, archive-only.
- ✅ **Watermark: per account, on the document's transaction date.** 🔴 SC statements are the ONLY
  transaction source for those accounts. 💭 Closed-book framing is under consideration, not asked.
- ✅ **Archive refinement BEFORE the tab**: tags reuse the existing `Tag` type and query path;
  reader writes them path-independently; purge manual with preview; per-tag retention opt-in.
  ✅ Review = confirm-and-correct. ✅ Inbox: track only, ⛔ never write to the mailbox.
- 🔴 **Suite deadlock BLOCKS CI, nothing merges to `main`** — its own session; ⛔ `--test-threads=1`
  was tried and reverted. 💭 Fold in the BuildKit fix: 19.2 of every 20 min of image build is a
  cold `cargo build`, because `cache-to: type=gha` does not export cache mounts (`tasks.md`).
- 🔴 **The non-production banner CANNOT fire on Android**, so the dev APK looks exactly like live.
  ⚠️ **End every session with the roadmap** — `CLAUDE.md`.

## 🔴 Open — pin `runs-on` before **2026-10-19** or CI breaks again, on the live release path too.

## ⛔ Standing
Public `dev/role-split-model-seats` (draft PR #1) / overlay `dev/untested-overlay`; both pushed.
⛔ Never run the suite on this box · live is never a deploy target · no vendor detail in the public
repo. ⚠️ `omni-me-private` is NOT hook-committed. ✅ CDP live on `tcp:9222`; drivers
`<scratchpad>/cdp.js` and `type.js` (CodeMirror ignores DOM writes — it needs real input events);
⛔ expression from a FILE, never argv, and ⚠️ the WebView pid changes on every relaunch.
