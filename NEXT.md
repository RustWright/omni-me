# NEXT

## ▶ NEXT ACTION — ARCHIVE REFINEMENT DESIGN, in plan mode (his decision, 2026-09-27)
✅ He chose design-next over gates-next and will open the next session in **plan mode**. ⛔ DESIGN only,
not build: tags + reader + query path + purge-with-preview + per-tag retention is more than one
session, and 3 of the 9 gates depend on what this design says. 🔴 **The release is THREE legs —
assistant, archive, finance re-enabling — all must ship** (`tasks.md` § TRIAGE), so the agent's abort is a release blocker, not a queued item, and the gate count needs re-deriving.
💭 Gates 3+4 (role C 429 retry, `max_tokens`) need no design, are one file, and each loses a document today — my proposal is to grab them in the design session's gaps; ⛔ his call, never assumed.

## ⛔ Waiting on the user — only this
- ✅ **Public-repo identity scrub AUTHORISED**, ⛔ END of a session only. `tasks.md` § Owed.
  ⛔ **PUBLIC repo ONLY** — he ruled 2026-09-27 that flagging exposure in the overlay, `.log/` or the transcript is a distraction, and the dev LLM key a non-issue. The line is *published vs not*.

## ⛔ Inherit — settled, do not re-derive
- 🔴🔴 **THE AGENT ABORTS on its first answer-loop tick** — `tokio-rt-worker` stack overflow, SIGABRT,
  within a second of `agent running`, once with nothing pending. 2/2 at tokio's 2 MB default, clean at
  `RUST_MIN_STACK=64MB` — deep-but-BOUNDED recursion, single-variable controlled. ⛔ Nothing in the
  repo sets it, so **production aborts**; a bigger stack mitigates, is not the fix. `tasks.md`.
- ✅ **`note.revise` FULLY CONFIRMED** — card both halves, accept, refusal, deletion, **and the model
  choosing the action unaided**, anchor copied exactly, frontmatter untouched. ⛔ Do not re-test.
- 🔴 **First agent boot sweeps the vector index 24m12s WITHOUT finishing** (release, ~5 cores, 1.8 GB,
  no progress logs) and cannot answer until it returns — so the index must survive redeploys on a
  volume. ⛔ Release builds only: debug cannot finish `init_all` at all. `tasks.md`.
- ✅ **Total takes precedence; ⛔ NEVER fetch a receipt's linked page.** The catch-all remainder leg
  already exists; only *surfacing* "4 of 12 listed" is left, and it is not a blocker.
- ✅ **Bulk/archive backfill = DOCUMENTS ONLY.** Ledger opens clean **2026-10-01** from September
  statements, which state their own balances. ⚠️ Email backlog in scope, archive-only.
- ✅ **Watermark: per account, on the document's transaction date.** 🔴 SC statements are the ONLY
  transaction source for those accounts. 💭 Closed-book framing is considered, not asked.
- ✅ **Archive refinement BEFORE the tab**: tags reuse the existing `Tag` type + query path; purge
  manual with preview; per-tag retention opt-in. ✅ Review = confirm-and-correct.
- 🔴 **Suite deadlock BLOCKS CI, nothing merges to `main`** — its own session; ⛔ `--test-threads=1`
  tried, reverted. 💭 Fold in the BuildKit fix. 🔴 Pin `runs-on` by **2026-10-19**. ⚠️ Roadmap every
  session (`CLAUDE.md`); the Android banner gap is tracked in `tasks.md` § Stage 0.

## ⛔ Standing
Public `dev/role-split-model-seats` (draft PR #1) / overlay `dev/untested-overlay`; both pushed.
⛔ Never run the suite on this box · live is never a deploy target · no vendor detail in the public repo.
⚠️ `omni-me-private` NOT hook-committed. ✅ Dev runs `dev-a0c4d31-3d7d6bc`; ⛔ `sudo docker pull` cannot
reach GHCR — pull as `deploy`. ✅ `pgrep -x` for pids; `drive.sh` resolves the CDP page id itself.
