# NEXT

**Next action: measure the constraint tax on OpenRouter, then close Phase B.** Everything else
in Phase B is built, tested and smoke-run live; the one measurement owed needs an endpoint
serving *both* halves of `--bench`, which Hetzner cannot. Run `omni-me-agent --bench` with
`OMNI_AGENT_LLM_{BASE_URL,MODEL,API_KEY}` on OpenRouter, `OMNI_AGENT_LLM_EXTRA_BODY` pinning the
upstream (unpinned results are unattributable) and `OMNI_AGENT_LLM_MIN_INTERVAL_MS` set. Needs a
seeded hub: [[reference-throwaway-hub-agent-testing]].

## Decisions in force — inherit, do not re-derive
- **The read catalog is NOT `RecordType`.** `RecordType` is a *document form* schema
  (generalization decisions 2 + 9) describing frontmatter keys; it cannot express a routine. The
  assistant reads `core/src/assistant/catalog.rs` — **code, not events**, since every table it
  names is made by a projection in code. `describe_type` reads the live `RecordType` where one exists.
- **The five-verb set is flexible** (user, 2026-09-08); load-bearing is *why* it stays small
  (accuracy degrades past ~15–20 tools). **Propose changes and talk them through.**
- ⚠️ **A listing verb now has live evidence, and it is the first thing to decide.** "What
  routines do I have?" **fails** — six turns, no answer. `search` is text matching and the word
  "routine" appears nowhere in a routine named "Morning"; nothing enumerates a kind of record,
  and no model quality fixes that. The bench keeps the case *because* it fails. Same family:
  date-range retrieval and routine aggregates.
- **`--ask` / `--bench` are test scaffolding** (`--probe` is permanent); model choice stays
  **DEFERRED to Phase D** — a dev endpoint is not a model choice.
- ⛔ **FINANCES DEFERRED INDEFINITELY.** The transaction catalog entry is a *test fixture*.
- **Do not re-survey:** generalization · the model bake-off · hosting · the Phase A checks ·
  **whether SurrealDB FULLTEXT works on SurrealKV — proven, and now load-bearing.**

## Traps the build found, all now regression-tested
BM25 is corpus-relative and legitimately scores `0.0` for a real match in a tiny corpus, so
`search` never thresholds on `> 0` · `search::highlight` marks the whole field and its markers
reached a user-facing answer once, hence keyword-in-context · a verb-call schema needs an
`answer` exit or the model cannot stop · constrained decoding **suppresses tool calling**
rather than failing · rate limiting is *correctness* on a capped tier. Detail:
[[project-assistant-bench-harness-traps]].

## Open threads
**A rule for reaching to widen a mechanism whose original purpose was never read** — design it
later (user, 2026-09-08), with nuance: the tell is *permanent + unversioned* **plus** *intent
unverified*, not "built for something else". [[feedback_prefer_integration_over_rewrite]] points
the wrong way here. · Routine search matches group names, not item names · Gemini has no `chat`
impl · untouched:
`ExtractionResult.total`, `GET /feedback`, server-side `feature.llm`, server 1.1.0 vs overlay
lock 1.1.2, `chunk_for_push` 413, history gap A/B, tasks gate estimation, curiosities prune.
