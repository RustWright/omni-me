# NEXT

## ▶ NEXT ACTION — run the local agent, then audit the pre-09-26 batches
⏳ Gated on the key, below. Then `cargo run -p omni-me-agent` with `OMNI_AGENT_DATA=<tmp>` and
`OMNI_AGENT_SERVER_URL` at the dev server's `:3001`; ask the phone to revise the probe note, confirm
the model picks `note.revise` **itself**, plus auto-approval. 🔴 Then audit the 12 pre-today batches:
one draft **per line item**, matching is pairwise, so none can reconcile. ⛔ Ask before re-importing.

## ⛔ Waiting on the user — only this
- 🔴 **Paste the DeepInfra key into `~/.config/omni-me/credentials.toml`** — one `api_key` line, the
  rest is filled. ✅ It is credit-capped and throwaway, so no new key is needed. ⛔ I cannot install
  it myself: the auto-mode classifier blocks it, rightly.

## ⛔ Inherit — settled, do not re-derive
- ✅ **Dev runs `dev-a0c4d31-3d7d6bc`.** ⛔ `sudo docker pull` cannot reach GHCR — pull as `deploy`,
  no sudo, *before* rewriting `OMNI_IMAGE`; read a tag from the run log, never construct it.
- ✅ **`note.revise` CONFIRMED on the phone, both paths** — one-line diff, frontmatter untouched; the
  refusal wrote nothing and left the card decidable. ⛔ Do not re-test. ⚠️ Seeded via `/sync/push`,
  so model action-choice is the open part — that is what the agent is for.
- ✅ **Total takes precedence; ⛔ NEVER fetch a receipt's linked page.** No URL allowlist (it would
  rebuild the gate the 2026-09-25 sender ruling removed), no per-receipt manual fetch. The catch-all
  remainder leg already exists; only *surfacing* "4 of 12 listed" is left, and it is not a blocker.
- ✅ **Agent locally now, on the deploy host eventually** — that host is merely the only always-on
  one, not an architectural need, so both placements are worth testing. ⛔ Not before the gates.
- ✅ **Bulk/archive backfill = DOCUMENTS ONLY.** Ledger opens clean **2026-10-01** from September
  statements, which state their own balances. ⚠️ Email backlog in scope, archive-only.
- ✅ **Watermark: per account, on the document's transaction date.** 🔴 SC statements are the ONLY
  transaction source for those accounts. 💭 Closed-book framing is considered, not asked.
- ✅ **Archive refinement BEFORE the tab**: tags reuse the existing `Tag` type + query path; purge
  manual with preview; per-tag retention opt-in. ✅ Review = confirm-and-correct. ⛔ Inbox: track only.
- 🔴 **Suite deadlock BLOCKS CI, nothing merges to `main`** — its own session; ⛔ `--test-threads=1`
  tried, reverted. 💭 Fold in the BuildKit fix (`tasks.md`). 🔴 **Pin `runs-on` by 2026-10-19.**
- 🔴 **The non-production banner CANNOT fire on Android**, so the dev APK looks exactly like live.
  ⚠️ **End every session with the roadmap** — `CLAUDE.md`.

## ⛔ Standing
Public `dev/role-split-model-seats` (draft PR #1) / overlay `dev/untested-overlay`; both pushed.
⛔ Never run the suite on this box · live is never a deploy target · no vendor detail in the public
repo. ⚠️ `omni-me-private` is NOT hook-committed. ✅ Scrub AUTHORISED, ⛔ END of session only — ⚠️ it
now also owes 3 email addresses this session printed into `.log/`. ✅ CDP `cdp.js`+`type.js`.
