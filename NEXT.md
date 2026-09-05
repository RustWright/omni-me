# NEXT

**Next action: Stage 2 of feedback capture — the diagnostic ring buffer.** Stage 1 is built and
verified; Stage 2 is in the same approved plan and is what makes a report actionable. Nothing in
the app records panics, console errors or failed `invoke` calls today, and `tracing_subscriber`
writes to stdout — logcat on Android, gone with the process. Then item 2. **Sequence unchanged:
1. feedback capture · 2. generalization · 3. AI/LLM/ML.** Do not run ahead.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY** (user, 2026-09-05). Do not start it, propose it, or
  "just fix" an item. Only the state survives: tab offline, both bank sources OFF, categorization
  deferred to `Unmatched`.
- **Feedback is an EVENT — not a note, not a projection** (decided and built 2026-09-05).
  `FeedbackCaptured` + one query; no projection handler (projections ignore it via `_ => Ok(())`).
  A note was rejected on merits: it would land in the notes list, search, the Obsidian export and
  the LLM derive pass — and `generic_notes.tags` is written **only** by `on_llm_processed`, so a
  typed `tags: [feedback]` is not queryable.
- **Screen context is DESCRIBE-ON-DEMAND, not the continuity store.** The store persists what
  would be *lost*; a report wants what was *shown*. `screen_context.rs` is a memory-only signal
  pages publish into. **Rule: a describer summarises, never quotes** — Settings reports its
  section and nothing else (it holds the server-token field). The editor buffer is the one
  exception, and is the line the modal lets the user drop before sending.
- **`ContinuityStore` holds ONLY unsaved sessions** — `put` evicts any where
  `content == last_saved_content`, so a "saved" branch reading it is unreachable. Don't add one.
- **No plugin seam for "pluggable" storage.** Storage is the event log; the destination is
  whatever calls `GET /feedback`. Straight-to-GitHub from the app was rejected (write-scoped
  token on the phone, no offline queue, no replication) and never to the public repo.
- ⚠️ `DocumentExtractor`/Gemini re-evaluation **is** item 3 — not settled just because it exists.

## Do NOT re-survey
**Feedback storage fork is CLOSED** — decided, built, tested. All three on-device confirmations
are closed. Cycle 3 review item struck. `probe_realdb.rs` is **not in this repo** (overlay-side).

## Open threads
⚠️ **Stage 1 is UNVERIFIED against a live box.** Capture → sync → `GET /feedback` has run only
against the mock bridge; the cross-device leg is the one part that proves the loop. ·
`npm run copy:editor:dev` must be re-run after every dx rebuild or CodeMirror 404s in dev. ·
7.2GB RAM: `CARGO_BUILD_JOBS=1`, one crate at a time, never two cargo processes. · Only Journal
and Notes publish describers; other pages report position only. · Curiosities→concepts pass owed
at cycle close · memory prune owed · `server_url` precedence question still open.
