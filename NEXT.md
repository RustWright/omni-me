# NEXT

**Next action: finish C1 of Phase C — retrieval.** Foundations are in and green; what remains
is `core/src/assistant/vector_store.rs` (the `record_embeddings` table + HNSW index, the
hash-guarded sweep, the KNN query), then reworking `store::search` + `verbs::search` onto the
merged result shape, then `--reindex` on the agent. Full plan:
`~/.claude/plans/lets-proceed-warm-stallman.md`.

## Decisions in force — inherit, do not re-derive
- **`search` returns ONE merged cross-type ranked list**, plus per-type match tallies, plus a
  floor of one guaranteed slot per type that matched anything. Cosine distance is
  corpus-independent so cross-type ranking is finally honest; the floor stops a 500-entry
  journal burying the one relevant note. ⛔ Do not revert to per-type grouping.
- **Fusion first, reranker second, both measured.** C2 is not done until there is a number for
  retrieval quality with fusion alone AND with reranking, on the same cases.
- **The agent must live on the box, permanently resident** (user, 2026-09-09) — it answers
  questions and scans receipts at any time, and the laptop is not guaranteed up. ⛔ Desktop
  hosting and load-model-around-batches are both CLOSED.
- **Box is the binding constraint:** 2988 MB available, **0 swap**, 2 vCPU (measured).
  `bge-reranker-v2-m3` (2271 MB) is out on RAM *and* on 2-core latency. Realistic ceiling is
  `bge-reranker-base` on a 4-core box. Ship `jina-turbo` (151 MB) as default; **model is a
  config key** so an upgrade is config, not a rebuild.
- **Box sizing deferred to C2 on purpose** — size against measurements. Apply regardless:
  **add swap** (0 swap on a live box is a standing hazard) and trim `SURREAL_HNSW_CACHE_SIZE`.
- ⚠️ **ONNX Runtime must be Microsoft's build, never ort's.** pyke's CDN binary needs glibc
  ≥ 2.38; dev is Ubuntu 22.04 (2.35), Docker base is bookworm (2.36) — links on neither.
  `scripts/fetch-onnxruntime.sh` handles it; **source it before building the agent.** The
  failure reads as undefined C++ symbols and looks like a missing compiler package. It isn't.
- **Do not re-survey:** SurrealDB 3.0.4 HNSW/KNN grammar, the fastembed API, the model-size
  table — all in the plan file, verified against pinned sources. ⚠️ `DISTANCE` not `DIST`;
  only `<|K,EF|>` (two integers) uses the HNSW index.
- **DeepInfra GO; model selection DEFERRED.** Phase C is local and doesn't touch it. ⛔ Closed.

## Open threads
`~/.dotfiles/claude/CLAUDE.md` has an **uncommitted** trim (made room for the new
`explain-before-asking` rule); SessionEnd does not commit dotfiles, so it needs a deliberate
commit or revert · `server/Dockerfile` + CI still need the ONNX Runtime treatment, owed at the
deployment phase · `[llm] model` still reads un-suffixed `openai/gpt-oss-120b`, the slow tier ·
`Usage` fix: `reasoning_tokens: Option<u32>` + parse `estimated_cost` · prompt caching
unmeasured · `ExtractionResult.total` always `None` · agent not in CI's release build.
