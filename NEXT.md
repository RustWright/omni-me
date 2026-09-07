# NEXT

**Next action: finish the v1.1.1 release.** CI was running on `3aa4a80` when the session paused.
When green: tag `v1.1.1`, then `gh workflow run app-release.yml -R RustWright/omni-me-private
-f version=1.1.1 -f public_ref=main -f targets=both -f box=hetzner`, then OTA both devices and
**verify on the phone** — the Finances tab must be gone, and a new report's `recent_errors`
should read `recovered after N retr(y/ies)` instead of `state not managed`. Then item 3
(AI/LLM/ML), planning-first, fresh session.

**What v1.1.1 is:** a cold-start race — the WebView fires startup invokes before `setup()`
reaches `handle.manage(AppState{…})`, so seven fail. `get_config` failing falls back to
`Features::default()` = everything on, set **once**, so the phone drew a Finances tab for a
feature that was off. Fix: deadline-based retry in the three shared invoke helpers in
`bridge.rs`. Found via the app's own feedback capture — its first real use. Detail in `tasks.md`.

## Decisions in force — inherit these
- ⛔ **FINANCES DEFERRED INDEFINITELY**, and now **off at the source**: both bank credential
  section headers renamed on the box so the scheduler boots `sources=0`. Reversible by renaming
  two lines; detail + backup path in the **overlay's** `tasks.md`.
- **Retry deadline is 10s deliberately** — too-short reproduces the silent bug being fixed,
  too-long is only a visible splash. Err long.
- **Retry safety differs by signature.** `state not managed` = refused before dispatch, so it
  provably never ran → safe anywhere. `__ipc_timeout__` = we stopped waiting and it may have
  run → retried **only** in `invoke_timed` (boot-only, read-only callers).
- **Features fail OPEN and stay that way** (`types.rs:243`); a drawn tab is also the error state.
- **A public stamp and the overlay `Cargo.lock` move together** or the next `--locked` deploy
  dies: 1.1.1 = 3 public files + both public locks + the overlay lock.
- **Real institution names never enter the public repo** — the guard caught one this session.

## Do NOT re-survey
Generalization is CLOSED (Phases 0/A/B/C) and `docs/src/invariants.md` is the contract — never
back-fit it. The v1.1.0 verification pass is CLOSED: steps 1–6 and legs 7a/7c confirmed on hardware.

## Open threads
⚠️ Server stays **1.1.0** (client-only fix) but the overlay lock now expects 1.1.1. · Leg **7b**
(a real panic reaching the ring buffer) is the one held item. · `GET /feedback` is **broken** —
SurrealDB `ORDER BY` parse error served as HTTP 200 `text/plain` [XS]. · New-projection history
gap: **A vs B not decided**, measure replay cost first. · `chunk_for_push` oversized-event 413
is **unverified**. · Real institution names sit in public `tasks.md` (~line 221), pre-existing. ·
Curiosities→concepts + memory prune owed (Cycle 4 close-out).
