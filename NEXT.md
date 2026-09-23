# NEXT

✅ **Dev is current and healthy** (`{"instance":"dev","status":"ok"}`, 3 IMAP sources, 4 LLM seats).
Rollback `/var/omni-snapshots/dev-pre-verify-20260923-0257.tgz`. Phone runs APK **1.1.5-dev**.
⛔ Full evidence lives in overlay `DEV_TESTING.md` §§ 7-8 and is NOT repeated here.

## ▶ NEXT ACTION — prove `receipts.rs` on the real Walmart forward

⏳ **Image build `35820346822`** carries forward-to-capture + the hardened prompt. When green:
repoint `~/omni-dev/.env`, then `POST /auto_import/tick?source=gmail_work`.
⚠️ **uid 3457 is already past the cursor**, so a tick will NOT refetch it — rewind the
`imap_cursors` row for `gmail_work` below 3457 first, or forward the receipt again.
**Expect:** `fetched:1 appended:1 ignored:0`, and a log line `receipt: producing proposed batch`
with `effective_confidence` / `needs_manual_review` / `dropped_postings` / `warnings`.
That log line has never once executed — it is the last unrun step on the unattended path.

## ✅ Decided
- **`EmailBody` uses a conditional prompt, not sender routing** (user, 2026-09-23). Why: most
  senders are single-amount, so a uniform `total` rule builds a check that cannot fail.
- **Forward-to-capture is real, gated three ways** (user, 2026-09-23): sender is one of the
  user's own mailbox addresses AND the body wraps a forward AND the original sender is a known
  vendor. It is also how the `.edu` mailbox gets in — its SSO blocks IMAP, so auto-forward it.
  ⚠️ Archiving never needed this: `imap.rs` archives before routing, so forwarded mail was
  already being kept. This only affects becoming a transaction draft.
- **Salvage over strictness** (2026-09-21). **`EXAMINE`, never `SELECT`.**
- 📊 **Do not re-measure:** `gmail_personal` ~5 msg/day. At JOBS=2: cold `cargo check` 7m36s ·
  core clippy (`auto-import`) 6m42s · app clippy 7m08s · overlay clippy 6m26s · dev image ~21min.

## ⚠️ Open
- 🔴 **The model will not leave `total` null on a single-amount email** — two prompt attempts
  failed. ⛔ Do NOT harden it a third time; the fix is structural (`DEV_TESTING.md` § 7).
- 🔴 **`assistant_projection::tests::arrival_order_does_not_change_a_threads_clocks` hangs ~1 in 5.**
  Named by diffing `... ok` lines against a green run. `timeout-minutes: 45` now caps both repos.
- ⛔ **`UIDVALIDITY` is handled nowhere.** Real fix carries `uid_validity` in the cursor; own session.

## ⛔ Inherit
- 🔴 **Locally `fmt`, `clippy`, `check` ONLY.** CI runs the suite — and its ONE invocation naming
  core+server+agent is load-bearing, it is what compiles core with `auto-import`.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⛔ Never deploy there. ⚠️ `omni-me-private`
  is NOT committed by the session hooks — by hand, current repo only.
- ⚠️ Strip ANSI before grepping docker logs. ⛔ Branches: `dev/role-split-model-seats` / `dev/untested-overlay`.
