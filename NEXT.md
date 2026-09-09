# NEXT

**Next action: fill the five roles with a benchmarked model each.** The provider question is
closed, so a bench run is finally worth spending. `scripts/bench-openrouter.sh --bench` is
primed and now refuses any pin that is not DeepInfra. Screen wide on OpenRouter (pinned to
DeepInfra tags), then confirm the winners direct.

## Decisions in force — inherit, do not re-derive
- **Provider: DeepInfra, open weights only** (user, 2026-09-08). It won on criterion 1
  (*owns no models*), which is where the real argument lives, plus SLA and adapter ownership.
  **Accepted risk, stated: US jurisdiction, no recourse against US legal process.** EU
  jurisdiction was demoted — the CLOUD Act follows the provider not the datacentre, and it
  answered state access rather than the commercial-misuse threat actually held. ⛔ Do not
  reopen; Phala's attested-GPU route is the one noted revisit, at the 3–6 month review.
- **Five roles, model per role**: interactive · batch · quarantined extractor · high-volume
  structurer · local. Table in `docs/src/assistant.md` and `core/src/llm/provider.rs`.
  C-vs-D is a **trust boundary**, not a speed tier. D scores on **abstention**, not accuracy.
- **Role-keyed config is deliberately NOT built.** `[llm]` stays one section until benchmarks
  say what fills each role. That refactor lands *with* the results.
- **Bench = production, structurally** — that is the guarantee, not a promise: the script
  gates on a DeepInfra pin, and requests carry `zdr` + `data_collection:deny` +
  `require_parameters` so a schema-ignoring endpoint errors instead of degrading silently.
- **Gemini is deleted** — client, extractor, `[gemini]` creds, the fallback. `NullLlmClient`
  replaced the fallback and reports *why* at call time. Closed-weights model ids are refused
  unless `[llm] allow_closed_weights = true`, which exists to keep the published promise that
  a commercial model is a supported configuration for **other people's** installs.
- **Finances is deferred, NOT cancelled** (user, 2026-09-08) — do not accept capability
  regressions on that path and call it acceptable because it is deferred.
- **Do not re-survey:** the provider field (41 candidates screened) · the bake-off
  (`~/productive_learning/.archive/poc/llm-quality-spike/`, parent workspace) · Hetzner's
  constrained-decoding failure (settled: deployment config) · the OpenRouter key
  (`[openrouter] api_key`, lifted by the bench script).

## Open threads
`prompts.rs` still asks for no `total`, so `ExtractionResult.total` is always `None` and
`verify`'s arithmetic check is unreachable (one-line schema fix, confirmed by the spike) ·
scanned PDFs need page rasterization, unbuilt · confirm with DeepInfra support that an
uploaded LoRA adapter counts as "Customer Data" before the first upload (Phase D) ·
`Session::constrained` retirement once the constraint-tax number exists.
