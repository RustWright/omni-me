# NEXT

**Next action (user, 2026-09-10): the keyword-AND defect, then agent deployment**, in that order,
then on to the next step in the intelligence deployment. Plan-mode session after a compact. The
retrieval arc is finished; the box thread is closed. ⛔ No code-review debt exists — do not raise it.

**C1–C3 DONE** (2026-09-10): embeddings, HNSW, RRF fusion, merged results, sweep, `--reindex`,
reranking, `--bench-retrieval`, and `docs/src/retrieval.md`.

## Decisions in force — inherit, do not re-derive
- **Reranking is OFF by default, on the measurement.** Three of four rerankers scored *worse*
  than fusion alone (fusion: 100% found, 75% top-1, MRR 0.865). ⛔ Do not "turn it on for
  quality" — tested, false for three of the four.
- **Default `rerank_model` is `jina-v2-multilingual`, NOT the smallest** — `jina-turbo` degrades
  ranking (MRR 0.865 → 0.756). Table: `MODEL_BENCH.md` § Part 5.
- **Box stays as-is. ⛔ CLOSED** (user, 2026-09-10): box sizing only mattered if reranking ran,
  and it does not. Measured 3820 MB / 2989 avail / 0 swap / 2 cores / 29 GB free, server 279 MB
  — the old "2988 MB" was the *available* column read as total. Do not reopen without a reranker.
- **Retrieval rationale lives in `docs/src/retrieval.md`**, not module headers; comments keep
  traps only. ⚠️ `core/tests/doc_pointers.rs` fails the build on a pointer to a missing anchor.
- **`search` returns ONE merged cross-type ranked list** + `keyword_matches` tallies + a floor
  of one slot per matching type. ⛔ Never revert to per-type grouping.
- **The agent lives on the box, permanently resident** (user, 2026-09-09). ⛔ Desktop hosting
  and load-model-around-batches are CLOSED.
- ⚠️ **`source scripts/fetch-onnxruntime.sh` from the REPO ROOT** before any linking build, or
  `ort-sys` links statically and dies on glibc symbols. Clippy never links; green proves nothing.
- ⚠️ **Only `<|K,EF|>` (two integers) uses the HNSW index**; `<|K,COSINE|>` silently
  table-scans. `DISTANCE`, not `DIST`.
- ⚠️ **`fusion::cut` does not sort** — the caller's order is the answer's. Do not add a sort.
- **Do not re-survey:** SurrealDB KNN grammar, fastembed API, model sizes, the reranker
  scorecard, box sizing. **DeepInfra GO; LLM model selection DEFERRED.**

## Open threads
**Comment doctrine — user decision pending.** The pilot cut the six retrieval files 406 → 319
comment lines (21% → 17%); codebase-wide it is 15,178 comment lines against **two** runnable
doctest lines. Blanket ban not settled · **keyword search needs EVERY query word** (`@@` defaults
to AND); own design pass, `@N,OR@` alone wrecks the tallies · `server/Dockerfile` needs ONNX and
the agent is absent from CI's release build (`ci.yml:165`), both owed at deployment · `[llm]
model` reads un-suffixed `openai/gpt-oss-120b` · `Usage`: `reasoning_tokens` + `estimated_cost` ·
prompt caching unmeasured · `ExtractionResult.total` always `None`.
