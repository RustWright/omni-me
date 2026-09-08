# NEXT

**Next action: capture a skip reason in the UI.** `pages/routines.rs` ~line 495 calls
`invoke_skip_routine_item(.., None)` — payload, command and column all support a reason, the UI
never asks, so **every skip on record has none**. Time-sensitive in a way nothing else here is:
a reason not asked for at skip time cannot be backfilled. Proven live that the assistant spots
"skipped 5 days running, same reason each time" *when the reason exists*.

**Then close Phase B: the constraint tax on OpenRouter.** Needs an endpoint serving *both*
halves of `--bench`, which Hetzner cannot (8/10 free-form vs 4/10 constrained there, on a stack
whose constrained decoding is already suspect). `cargo run -p omni-me-agent -- --bench` — never
a prebuilt binary — with `OMNI_AGENT_LLM_{BASE_URL,MODEL,API_KEY}` on OpenRouter,
`OMNI_AGENT_LLM_EXTRA_BODY` pinning the upstream (unpinned is unattributable) and
`OMNI_AGENT_LLM_MIN_INTERVAL_MS` set. Seeded hub: [[reference-throwaway-hub-agent-testing]].

## Decisions in force — inherit, do not re-derive
- **The read catalog is NOT `RecordType`.** That is a *document form* schema (generalization
  decisions 2 + 9) describing frontmatter keys, and cannot express a routine. The assistant reads
  `core/src/assistant/catalog.rs` — **code, not events**, since every table it names is made by a
  projection in code; `describe_type` merges in the live `RecordType` where one exists.
- **The five-verb set is flexible** (user, 2026-09-08); load-bearing is *why* it stays small
  (accuracy degrades past ~15–20 tools). **Propose changes and talk them through.**
- **`list` is the fifth verb** (user, 2026-09-08), because `search` matches text and the word
  "routine" appears nowhere in a routine called "Morning". Narrowed by catalog-declared
  `filters`, which `describe_type` publishes; errors name the valid keys so a guess costs no turn.
- **A skipped routine item counts as done** (user, 2026-09-08). `routines::roll_up` owns that
  rule for the screen *and* the assistant; `skipped` rides alongside so the distinction survives.
- **`--ask` / `--bench` are test scaffolding** (`--probe` is permanent); model choice stays
  **DEFERRED to Phase D** — a dev endpoint is not a model choice.
- ⛔ **FINANCES DEFERRED INDEFINITELY.** The transaction catalog entry is a *test fixture*.
- **Do not re-survey:** generalization · the model bake-off · hosting · the Phase A checks ·
  **whether SurrealDB FULLTEXT works on SurrealKV — proven, and now load-bearing.**

## Traps the build found, all now regression-tested
BM25 is corpus-relative and scores `0.0` for a real match in a tiny corpus, so `search` never
thresholds on `> 0` · `search::highlight` marks the whole field and its markers reached a
user-facing answer once, hence keyword-in-context · **SurrealDB `ORDER BY` only accepts a column
present in the SELECT** · child rows must declare their order, since completion ids start with
the *item* id · harness traps in [[project-assistant-bench-harness-traps]].

## Open threads
**A rule for reaching to widen a mechanism whose original purpose was never read** — design it
later (user, 2026-09-08), with nuance: the tell is *permanent + unversioned* **plus** *intent
unverified*, not "built for something else"; [[feedback_prefer_integration_over_rewrite]] points
the wrong way. · Routine search matches group names, not item names · Gemini has no `chat`
impl · untouched: `ExtractionResult.total`, `GET /feedback`, server-side `feature.llm`, server
1.1.0 vs overlay lock 1.1.2, `chunk_for_push` 413, history gap A/B, curiosities prune.
