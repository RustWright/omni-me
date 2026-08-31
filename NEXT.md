# NEXT

**Next action: OTA-update both personal devices 1.0.3 → 1.0.4 — itself the update-path test on
real hardware.** Both run 1.0.3; **1.0.4 is published for both**. Phone: Settings → App Updates →
*Check for updates* (the two Android prompts reappear — the app is the installing source), then
**type a sentence and see whether the keyboard commits predictive suggestions** — the fix 1.0.4
exists for, **unconfirmed on-device**. Then brokerage **Reconnect** in-app, then box auth.

## Decisions in force — inherit these, don't re-derive
- **Go-live is DONE (2026-08-31):** box wiped total, fresh import from the canonical ledger +
  vault, seeded, both personal devices installed and syncing. This laptop is the **dev/import
  machine only**; `surface` is the go-live Linux desktop.
- **Desktop AppImages MUST build on the oldest supported target.** `build-desktop` is pinned to
  `ubuntu-22.04`: an AppImage never bundles glibc, so `ubuntu-latest` (2.39) shipped a binary that
  would not start on `surface` (2.35) — invisible from this 24.04 laptop. `dx` has **no musl
  build** so it compiles from source there, and `rust-cache` keys on `Linux-x64` without the
  Ubuntu version, so that job's cache is namespaced or it restores a 24.04 `dx`. 1.0.3 was
  verified to need only `GLIBC_2.35`; 1.0.4 is inferred from the same job, not checked.
- **Pause auto-import sources before any wipe** — a 30-min source fired in the 14-min seed gap.

## Do NOT re-survey
- **The wipe→import→seed chain is verified.** `/data` 21M → 64K, 4 snapshots deleted, local reset
  213M → 776K, import reproduced the 2026-08-30 corpus exactly, **0 diffs** vs `ledger bal`
  (twice), net worth **83 290.52 CAD**, pushed **12 277**.
- **The Wise "duplicate transactions" investigation is CLOSED — not a bug**, just the poller
  ticking before the seed landed; two dead ends are written up in `tasks.md` so they are not
  re-walked. In the review inbox commit only the 2026-08-31 transfer (per-row selection exists);
  the other four are already in the ledger and won't be re-proposed.

## Open threads
- Editor fix: DOM attributes verified in a browser, **on-device behaviour unconfirmed** (CM6's
  Android IME composition handling could still block commit) · no in-app "reset local data": a
  device that ran an older build inherits a stale `server_url` outranking the compile-time
  default, and only `rm -rf` fixes it (bit `surface`).
- Dedup keys on the reference number, which multi-leg transactions share → a second leg could be
  silently dropped, and the failure mode is a *missing* transaction · `--force` recompiles `dx`
  every desktop release (~10 min), droppable now the guard exists · owed: the `xvfb-run` smoke
  step · desktop white flash on a real GPU.
- **On a NEW device run `omni-me-private/privacy-guard/install.sh` before committing here** — a
  fresh clone of this PUBLIC repo has no guard, and the session hooks auto-commit and push.
