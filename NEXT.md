# NEXT

**Next action: item 3 — AI / LLM / ML.** Planning-first by its own framing; give it a fresh
session. *How to integrate, host and securely use AI in this app for the best value at least
cost* — explicitly a **re-examination, not an implementation** of the existing design;
`DocumentExtractor`/Gemini was queued for re-evaluation, not accepted. Related: LLM chat that
executes commands (Cycle 5), `feature.llm`, `llm/` as it stands; see `tasks.md` § The agreed
sequence. Items 1 and 2 are done, and **v1.1.2 is released and VERIFIED on both devices**.

## What shipped today, and the one thing to watch
A cold-start race, found by the app's own feedback capture on its first real use. The WebView
fires startup invokes before the backend is ready: **`AppState` unmanaged** → Tauri *rejects* →
`get_config` fell back to `Features::default()` (all on) → the phone drew a Finances tab for a
disabled feature (fixed 1.1.1, retry in the shared invoke helpers). **IPC not ready** → Tauri
*drops* it, the promise never settles → the boot future parked forever and the splash never
lifted after the updater's auto-restart (fixed 1.1.2, `invoke_get_config_timed` + the
`continuity.rs` retry loop). ⚠️ **The race still happens — handled, not eliminated:** the phone
recovers all seven boot commands after 1 retry at 567–853ms against a 10s deadline, and those
timings climbing is the cue that the margin is eroding.

## Decisions in force — inherit these
- **Boot invokes need BOTH halves.** `invoke_timed` turns "never settles" into an `Err`; the
  retry re-invokes. Timed-alone fails open with wrong data; retry-alone never fires.
- **Features fail OPEN and stay that way** (`types.rs:243`); a drawn tab is also the error state.
- ⛔ **FINANCES DEFERRED INDEFINITELY**, now **off at the source** — bank credential headers
  renamed on the box, scheduler boots `sources=0`. Detail in the overlay.
- **A public stamp and the overlay `Cargo.lock` move together**: 3 public files + both public
  locks + the overlay lock, or the next `--locked` deploy dies.
- **Real institution names never enter the public repo**; the canonical guard is in
  `omni-me-private/privacy-guard/` and is **not** installed in a fresh clone.
- **CLOSED, do not re-survey:** generalization (Phases 0/A/B/C) — `invariants.md` is the
  contract, never back-fit it — and the whole v1.1.x verification pass.

## Open threads
⚠️ Server stays **1.1.0** (client-only fixes) but the overlay lock expects 1.1.2. · Leg **7b** (a
real panic reaching the ring buffer) still held. · `GET /feedback` **broken** — SurrealDB
`ORDER BY` parse error served as HTTP 200 `text/plain` [XS]. · Every update leaves a husk process
on a deleted `/tmp/tauri_current_app…` binary — 3× today, harmless (no db fds), unexplained. ·
New-projection history gap: **A vs B undecided**. · `chunk_for_push` 413 **unverified**. ·
Curiosities→concepts + memory prune owed (Cycle 4 close-out).
