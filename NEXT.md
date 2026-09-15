# NEXT

**Next action: Stage 3 — RUN THE SLATES.** All five seats now have an instrument and written
thresholds. ⛔ Nothing has touched a real endpoint yet; this is the step that spends tokens.
⛔ Stages A–E of `~/.claude/plans/lets-build-c2-and-snoopy-tome.md` are BUILT — don't redesign.

## ⛔ Decisions in force — inherit, do not re-open
- ✅ **C2 and C3 are IN model selection** (user's call, 2026-09-15, reversing the 2026-09-14 cut).
  Both were excluded for having no caller, not for being wrong. **Both blockers are gone.**
- ✅ **BUILT + VERIFIED 2026-09-15** — the scheduled pass (`document_enrichment.rs`,
  `enrichment_scheduler.rs`), `DocumentTranscriber` (`extraction/transcribe.rs`), `build_reader` +
  `build_transcriber`, and both scorecards: `--bench-reading` (C2), `--bench-transcription` (C3).
  1297 tests, clippy ×3, binary smoke-run ×4 gates, both benches dry-run and refuse. ⛔ **Zero
  tokens.** Rationale: `docs/src/archive.md` + `MODEL_BENCH.md` Parts 8–9; don't restate it.
- ✅ **C2 and C3 thresholds ALREADY WRITTEN** (`MODEL_THRESHOLDS.md`), before any run. Both
  lexicographic: fabrication, then recall, then agreement.
- 🔴 **C2's bench is TEXT-ONLY; C3's corpus is BORN-DIGITAL only.** ⛔ Neither measures reading a
  scan, which is what both roles exist for. Do not publish either seat as proven on scans.
- ✅ **The pass is OFF by default AND capped per tick** (user picked both): `OMNI_ENRICH_ENABLED`
  (fails closed, logs typos at ERROR) · `OMNI_ENRICH_INTERVAL_SECS` · `OMNI_ENRICH_MAX_PER_TICK`.
- ✅ **Server is the only spawn site, derived not chosen** — `archive::ingest_one` has exactly ONE
  caller (`POST /documents/archive`), so the box holds every blob. ⚠️ "Blobs don't sync" is about
  EVENT replication, NOT "a phone capture's bytes stay on the phone". [[project-blobs-do-not-sync]].
- ✅ **The candidate query IS the work queue** — no table, no cursor. ⚠️ Two starvation guards:
  unreadable MIMEs excluded *at the query*, and an empty transcription is APPENDED not skipped.
- ✅ **FIXED 2026-09-15: `archive::derive_text` now emits an email's `Date:`.** It was dropped, and
  nothing else on the row carried it. ⛔ The probe/`derive_text` shape coupling is now guarded by a
  test — the two move together or the bench scores a shape production never sends.
- ✅ **Findings stay RECORDED, not fixed** — three role-D, two C1, C2's `kind`/`title`. ⛔ "No real
  data" was always a WRITE rule: box and phone outside the grant, no deploy, no SSH.
- ⛔ **Never decide inside a noise floor or on preference.** ⚠️ Re-resolve model ids (`-Ultra`=403);
  screen on OpenRouter/DeepInfra, deciding run direct; vision parity still unproven.
- ⛔ **Do not re-survey, ALL CLOSED:** catalogue retrieval · blob store + LRU · archive page ·
  `imap_real.rs` · Phases D–G · finance propose · role wiring · isolation guard · 413 hazard ·
  multi-part docs · enrichment. ⛔ No `HANDOVER.md`.

## 🔴 EVERY cargo invocation goes in the detached systemd unit
`MemoryMax=4G`, `JOBS=1`, `DEV_DEBUG=0`. A bare `cargo test -p omni-me-core` had oomd kill the
terminal AND GNOME Shell today, reading afterwards as an unexplained laptop restart. ⛔ No "this
one is small enough". Recipe + disk reclaim: [[project-dev-machines-and-build-limits]].
