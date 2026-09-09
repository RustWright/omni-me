# NEXT

**Next action: create the DeepInfra account, walked live.** The user asked for this one step
at a time — read the screen, hit the roadblock, handle it — not handed over as a checklist.
Then point `[llm]` at `https://api.deepinfra.com/v1/openai` and re-run **only the two role-A
finalists** (`openai/gpt-oss-120b` on a low-latency tier, and `deepseek/deepseek-v4-flash`).
That doubles as the direct-path smoke test: multi-turn `chat`, tool calls, schema handling,
and the model id accepted by the vendor allowlist.

## Decisions in force — inherit, do not re-derive
- **Provider: DeepInfra, open weights only** (user, 2026-09-08). **Accepted risk, stated: US
  jurisdiction, no recourse against US legal process.** ⛔ Do not reopen; confidential
  computing is the one noted revisit, at the 3–6 month review.
- **Role A has a leading candidate, not a decision** — `gpt-oss-120b` on the fast tier
  (10/10, 3.5s median) with `deepseek-v4-flash` close behind (10/10 both arms, zero reasoning
  tokens, cheapest). Numbers in `tasks.md`; confirm direct before pinning either.
- **Role B is NOT decidable from the 2026-09-09 run — the bench saturated** at 10/10 for four
  models. Do not break that tie by preference; that is the argued-not-measured choice the
  role table exists to prevent. B's real jobs arrive in Phase E/F, and score it then.
- **Roles C, D, E stay empty because they have no caller.** Benchmarking for work nothing
  performs measures a config we cannot ship. Published as *unmeasured*, not *pending*.
- **Role-keyed config stays deferred**, now for a third reason: only role A has a caller at
  all. The interactive winner becomes the configured `[llm]` model; the refactor lands when a
  second role has something to call it.
- **`Session::constrained` STAYS.** The tax is zero where measurable, but zero means
  *harmless*, not useless — and it is the only way to reach an endpoint with no `tools`
  parameter, which is how `llama-4-scout` was benched at all. `--bench --constrained` runs
  that arm alone and reports the tax as NOT APPLICABLE.
- **Compare endpoints, never models.** Same weights + same gateway + different serving tier =
  **3.7× latency**. Third time this has misled the project.
- **Do not re-survey:** the provider field · the Phase 0 bake-off · Hetzner's
  constrained-decoding failure · the slate pins, live-resolved and recorded in
  `scripts/bench-slate.sh`.

## Open threads
`prompts.rs` asks for no `total`, so `ExtractionResult.total` is always `None` — travels with
roles C/D · scanned PDFs need rasterization · confirm a LoRA adapter counts as "Customer
Data" before the first upload (Phase D) · `llama-4-maverick` @ `deepinfra/base` returns
`HTTP 405` on multi-turn; retry another tag if it is ever wanted · the agent is in CI's clippy
and tests but still not its release build.
