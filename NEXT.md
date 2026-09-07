# NEXT

**Next action: intelligence Phase 0 — contract + two spikes.** Item 3 is now PLANNED, not
built. Full design in `~/.claude/plans/lets-plan-how-to-sprightly-hartmanis.md`; read it before
touching anything, it carries the reasoning this file only summarises. Phase 0 is: write the
interface contract into `docs/src/` (docs-before-code, the generalization Phase 0 precedent),
plus a **quality spike** (hand prompts against Hetzner's free endpoint on real data — is the
27–35B class enough?) and a **RAM spike** (does a second SurrealKV fit on the CX22, or is a
CX32 needed?). Both spikes de-risk assumptions the whole plan rests on. **Neither is started.**

## Decisions in force — inherit these, do not re-derive
- **Scope is a personal assistant over the whole app**, not a feature in it. The user's brief
  supersedes `tasks.md` item 3's "re-evaluate DocumentExtractor/Gemini" framing.
- **Nothing in today's `llm/` or `extraction/` is load-bearing.** Verified: the text LLM has no
  UI trigger, extraction's only call sites are behind ⛔-deferred finances. Do not preserve it.
- **A small FIXED verb set** — `search`/`read`/`propose`/`list_types`/`describe_type` — generic
  over declared record types. Tool accuracy degrades past ~15–20 tools, so a tool-per-feature
  surface would rot exactly as the app grows. New features add *data*, never tools.
- **The agent is a headless omni-me device**: own process, own DB, own device id, syncs like the
  phone. Forced by SurrealKV being per-process; chosen so agent failure can't take sync down.
- **Autonomy is earned.** Proposal-only → per-action-type promotion → "reversible acts freely,
  irreversible asks". The proposal lifecycle is audit trail + promotion evidence + LoRA corpus
  + model eval set, all one structure, generalizing `AutoImportBatchProposed`.
- **Hosting: Hetzner's free Experiments endpoint now → an EU ZDR open-weights API long-term.**
  Reasoning is incentive alignment, not physical control (structural privacy isn't buyable at
  this budget and the box already concedes it). **The long-term provider is NOT chosen** —
  Nebius is one candidate; a comparison against comparable services is owed *before* the switch.
  Criteria + candidate list are in the plan. **Do not buy hardware or a GPU box.**
- ⛔ **FINANCES DEFERRED INDEFINITELY.** Both bank sources off at the source.
- **CLOSED, do not re-survey:** generalization (Phases 0/A/B/C), the v1.1.x verification pass,
  and the hosting market survey — it was done properly the second time, conclusions are in the plan.

## Open threads
⚠️ **`guard_event_type` must move into `core` before the agent writes anything** — Phase B's
"a future tool layer inherits it" is FALSE for a separate binary. · Server stays **1.1.0**;
overlay lock expects 1.1.2. · `GET /feedback` **broken** (SurrealDB `ORDER BY` parse error
served as HTTP 200 `text/plain`) [XS]. · `ExtractionResult.total` never populated ⇒ its
self-check is unreachable. · Server does **not** enforce `feature.llm`. · Leg **7b** held. ·
`chunk_for_push` 413 unverified. · New-projection history gap: A vs B undecided. ·
Curiosities→concepts + memory prune owed (Cycle 4 close-out).
