# NEXT

**Next action: the long-term LLM provider conversation, in a FRESH session** (user,
2026-09-08 — this one carries too much noise). Planning-first and cross-cutting: self-hosted
vs managed, what ZDR requires, what Hetzner's constrained-decoding failure means for a
self-hosted future, cost. **Only after that** is a bench run worth spending — the constraint
tax is a property of the *serving stack*, so a number is a fact about one endpoint only.

**The bench harness is fixed and primed, not run.** `--bench`/`--ask --constrained` now send
exactly one channel: constrained requests carry `response_format` and **no `tools`**, with the
same verb docs rendered into the system prompt by `verbs::tools_as_prompt()`. New guards:
`Outcome::off_schema` counts replies ignoring the envelope and `--bench` withholds the tax when
any do; `scripts/bench-openrouter.sh` pre-flights the pin. 745 tests + clippy + fmt clean.
⛔ Do not re-open the fix; do not run the bench before the provider decision.

## Decisions in force — inherit, do not re-derive
- **`structured_outputs` is a property of the endpoint TAG**, not the model or provider:
  `gpt-oss-120b` has it on `deepinfra/bf16`/`turbo`, **not** on `deepinfra/fp8`. And
  OpenRouter documents that some providers only treat a schema as a *hint*. Both guards above
  exist for this — [[project-assistant-bench-harness-traps]] (now eight).
- **Bench stack, when a run happens = `z-ai/glm-5.3-flash` @ `deepinfra/fp4`** (user,
  2026-09-08). Overridable via `OMNI_BENCH_MODEL`/`OMNI_BENCH_PIN`. Picks a *stack*; model
  selection stays deferred, and the provider session may replace this outright.
- **The Phase 0 spike is NOT lost** — `~/productive_learning/.archive/poc/llm-quality-spike/`,
  in the **parent** workspace. Inherit: Hetzner's constrained-decoding failure is **deployment
  config, settled by control experiment** (DeepInfra passed the same weights in 1.3s) · **Qwen
  is not a candidate**, it is what Hetzner serves free · its scores are **not comparable to
  `--bench`**. Load-bearing for the provider session.
- **Released, not in an endgame:** v1.0.0 tagged 2026-08-30, now v1.1.2. Read release state
  from `git tag`; a stale memory note said otherwise and misframed this session.
- **Frontend↔core share NO Rust code** (JSON over Tauri IPC) — [[project-cross-boundary-rules-get-a-fixture]].
  **The read catalog is NOT `RecordType`**. **`--ask`/`--bench` are test scaffolding**,
  `--probe` is permanent. ⛔ **FINANCES DEFERRED INDEFINITELY.**
- **Do not re-survey:** generalization · the bake-off · hosting · SurrealDB FULLTEXT on
  SurrealKV (proven) · the OpenRouter key (`[openrouter] api_key`, lifted by the bench script).

## Open threads
Retire `Session::constrained` once the number exists? · routine search matches group names,
not item names · Gemini has no `chat` impl · a rule for widening a mechanism whose purpose
was never read (2026-09-08).
