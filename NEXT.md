# NEXT

**Next action: decide where verb documentation lives when the reply is schema-constrained
— then, and only then, run the tax.** ⛔ Do not spend a run before that: `Session::ask` sends
`verbs::tools()` **and** `response_format` together, so the model gets two ways to call a verb
and picks one by training. Measured 2026-09-08 on DeepInfra: `gpt-oss-120b` ignored the schema
and used native tool calls (2/2) — a "constrained" run that was not constrained, tax ≈ 0 by
construction; `glm-5.3-flash` emitted a hybrid, `{"verb":"list","arguments":{"name":"search",
…}}`, recording a verb it did not want. Evidence on `Session::constrained`; seven traps now in
[[project-assistant-bench-harness-traps]].

**The fork:** dropping `tools` when constrained is right, but the verb *documentation* lives in
`tools()`, so that half becomes less **informed** and the run measures information loss instead.
Recommendation: render the same docs into the system prompt there, so the halves differ only in
reply format.

⚠️ Second blocker: `glm-5.3-flash` was **rate-limited upstream on every provider tried**
(DeepInfra *and* Fireworks, `limit_source: upstream_provider_shared_pool`) all session. Not our
rate, not credit — ~$20.38 left. Retry later, or use `openai/gpt-oss-120b`, reachable throughout.

## Decisions in force — inherit, do not re-derive
- **Bench model = `z-ai/glm-5.3-flash` pinned to `deepinfra/fp4`** (user, 2026-09-08): the only
  cheap provider serving it with both `tools` and `structured_outputs`, and ZDR per the design
  plan's criterion 3. This picks a *stack*; model selection itself stays deferred.
- **The Phase 0 spike is NOT lost** — `~/productive_learning/.archive/poc/llm-quality-spike/`,
  in the **parent** workspace (its corpus is real journal data). Inherit its conclusions:
  Hetzner's constrained-decoding failure is **deployment config, settled by control experiment**
  (DeepInfra passed the same weights in 1.3s) · **Qwen is not a candidate**, it is what Hetzner
  serves free · GLM 10/12, DeepSeek 9/12 on the spike's own loop, **not comparable to `--bench`**
  (12 cases incl. four `propose`; no write path exists here).
- **Frontend↔core share NO Rust code** (JSON over Tauri IPC); cross-boundary rules are pinned
  by a fixture — [[project-cross-boundary-rules-get-a-fixture]]. **The read catalog is NOT
  `RecordType`** (`core/src/assistant/catalog.rs`, code not events). **`--ask`/`--bench` are
  test scaffolding**, `--probe` is permanent. ⛔ **FINANCES DEFERRED INDEFINITELY.**
- **Do not re-survey:** generalization · the bake-off · hosting · SurrealDB FULLTEXT on SurrealKV
  (proven) · the OpenRouter key — `[openrouter] api_key`, lifted by `scripts/bench-openrouter.sh`.

## Open threads
Routine search matches group names, not item names · Gemini has no `chat` impl · a rule for
widening a mechanism whose purpose was never read (2026-09-08).
