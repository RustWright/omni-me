# NEXT

**Next action: item 2 of the agreed sequence — generalization.** Feedback capture is DONE, both
stages built and verified 2026-09-05. **Sequence unchanged: 1. feedback ✅ · 2. generalization ·
3. AI/LLM/ML.** Do not run ahead. Its two device-only legs (cross-device end-to-end, a real
panic) are **deliberately deferred to the next release test pass** (user, 2026-09-05 — test
these together, not piecemeal). Do not re-propose them as standalone work.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY** (user, 2026-09-05). Do not start it, propose it, or
  "just fix" an item. State only: tab offline, both bank sources OFF, categorization to `Unmatched`.
- **Feedback is an EVENT — not a note, not a projection.** `FeedbackCaptured` + one query, no
  projection handler. Screen context is **describe-on-demand** (`screen_context.rs`); a describer
  **summarises, never quotes** — the editor buffer is the one exception, and is droppable.
- **Diagnostics live in `frontend/src/diagnostics.rs`** — a 50-entry `thread_local!` ring, not a
  Dioxus signal: three of four producers fire outside any Dioxus scope where a signal write
  panics. **`install()` must stay in `main()` before `dioxus::launch`** — dx's dev console patch
  wraps ours only because ours goes first; move it and the tap dies. The wrapper is built in JS
  because a Rust closure cannot reach JS `arguments`. **Only a panic persists** (to
  `localStorage`, restored for one relaunch): wasm has no unwinding, so a trapped module can
  never deliver its own ring. Per-error persistence was rejected as churn.
- **`recent_events` is built server-side** in `commands/feedback.rs` — off IPC, unforgeable, and
  event payloads are never included in it.
- **`ContinuityStore` holds ONLY unsaved sessions** — `put` evicts clean ones, so a "saved"
  branch reading it is unreachable. Don't add one.
- ⚠️ `DocumentExtractor`/Gemini re-evaluation **is** item 3 — not settled just because it exists.

## Do NOT re-survey
Feedback storage fork is **CLOSED** — decided, built, tested at 390 and 1280. **Verification PNGs
are throwaway**: `UI_WORKFLOW.md` § Screenshots already says they land at the repo root, are
gitignored by `/*.png`, and get deleted after checking. Never file a task about them again.
`probe_realdb.rs` is **not in this repo** (overlay-side).

## Open threads
⚠️ **Disk AND RAM are both ongoing constraints here** (user, 2026-09-05); disk hit 100% (143M)
mid-session, now 15G. **Never `cargo test -p A -p B`** — the differing feature unification builds
a second artifact set (~1.4G). One crate at a time, `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0`. ·
`npm run copy:editor:dev` after every dx rebuild or CodeMirror 404s. · Only Journal and Notes
publish describers. · Curiosities→concepts pass owed at cycle close · memory prune owed ·
`server_url` precedence question still open.
