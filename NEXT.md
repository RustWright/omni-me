# NEXT

## ▶ NEXT ACTION — 🔴 decide how `salvage_total` reads a printed total, then fix it
The live pair PASSED, but both messages lost their stated total: the document prints `$116.47`, the
model copies it as printed, `parses_as_decimal` rejects the `$`, and `salvage_total` discards it.
⛔ **The guard turns a hard failure into a silent loss of the authoritative number** — no
cross-check, `effective_confidence` 0.36/0.225, all manual review. ⚠️ Not a blind strip: the corpus
is CAD/USD/EUR/NGN and stripping `,` from `1.234,56` is wrong by 1000×. Recommendation in overlay
`DEV_TESTING.md`. ⚠️ **Also persist the printed form** (`total_as_printed`, like `date_as_printed`):
`raw_response` is NOT written into the event, so diagnosing this meant grepping archived text.

## ⛔ Waiting on the user — only these
- 🔴 **Inbox management after extraction** and **reviewing the email archive** — product calls.
  ✅ Ledger catch-up and the finances tab move to the END of testing. ⛔ Not blockers.
- ✅ **Public-repo identity scrub AUTHORISED** (2026-09-26), ⛔ only at the END of a session.
  Sites + traps: `tasks.md` § Owed at cycle close.

## ⛔ Inherit — settled, do not re-derive
- 🔴 **`gmail_work` and `yahoo` are STILL PAUSED.** Only `gmail_personal` was resumed.
  ⚠️ `/auto_import/status` reports `health: healthy` for a paused source that imports nothing.
- ✅ **VERIFIED ON REAL MAIL 2026-09-26, dev `dev-1c827e2-019626e`.** ⛔ Do not re-test: `order_ref`
  and the new `order_ref_printed` agree (`600000119191856`), all three messages share one
  `order_group`, `sender_auth=authenticated` with no false warning, `failed=0` where the pre-fix
  path dropped everything, and the delivery mail extracted **4 postings, was 0**.
- ✅ **`ShippingNotice` is charge-bearing BY DESIGN** — vendors restate the total in it. ⛔ Do not
  "fix" the delivered mail being labelled `shipping_notice`.
- ⚠️ **Extraction robustness is now load-bearing for the WHOLE mailbox**: the gate inversion claims
  every message, so any field the model phrases oddly can lose a document. All four in
  `ExtractionResult` are guarded now; a fifth added later would not be.
- 🔴 **A failed message ADVANCES the cursor by design** — a transient parse failure loses a real
  receipt, recoverable only by hand-editing `imap_cursors`. ⚠️ So pause rather than risk it:
  `EXAMINE` + UID cursor means nothing is lost, `resume` pulls at once, and a pause survives a
  restart. ✅ Used three times today, and it beats any deploy deadline.
- ⚠️ **No deploy has exercised the paged replay** — no version bump since `a0ad51a`.
- ✅ **Dev builds dispatchable**: ids `359907704` (image) / `359972206` (APK); `303640807` is LIVE.
- 🔴 **`cargo clippy -p omni-me-core` compiles NONE of `auto_import`** — pass `--features auto-import`.

## 🔴 Open — APK `1.1.11-dev` is BUILT and on the box, **not installed**. Install it, then queue item 6 (five on-device confirmations) and the cold-backfill wedge. Phone answers adb, stay-awake on.

## ⛔ Standing
`dev/role-split-model-seats` (public; draft PR #1 open, so a push runs full CI — ⛔ never run the
suite on this box) / `dev/untested-overlay`. Both pushed. ⛔ Live is never a deploy target here.
⚠️ `omni-me-private` is NOT hook-committed. ⛔ No sender or vendor detail in the public repo.
