# NEXT

**Next action: C3 — finish the retrieval docs, then decide the box.** `assistant.md` and
`MODEL_BENCH.md` § Part 5 are written; what is still owed is the subsystem rationale move
(`retrieval.rs` / `vector_store.rs` module essays → `docs/`, leaving `//!` pointers) per the
comment convention. Then the box-sizing call, which now has numbers instead of estimates.

**C1 + C2 DONE** (2026-09-10): embeddings, HNSW, RRF fusion, merged results, sweep,
`--reindex`, cross-encoder reranking, and `--bench-retrieval` — a local, deterministic,
token-free retrieval scorecard.

## Decisions in force — inherit, do not re-derive
- **Reranking is OFF by default, on the measurement.** Three of four rerankers scored *worse*
  than fusion alone (fusion: 100% found, 75% top-1, MRR 0.865). ⛔ Do not "turn it on for
  quality" — that was tested and is false for three of the four.
- **Default `rerank_model` is `jina-v2-multilingual`, NOT the smallest.** `jina-turbo` fits any
  host and actively degrades ranking (MRR 0.865 → 0.756). A default that hurts when switched
  on is worse than one that needs a bigger box. Full table: `MODEL_BENCH.md` § Part 5.
- **Box sizing, now with numbers:** the only reranker that helps costs ~750 MB–1.1 GB resident
  and 621 ms/query on **twelve** cores. The box has 2988 MB, 0 swap, **2** cores. Reranking is
  not an option there. ⚠️ Still apply regardless: **add swap**, trim `SURREAL_HNSW_CACHE_SIZE`.
- **`search` returns ONE merged cross-type ranked list** + `keyword_matches` tallies + a floor
  of one slot per matching type. ⛔ Never revert to per-type grouping.
- **The agent lives on the box, permanently resident** (user, 2026-09-09). ⛔ Desktop hosting
  and load-model-around-batches are CLOSED.
- ⚠️ **ONNX Runtime must be Microsoft's build.** `source scripts/fetch-onnxruntime.sh` before
  building the agent. The failure reads as undefined C++ symbols and is not a missing package.
- ⚠️ **Only `<|K,EF|>` (two integers) uses the HNSW index**; `<|K,COSINE|>` silently
  table-scans. `DISTANCE`, not `DIST`.
- ⚠️ **`fusion::cut` does not sort** — it takes the caller's order. That is what lets a
  reranked list (cross-encoder logits) pass through beside RRF weights. Do not add a sort.
- **Do not re-survey:** SurrealDB KNN grammar, the fastembed API, model sizes, the reranker
  scorecard. All recorded. **DeepInfra GO; LLM model selection DEFERRED.** ⛔ Closed.

## Open threads
**NEW — keyword search requires EVERY query word.** SurrealDB's `@@` defaults to
`BooleanOperator::And` (verified, `surrealdb-core-3.0.4` `sql/operator.rs:249`), so "when is my
dentist appointment" misses a note titled *Dentist appointment*. Affects the app's search box
too. `@N,OR@` is the opt-in but `omni_text` has no stopword filter, so OR alone would match
everything on "is"/"my" and wreck the `keyword_matches` tally — needs its own design pass ·
`server/Dockerfile` needs ONNX Runtime for a packaged agent, owed at deployment · agent not in
CI's release build · `[llm] model` reads un-suffixed `openai/gpt-oss-120b`, the slow tier ·
`Usage`: `reasoning_tokens: Option<u32>` + parse `estimated_cost` · prompt caching unmeasured ·
`ExtractionResult.total` always `None`.
