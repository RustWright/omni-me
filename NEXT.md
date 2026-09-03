# NEXT

**Next action: the batched on-device pass — everything else here is green.**
802 tests pass, 0 failures. Stages 1, 1b, 2, 4, 5 done; 3 all but swipe. 6/7 need sessions.

## Decisions in force — inherit these, don't re-derive
- **The plan is `~/.claude/plans/i-m-not-sure-what-s-elegant-firefly.md`** — seven tiers
  from the 2026-09-03 feedback + the route to the AI milestone. Read it before replanning.
- **Confidence before AI.** Stages 1, 1b and 2 gate the LLM push; Stage 6 (statements)
  should land before it too — a `Statement` entity is churn LLM tools mustn't chase.
- **Dev never writes to the box (user)** — live use means a test write leaves
  indistinguishable fake rows in the real ledger. `OMNI_DATA_DIR` overrides the data root;
  under it the persisted `server_url` is IGNORED — only `OMNI_SERVER_URL` moves it (9 tests).
- **Swipe-between-finances-tabs is HELD (user)** — the sticky sub-nav ships first and the
  device trial decides. Rationale: side-nav-swipe vs section-swipe is a real conflict, so
  if sticky suffices the ambiguity never needs adjudicating. Don't add it speculatively.
- **Font size is EDITOR-ONLY (user).** App-wide is blocked on converting 106 px-based text
  utilities (81 are `text-[10px]`) to rem, else the smallest labels freeze while body grows.
- **Statement labels remember per account** (the path can't identify an institution).
- **"Complete" = the three reflection keys** — the scheduler was wrong, not the predicate.
- **Calendar:** fill = volume, ✓ = complete, ring = closed. Thresholds NOT settled —
  250-word steps is provisional; re-derive from live data after `JournalDayStat` widens.

## Corrections to earlier handoffs — verified live
- **This host IS `surface` and holds live data** (42M `local.db`). 7GB RAM ⇒
  `CARGO_BUILD_JOBS=1`. The 32GB box is gone indefinitely — but **releases build in CI**
  (`app-release.yml`), so only local release builds are lost.
- **Watch `df -h`** — `target/` grew 1.1G → 30G in one session, filled the disk, and caused
  every "background task killed" plus a linker failure. ~41G reclaimed.
- `editor.js` edits need `npm run build:editor`; the app loads `editor.bundle.js`.

## Batched on-device checklist (device unplugged — do ALL in one connection)
Android API level (decides the caret fix; `ime()` needs API 30+, target may be 29) ·
scroll/keyboard · selection contrast · strikethrough · auto-close catch-up on open ·
**sticky sub-nav: does it remove the need for swipe?** · drawer: caret gone (expected) AND
keyboard dismissed (UNKNOWN — if the IME stays up, focus a non-editable element instead).

## Open threads
- Undecided: branch *naming* convention (dev-writes-to-box is settled: never).
- Filed: statement imports capture no institution tag — fold into Stage 6.
