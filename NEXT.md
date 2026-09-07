# NEXT

**Next action: verify v1.1.2 on both devices.** Release run `34127047401` was building when the
session paused (`gh run view 34127047401 -R RustWright/omni-me-private`). When it lands, OTA both:
- **Phone:** Finances tab stays gone, and a fresh problem report's `recent_errors` shows
  `recovered after N retr(y/ies)` rather than `state not managed`.
- **`surface`: let the updater AUTO-RESTART — do NOT close the app manually.** That is the only
  reproduction path for the splash hang; a manual relaunch always worked and proves nothing. If
  it hangs anyway, the new `tracing::debug!("get_config")` settles it: present ⇒ IPC arrived.
  **Then item 3 (AI/LLM/ML), planning-first, fresh session.**

## What 1.1.1 + 1.1.2 fixed — one root cause, two severities
The WebView fires startup invokes before the backend is ready. **`AppState` unmanaged** → Tauri
*rejects* → `Err` → `get_config` fell back to `Features::default()` (all on) → the phone drew a
Finances tab for a disabled feature; fixed 1.1.1 by a bounded retry in the shared helpers.
**IPC not ready** → Tauri *drops* it, the promise never settles → the boot future parked forever
and the splash never lifted (`surface`, after the updater's auto-restart, on the 1.1.0 *and*
1.1.1 updates); fixed 1.1.2 with `invoke_get_config_timed` + `continuity.rs`'s retry loop.

## Decisions in force — inherit these
- **Boot invokes need BOTH halves.** `invoke_timed` turns "never settles" into an `Err`; the
  retry then re-invokes. Timed-alone fails open with wrong data; retry-alone never fires, since a
  dropped invoke produces no error.
- **Features fail OPEN and stay that way** (`types.rs:243`); a drawn tab is also the error state.
- ⛔ **FINANCES DEFERRED INDEFINITELY**, now **off at the source** — bank credential headers
  renamed on the box, scheduler boots `sources=0`. Detail in the overlay's `tasks.md`.
- **A public stamp and the overlay `Cargo.lock` move together**: 3 public files + both public
  locks + the overlay lock, or the next `--locked` deploy dies.
- **Real institution names never enter the public repo**; canonical guard lives in
  `omni-me-private/privacy-guard/`, **not** installed in a fresh clone.
- **CLOSED, do not re-survey:** generalization (Phases 0/A/B/C) — `invariants.md` is the
  contract, never back-fit it — and the v1.1.0 verification pass (steps 1–6, legs 7a/7c).

## Open threads
⚠️ Server stays **1.1.0** (client-only fixes) but the overlay lock expects 1.1.2. · Leg **7b** (a
real panic reaching the ring buffer) still held. · `GET /feedback` **broken** — SurrealDB
`ORDER BY` parse error served as HTTP 200 `text/plain` [XS]. · Every update leaves a husk process
on a deleted `/tmp/tauri_current_app…` binary — harmless (no db fds), unexplained. · New-projection
history gap: **A vs B undecided**. · `chunk_for_push` 413 **unverified**. · Curiosities→concepts
+ memory prune owed (Cycle 4 close-out).
