# NEXT

## ▶ NEXT ACTION — auto-approval (now unblocked), then audit the pre-09-26 batches
⛔ Both ready; the ORDER is my proposal, not his priority. Auto-approval needs a live agent:
`./target/release/omni-me-agent` with `OMNI_AGENT_DATA=<tmp>`, `OMNI_AGENT_SERVER_URL` at dev's
`:3001`, `FASTEMBED_CACHE_DIR=/dev/null/not-a-dir` (skips a 25-min sweep), 🔴 `RUST_MIN_STACK=67108864`
or it ABORTS. Then the 12 pre-today batches — pairwise matching, so none reconcile. ⛔ Ask first.

## ⛔ Waiting on the user — only these
- 🔴 **Consider rotating the DeepInfra key.** A file-change diff put its value into this session's
  transcript, which the hooks mirror into `logs/`. Credit-capped so bounded — but it is his call.
- ✅ **Public-repo identity scrub AUTHORISED**, ⛔ END of a session only. ⚠️ It now also owes the 3
  email addresses this session printed into `.log/`. `tasks.md` § Owed.

## ⛔ Inherit — settled, do not re-derive
- 🔴🔴 **THE AGENT ABORTS on its first answer-loop tick** — `tokio-rt-worker` stack overflow, SIGABRT,
  within a second of `agent running`, once with nothing pending. 2/2 at tokio's 2 MB default, clean at
  `RUST_MIN_STACK=64MB` — deep-but-BOUNDED recursion, single-variable controlled. ⛔ Nothing in the
  repo sets it, so **production aborts**; a bigger stack mitigates, is not the fix. `tasks.md`.
- ✅ **`note.revise` FULLY CONFIRMED** — card both halves, accept, refusal, deletion, **and the model
  choosing the action unaided**, anchor copied exactly, frontmatter untouched. ⛔ Do not re-test.
- 🔴 **First agent boot sweeps the vector index 24m12s WITHOUT finishing** (release, ~5 cores, 1.8 GB,
  zero progress logs) and cannot answer until it returns. ⛔ Blocks the on-box deploy until the index
  survives redeploys on a volume; ⛔ debug builds cannot even finish `init_all`.
- ✅ **Total takes precedence; ⛔ NEVER fetch a receipt's linked page.** The catch-all remainder leg
  already exists; only *surfacing* "4 of 12 listed" is left, and it is not a blocker.
- ✅ **Bulk/archive backfill = DOCUMENTS ONLY.** Ledger opens clean **2026-10-01** from September
  statements, which state their own balances. ⚠️ Email backlog in scope, archive-only.
- ✅ **Watermark: per account, on the document's transaction date.** 🔴 SC statements are the ONLY
  transaction source for those accounts. 💭 Closed-book framing is considered, not asked.
- ✅ **Archive refinement BEFORE the tab**: tags reuse the existing `Tag` type + query path; purge
  manual with preview; per-tag retention opt-in. ✅ Review = confirm-and-correct.
- 🔴 **Suite deadlock BLOCKS CI, nothing merges to `main`** — its own session; ⛔ `--test-threads=1`
  tried, reverted. 💭 Fold in the BuildKit fix (`tasks.md`). 🔴 **Pin `runs-on` by 2026-10-19.**
- 🔴 **Non-production banner CANNOT fire on Android.** ⚠️ **End every session with the roadmap.**

## ⛔ Standing
Public `dev/role-split-model-seats` (draft PR #1) / overlay `dev/untested-overlay`; both pushed.
⛔ Never run the suite on this box · live is never a deploy target · no vendor detail in the public
repo. ⚠️ `omni-me-private` NOT hook-committed. ✅ Dev runs `dev-a0c4d31-3d7d6bc`; ⛔ `sudo docker
pull` cannot reach GHCR, pull as `deploy`. ✅ `pgrep -x` for pids; `drive.sh` resolves the page id.
