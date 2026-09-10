# NEXT

**Next action: Phase C — retrieval.** `fastembed` embeddings + SurrealDB HNSW + reranker, so
`search` stops being BM25-only. Runs **locally, does not touch DeepInfra**, so model selection
does not gate it. Plan §270 in `~/.claude/plans/lets-plan-how-to-sprightly-hartmanis.md`.
Major cross-cutting phase — give it its own planning-first session, not another's tail.

## Decisions in force — inherit, do not re-derive
- **DeepInfra is a GO. Model selection is DEFERRED** (user, 2026-09-09) until an agent is about
  to go live and be wired to the app. The bench is a **recorded data point**, not a pending
  decision. ⛔ Do not reopen it as "unfinished work".
- **The go/no-go rests on verified API facts, not on the bench** — key, base URL, tool calling,
  schema decoding, ids, retention, limits, cost. Verdict + non-blocker table top of
  `MODEL_BENCH.md`. Low confidence in the *ranking* does not touch the verdict.
- **Provider: DeepInfra, open weights only** (user, 2026-09-08). **Accepted risk, stated: US
  jurisdiction, no recourse against US legal process.** ⛔ Do not reopen; confidential
  computing is the one noted revisit, at the 3–6 month review.
- **Zero-retention needs no configuration** — blanket policy; the Google/Anthropic carve-out is
  guarded by `refusal_reason` alone. **Tier is the model id direct**, not a routing pin:
  `-Ultra` 403s and `service_tier: priority` is not delivered, so the id is the only control.
- **Gateway screening is a validated method** — direct `-Turbo` matched OpenRouter's
  `deepinfra/turbo` within 0.1s. Screen cheap through a gateway, decide direct.
- ⚠️ **Reasoning tokens are NOT reported by DeepInfra.** "of which reasoning 0" means *absent*.
- **`gpt-oss` emits tool calls under a schema on every path** (R8), so its constraint tax is
  un-measurable. A probe said otherwise and was wrong: a probe proves a capability exists,
  never a behaviour absent.
- **Roles B–E stay unfilled.** B saturated the bench; C, D, E have no caller. Published
  *unmeasured*, not *pending*. Role-keyed config stays deferred for the same reason.
- **`Session::constrained` STAYS** (only route to a `tools`-less endpoint) · **compare
  endpoints, never models** · **do not re-survey** the provider field, the Phase 0 bake-off,
  Hetzner's constrained-decoding failure, or the catalogue (ids + prices in `MODEL_BENCH.md`).

## Open threads
`[llm] model` still reads un-suffixed `openai/gpt-oss-120b`, the slow 15.0s tier — harmless
while nothing calls it, wrong to ship · `Usage` fix: `reasoning_tokens: Option<u32>` + parse
`estimated_cost` (breaks test literals, own small task) · prompt caching unmeasured, acts on
the term dominating cost 11–20:1 · bench de-saturation proposal (Part 4), unadopted · role C
needs a vision model, only an `-Exp` one exists · LoRA as "Customer Data" pre-Phase D ·
`ExtractionResult.total` always `None` · scanned PDFs need rasterization · agent not in CI's
release build.
