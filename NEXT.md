# NEXT

**Next action: OPEN PHASE 3 BY DECIDING WHICH EVENT CARRIES MODEL-TRANSCRIBED TEXT** — ⛔ a real fork, ⛔ do not guess it in passing. `text` lives on
`DocumentArchived`, written ONCE at ingest, so a scan archived today (`text_source: "none"`) has no route to gain text later. Either transcribe at ingest
(core needs an extractor, and `DocumentExtractor` returns the finance-shaped `ExtractionResult` — there is no transcribe-to-text method) or add a carrier.
⛔ `archive.rs`'s header warns against quietly wiring an extractor in: text added there is unreachable for every document already filed. Then build Phase 3
(fields: parser-derived for statements, model+flagged for the rest). Plan: `~/.claude/plans/lets-continue-sharded-minsky.md`. ⛔ **PLANNING IS DONE.**
✅ **Phases 1–2 BUILT 2026-09-11** — events + projection + `Feature::Documents`; `core::blob` (shared by 3 callers), `core::archive`, `POST
/documents/archive`, `docs/src/archive.md`. **1074 core + all server integration green, clippy clean.** ⚠️ **No real corpus has been ingested yet.**
⛔ **`"documents"` sits TEMPORARILY in `NOT_QUERYABLE`** (`catalog.rs`) — ⛔ **Phase 5's first step deletes it**; left in, the archive is invisible to the
assistant and nothing else reports it. 🔴 **`omni-me-private` holds UNCOMMITTED work the session hooks do NOT reach.**

## Decisions in force — inherit, do not re-derive
- ⛔ **THE ARCHIVE IS THE SPINE** (user, 2026-09-11). Email ingestion and finance statements become **producers into it**; finance propose actions are a
  separate, smaller, LATER item. ⛔ Do not start either before the archive lands.
- ⛔ **Model-extracted fields are ALLOWED and FLAGGED** (user, 2026-09-11). Every field carries `source` (`parser:<id>` / `model:<name>@<ver>` / `human`)
  **and** `verified`. ⚠️ The HAND WASH trap is exactly why: guards inspect arithmetic, and there is none in *"this is a 2023 notice of assessment"*. ⛔ Never
  invent a confidence score implying the flag is stronger than it is. ⛔ **Correcting a field requires the document VISIBLE BESIDE IT** — else it is a guess.
- ⛔ **The viewer renders CLIENT-SIDE, every type, from the LRU cache** — `attachments.rs` exists so a document stays viewable **offline**, and a server
  round-trip defeats that. **PDF → pdf.js** via the esbuild step CodeMirror already uses (⚠️ BOTH copy targets) · **CSV → a table of RAW ROWS**, ⛔ never the
  parser's interpretation · ⛔ **never a download link**. ⛔ `rasterize_pdf` stays EXTRACTION-only. 🔴 **Three live viewer bugs are written up in `tasks.md`
  § Document viewing** (Android has no PDF renderer · HEIC classified viewable but undecodable · CSV has no viewer at all).
- 🔴 **BLOB BYTES DO NOT SYNC** (memory `project-blobs-do-not-sync`) — documents live in **ONE durable place, the box**. ⛔ Server backup is a **dependency
  sequenced before the REAL backfill**, not a chore. ⚠️ **Verify a backup by SIZE, never exit code.**
- ⚠️ **The 8-page PDF refusal is MINE, not the user's** (corrected by him 2026-09-11) — a prior session's default recorded with a ⛔ as though decided.
  **Revisitable.** It constrains **extraction's** base64 budget; ⛔ it must NEVER reach the viewer.
- ⛔ **Role C selection STAYS DEFERRED to the end of this block.** All archive work + backfill testing runs on the **dev sync server** with throwaway data;
  the **REAL** document backfill waits for model selection. ⛔ Never quote a run as a ranking.
- ⛔ **APPROVAL LIVES WHERE THE DATA LIVES** — archive review on the **archive page**, ⛔ never the assistant inbox. ⛔ **THE JOURNAL IS NEVER PROPOSABLE** —
  sole author, readable and citable. ⛔ Beliefs **ask-only**. ⛔ **Irreversible actions can never be granted autonomy**; only the user grants.
- ⛔ **The log carries the outcome, never the anchor** — projections fold in **`received_at`** order, documents included. ⛔ **Notes: the model never sends a
  body** (`note.revise` = `find`/`replace`, approving device splices; exactly one match or refuse). ⛔ **Fan-out stops at `build_events`.**
- ⛔ **Extraction: text first, rasterize only on empty** · ⛔ **`json_schema`, never `json_object`** · ⚠️ budget on **base64 length** · ⛔ in the **extractor,
  not the route**. ⛔ **Workspace is rustls-only** — name `ring` or `ClientConfig::builder()` panics; ⚠️ one dev-dep missing `default-features=false` re-adds openssl.
- ⛔ **Never hardcode a catalogue-derived count** — derive from `catalog::visible`. ⚠️ **v3 refuses `ORDER BY` over a field the selection omits.** ⛔ **Do not
  re-survey, ALL CLOSED:** retrieval is catalogue-driven (`catalog::visible` drives `sweep` + `describe`, so a new entry gets search/embedding/verbs FREE) ·
  blob store + LRU mirror · `extraction/media.rs` · `auto_import/imap_real.rs` · KNN/analyzers/fastembed · Phases D–G · **the corpus** (765 PDF / 276 CSV /
  233 MB; 124 PDFs no parser can touch) · **box capacity** (38G disk, **29G free**, 71M used — ⛔ NOT a constraint).
- ⛔ **Box deploy DEFERRED.** 🔴 **Builds: NOT 5G, NOT `--scope`** — detached **service**, `JOBS=1`, `DEV_DEBUG=0`, ⛔ + `source scripts/fetch-onnxruntime.sh`
  (⚠️ **clippy never links**). ⚠️ Keep logs OUTSIDE the repo — SessionEnd pushes to a **PUBLIC** remote.
