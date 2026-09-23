# NEXT

✅ Dev healthy on `dev-68b3734-530edd6`, 3 IMAP sources, 4 LLM seats. Phone on APK **1.1.5-dev**.
Rollback `/var/omni-snapshots/dev-pre-verify-20260923-0257.tgz`. ⛔ Full evidence is in overlay
`DEV_TESTING.md` §§ 7-8 and is NOT repeated here.

## ▶ NEXT ACTION — decide what counts as a receipt

✅ **Unattended path PROVEN (2026-09-23):** a real forwarded Walmart receipt made a proposed batch
— `effective_confidence=0.31`, `needs_manual_review=true`, warning correct against the document.

🔴 **Decide first — ONE Walmart order produced SIX proposed batches** (table in `DEV_TESTING.md`
§ 7): ~4× the purchase plus two zero-amount entries, one a satisfaction survey. ⛔ **Not a
dedup-key problem** — `accepts()` claims every mail from a listed sender and the extractor makes a
transaction from each; ⚠️ `verify` does not catch this shape. Needs something separating "reports
a purchase" from "is about a purchase". ✅ Review queue now leads with the verdict panel (mock).

## ✅ Decided
- **`EmailBody` uses a conditional prompt, not sender routing** (user, 2026-09-23). Why: most
  senders are single-amount, so a uniform `total` rule builds a check that cannot fail.
- **Forward-to-capture is real, gated three ways** (user, 2026-09-23): self-addressed sender, a
  quoted header block, original sender on the vendor list. ⚠️ Gate on the header BLOCK, not a
  "Forwarded message" separator — real forwards come both ways. Also how `.edu` gets in.
- 📊 **Do not re-measure** (JOBS=2): cold `cargo check` 7m36s · core clippy 6m42s · app 7m08s ·
  overlay 6m26s · dev image ~21min · `gmail_personal` ~5 msg/day.

## ⚠️ Open
- 🔴 **Server config writes NEVER persist, dev AND LIVE** — `/config/omni-me` is root-owned, app is
  uid 10001; pause/resume and source CRUD vanish on restart while `/auto_import/status` (live
  registry, not disk) reports success. Detail + fix options in memory.
- 🔴 **`total` stays non-null on single-amount email**, 2 prompts in. ⛔ Fix is structural, not a 3rd.
- 🔴 **`assistant_projection::tests::arrival_order_…_clocks` hangs ~1 in 5** — named by diffing
  `... ok` lines vs a green run. `timeout-minutes: 45` now caps both repos.
- ⛔ **`UIDVALIDITY` handled nowhere.** Real fix carries `uid_validity` in the cursor; own session.

## ⛔ Inherit
- 🔴 **Locally `fmt`/`clippy`/`check` ONLY.** CI's ONE invocation naming core+server+agent is what
  compiles core with `auto-import` — splitting it silently drops those tests.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⛔ Never deploy there. ⚠️ `omni-me-private`
  is NOT committed by the session hooks — by hand, current repo only.
- ⚠️ Strip ANSI before grepping docker logs. **`EXAMINE`, never `SELECT`.** Salvage over
  strictness (`docs/src/extraction.md`). ⛔ Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
