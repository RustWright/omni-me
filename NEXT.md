# NEXT

✅ **Grouping BUILT + browser-verified** (public `228f7e9`), both 2026-09-23 questions answered.
Gate green: 1266 core / 84 app / 163 frontend, clippy ×4, fmt, zero console errors in mock.
⛔ **Not model-validated.** Dev image building at handoff (run `35955831370`). Rollback
`/var/omni-snapshots/dev-pre-kindgate-20260923-1412.tgz`.

## ▶ NEXT ACTION — land the dev image, then prove a merge on real mail
1. When run `35955831370` finishes: on the box, `cd ~/omni-dev`, point `.env` at the new tag,
   `docker compose pull && up -d`. ⚠️ `AutoImportProjection` is **version 2**, so first boot
   rebuilds it from the log against real rows — watch that boot, it is the migration test.
2. 🔴 **The merge proof needs NEW UIDs and the user's hands.** Archived mail sits past the IMAP
   cursor. Ask him to forward two mails about **one** order. Until then grouping is unit-proven.

## ⛔ Inherit — settled, do not re-derive
- **No vendor reference → no grouping**, per class, added only where a reliable one is found.
- **A revision opens a new review item**; committed books are never amended. ⚠️ His revisit
  trigger is his own friction, so review volume on this path is the thing to watch.
- ⚠️ **Dismissed is treated like committed** — a genuinely new mail about a dismissed order is
  reviewable. That call is MINE, not his. Flag it if it annoys him.
- ⛔ **"One Walmart order → six batches" is NOT a duplication factor.** He forwarded **two
  separate orders** while testing. The mechanism is real; the number is not.
- ⚠️ The **kind gate makes grouping safe** — `order_ref` is junk on non-order mail.

## 🔴 Open
- 🔴 **Projection writes swallow statement errors.** `.await?` returns Ok on a *rejected*
  statement: the row stays stale and reports success. Four have ZERO `.check()`; `tasks.md`
  says why not to sweep it blind.
- 🔴 **Suite deadlock** (1 in 6): 312 leaked `surrealkv-commit` threads from `test_db()`. Did
  **not** fire today. Plan in `tasks.md`; 217 call sites. ⛔ **`UIDVALIDITY`: nowhere.**
- ⚠️ **Playwright MCP is unusable here** — it wants a `chrome` channel whose install needs sudo.
  Worked around with bundled chromium driven from a scratchpad script.

## ⛔ Standing
- 🔴 **CI's ONE invocation** naming core+server+agent is what compiles core with `auto-import`; a
  narrower `-p omni-me-core` runs ZERO of those tests and looks like a pass.
- 🔴 **NOTHING since `sha-9a0b1dd` has run on live.** ⛔ Never deploy there. ⚠️ `omni-me-private`
  is NOT committed by the hooks — by hand, current repo only.
- ⚠️ Strip ANSI before grepping docker logs. **`EXAMINE`, never `SELECT`.** Branches:
  `dev/role-split-model-seats` / `dev/untested-overlay`.
