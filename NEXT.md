# NEXT

**Next action: C2 of Phase C — the reranker, and the measurement that sets its default.**
Add `TextRerank` behind an `assistant.rerank` config key (model name a config key too), then
extend `agent/src/bench.rs` with retrieval cases: for a question with a known correct record,
does it come back, and at what rank? Report **BM25-only vs fused vs fused+reranked**, plus
resident memory and wall-clock per model. The number picks the default — do not pick it first.
Plan: `~/.claude/plans/lets-proceed-warm-stallman.md`.

**C1 is DONE** (2026-09-10): local embeddings, SurrealDB HNSW, RRF fusion, merged results,
agent sweep + `--reindex`. 763 core tests green, clippy clean with and without the feature.

## Decisions in force — inherit, do not re-derive
- **`search` returns ONE merged cross-type ranked list** + `keyword_matches` tallies + a floor
  of one slot per matching type. ⛔ Never revert to per-type grouping. `keyword_matches` counts
  BM25 only, deliberately — every record has *some* vector similarity.
- **The agent lives on the box, permanently resident** (user, 2026-09-09) — it answers at any
  time and the laptop is not guaranteed up. ⛔ Desktop hosting and load-model-around-batches
  are CLOSED.
- **Box is the binding constraint:** 2988 MB available, **0 swap**, 2 vCPU (measured).
  `bge-reranker-v2-m3` (2271 MB) is out on RAM *and* on 2-core latency. Ceiling is
  `bge-reranker-base` on a 4-core box; ship `jina-turbo` (151 MB) as default.
- **Box sizing deferred to C2 on purpose.** Apply regardless: **add swap** (0 swap on a live
  box is a standing hazard) and trim `SURREAL_HNSW_CACHE_SIZE` from its 256 MB default.
- ⚠️ **ONNX Runtime must be Microsoft's build, never ort's.** ort's needs glibc ≥ 2.38; dev is
  Ubuntu 22.04 (2.35) and the Docker base is bookworm (2.36) — links on neither.
  **`source scripts/fetch-onnxruntime.sh` before building the agent.** CI has a step for it.
  The failure reads as undefined C++ symbols and looks like a missing compiler package.
- ⚠️ **Only `<|K,EF|>` (two integers) uses the HNSW index**; `<|K,COSINE|>` parses fine and
  silently table-scans. `the_knn_query_uses_the_hnsw_index` asserts the query plan and is
  verified to fail if that regresses. Also `DISTANCE`, not `DIST`.
- **Do not re-survey:** SurrealDB 3.0.4 KNN grammar, the fastembed API, the model-size table —
  all in the plan file, verified against pinned sources. **DeepInfra GO; model selection
  DEFERRED** — Phase C is local and doesn't touch it. ⛔ Closed.

## Open threads
`target/` was `cargo clean`ed at 43.5 GB on 2026-09-10 (disk hit 100%) — **next build is a full
one** · `server/Dockerfile` needs ONNX Runtime for a *packaged* agent, owed at deployment · C3
docs partly done (`assistant.md` rewritten), subsystem rationale still owed · `[llm] model`
reads un-suffixed `openai/gpt-oss-120b`, the slow tier · `Usage` fix: `reasoning_tokens:
Option<u32>` + parse `estimated_cost` · prompt caching unmeasured · `ExtractionResult.total`
always `None` · agent not in CI's release build.
