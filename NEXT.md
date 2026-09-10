# NEXT

**Next action: the ask/answer INTERFACE, planning-first — agent deployment folds into it.**
Decided 2026-09-10, ahead of the roadmap's Phase D: a question authored as an event on any
device, answered by the agent, the answer syncing back. D's proposals have no trigger until
this or Phase F exists, so D first is as unobservable as a deployed mute agent. Deploy the
agent in that session too, so it reaches the box once, resident *and* usable. ⛔ No
code-review debt exists — do not raise it.

## Decisions in force — inherit, do not re-derive
- **The retrievers are NOT equals** (measured; `MODEL_BENCH.md` § Retrieval decision 6).
  Semantic ranks, keyword supplies only what it missed (`fusion::prefer`). ⛔ Do not re-wire
  RRF: as peers it cost 19 points of top-1, and semantic-only scored *identical* to every
  "fused" number ever recorded — the keyword arm had never voted. `fusion::fuse` stays
  unwired, for a future retriever of comparable precision.
- **Stopwords live in Rust, not the analyzer** — SurrealDB has no such filter. ⚠️ An
  all-stopword query passes through unchanged, or `$q` is empty and matches nothing.
- **The app's search box is SUPERSEDED, not deferred** (user). It uses `CONTAINS`, never the
  FTS index, and the assistant replaces it. ⛔ Never file it as debt.
- **Reranking OFF by default**; default `rerank_model` is `jina-v2-multilingual`, NOT the
  smallest — `jina-turbo` degrades ranking. **Box stays as-is, ⛔ CLOSED.**
- ⚠️ **Never add `-p omni-me-agent` to CI's release line** (`ci.yml:165`). Cargo unifies
  features per invocation, so the server would link `libonnxruntime.so`, which the production
  image lacks — green in CI, dead at container start. Own build step + an `ldd` assertion.
- ⚠️ **Resident and askable are mutually exclusive today** — `surrealkv` holds an OS-level
  exclusive lock, so `docker exec … --ask` cannot open a running agent's DB.
- ⚠️ **`source scripts/fetch-onnxruntime.sh` from the REPO ROOT** before any linking build —
  clippy never links, so green proves nothing. ⚠️ Only `<|K,EF|>` uses HNSW; `fusion::cut`
  does not sort. **Do not re-survey:** SurrealDB KNN + match grammar, analyzer filters (there
  is no stopword one), fastembed, the reranker scorecard, box sizing. **DeepInfra GO.**

## Open threads
**The retrieval fixture cannot adjudicate the keyword pass** — semantic-only scores 1.000 on
its lexical column, so no case exists that a keyword index could win; and one case is
mislabelled because the label test's own stop list excuses "worse"/"day"/"back"/"up"/"out".
Fixing it re-baselines every MODEL_BENCH number, so it is its own change · **stemming needs a
migration** (`DEFINE ANALYZER IF NOT EXISTS` silently no-ops on existing installs) ·
`server/Dockerfile` has no ONNX · `[llm] model` un-suffixed · `Usage` lacks `reasoning_tokens`
+ `estimated_cost` · prompt caching unmeasured · `ExtractionResult.total` always `None` ·
`MEMORY.md` is 20KB against a 25KB cap — prune before entries start dropping.
