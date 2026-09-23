# NEXT

✅ Dev healthy on `dev-68b3734-530edd6`, 3 IMAP sources, 4 LLM seats. Phone runs APK **1.1.5-dev**.
Rollback `/var/omni-snapshots/dev-pre-verify-20260923-0257.tgz`.
⛔ Full evidence lives in overlay `DEV_TESTING.md` §§ 7-8 and is NOT repeated here.

## ▶ NEXT ACTION — decide what counts as a receipt

✅ **Unattended path PROVEN (2026-09-23):** a real forwarded Walmart receipt made a proposed batch
— `effective_confidence=0.31`, `needs_manual_review=true`, warning correct against the document.

🔴 **Decide first — ONE Walmart order produced SIX proposed batches** (read live off the phone;
table in `DEV_TESTING.md` § 7). Committing them books ~4× the purchase plus two zero-amount
entries — one of them a satisfaction survey. ⛔ **Not a dedup-key problem**: `accepts()` claims
every mail from a listed sender and the extractor makes a transaction from each. ⚠️ `verify` does
not catch this shape. Needs something separating "reports a purchase" from "is about a purchase".
✅ The review queue now leads with the verdict panel, not a collapsed JSON blob (mock-verified).

## ✅ Decided
- **`EmailBody` uses a conditional prompt, not sender routing** (user, 2026-09-23). Why: most
  senders are single-amount, so a uniform `total` rule builds a check that cannot fail.
- **Forward-to-capture is real, gated three ways** (user, 2026-09-23): a self-addressed sender, a
  quoted header block, and an original sender on the vendor list. ⚠️ Gate on the header BLOCK, not
  a "Forwarded message" separator — real forwards come both ways. Also how `.edu` gets in.
- **Salvage over strictness** (2026-09-21). **`EXAMINE`, never `SELECT`.**
- 📊 **Do not re-measure** (JOBS=2): cold `cargo check` 7m36s · core clippy 6m42s · app 7m08s ·
  overlay 6m26s · dev image ~21min · `gmail_personal` ~5 msg/day.

## ⚠️ Open
- 🔴 **The model will not leave `total` null on a single-amount email**, two prompt attempts in.
  ⛔ Do NOT harden it a third time; the fix is structural (`DEV_TESTING.md` § 7).
- 🔴 **`assistant_projection::tests::arrival_order_does_not_change_a_threads_clocks` hangs ~1 in 5** —
  named by diffing `... ok` lines vs a green run. `timeout-minutes: 45` now caps both repos.
- ⛔ **`UIDVALIDITY` is handled nowhere.** Real fix carries `uid_validity` in the cursor; own session.

## ⛔ Inherit
- 🔴 **Locally `fmt`, `clippy`, `check` ONLY.** CI's ONE invocation naming core+server+agent is
  load-bearing: it is what compiles core with `auto-import`.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⛔ Never deploy there. ⚠️ `omni-me-private`
  is NOT committed by the session hooks — by hand, current repo only.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
