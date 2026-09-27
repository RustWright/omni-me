# NEXT

## ▶ NEXT ACTION — ⛔ HIS CALL. Purge is built; it has never run over HTTP.
💭 **My proposal, not his ruling: exercise purge against the dev server**, because it is the only
piece of it nothing has tested — the engine is unit-tested against a real database and the routes
only compile. ⛔ **Gate 1 (server backup) precedes any purge on real data**; verify by size, never
by exit code. The alternatives are per-tag retention (the queue it feeds now exists) or the
confirm-and-correct affordance. Design: `tasks.md` § ARCHIVE REFINEMENT.

## ⛔ Waiting on the user — only this
- ✅ **Public-repo identity scrub AUTHORISED**, ⛔ END of a session only. `tasks.md` § Owed.
  ⛔ **PUBLIC repo ONLY** — overlay, `.log/` and transcript are out of scope by his 2026-09-27 ruling.

## ✅ Done 2026-09-27 — ⛔ do not redo or re-verify
- **Archive tags ship.** `tags` is a `DocumentField` key holding the joined set, hoisted into an
  indexed array column; filter on `list_documents` and the catalogue's `list` verb; chip editor and
  filter menu on the page. ⛔ Projection **version 1 → 2**.
- **Purge-with-preview ships.** `document_purged` tombstone (⛔ UPSERT — a purge can arrive before
  the archive event it purges), text cleared, `hidden_when` takes the row out of
  search/list/read/count **and** the vector index. Server preview+confirm routes behind a
  single-use ticket the confirm may only *shrink*. Accounting asserted.
  ⏳ **Never run over HTTP.** ⚠️ Purged rows are excluded from every archive read, so the
  "reads as purged" state is only reachable on a stale detail view.
- **Gate 3 closed** — role C has 429 retry/backoff/spacing. ⚠️ `min_interval` is `None` in
  production. 🔴 **Gate 4 was already done before the triage claimed it open.** Gates: 9 → **7 open**.
- 🔴 **Found while building purge: removed routine items were reaching the assistant.**
  `fetch_children` applied no hidden rule at all; the rollup over the same items filtered them.
  Fixed with the same change. ⛔ Do not re-investigate.

## ⛔ Inherit — settled, do not re-derive
- 🔴 **Purge confirms at GROUP granularity, once** (his ruling): every document listed and
  individually sparable, one action. ⛔ Therefore **retention NEVER deletes** — it fills that queue.
- ⛔ **The blob refcount reads TWO STORES**: documents from the projection, transaction attachments
  from the **event log**, because the server maintains `DocumentsProjection` alone and
  `transactions` does not exist there. ⛔ Never "simplify" it to one query.
- 🔴🔴 **THE AGENT ABORTS on its first answer-loop tick** — `tokio-rt-worker` stack overflow, SIGABRT.
  2/2 at tokio's 2 MB default, clean at `RUST_MIN_STACK=64MB`. ⛔ Nothing in the repo sets it, so
  **production aborts**; a bigger stack mitigates, is not the fix. Blocks the assistant leg, one of
  **three** that must all ship.
- 🔴 **First agent boot sweeps the vector index 24m12s WITHOUT finishing**, so the index must
  survive redeploys on a volume. ⛔ Release builds only.
- ✅ **`note.revise` FULLY CONFIRMED**, model's own action choice included. ⛔ Do not re-test.
  ✅ **Backfill = DOCUMENTS ONLY**; ledger opens clean 2026-10-01. ✅ Watermark per account, on the
  document's transaction date. ✅ Total wins; ⛔ NEVER fetch a receipt's linked page.
- 🔴 **Suite deadlock BLOCKS CI, nothing merges to `main`** — its own session. 🔴 Pin `runs-on`
  by **2026-10-19**. ⚠️ Roadmap every session (`CLAUDE.md`).

## ⛔ Standing
Public `dev/role-split-model-seats` (draft PR #1) / overlay `dev/untested-overlay`; both pushed.
⛔ Never run the full suite on this box · live is never a deploy target · no vendor detail in public.
⚠️ `omni-me-private` NOT hook-committed. ✅ Dev runs `dev-a0c4d31-3d7d6bc`; ⛔ pull GHCR as `deploy`.
