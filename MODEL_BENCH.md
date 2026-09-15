# Assistant model bench — results log and migration risk register

Working document. Two jobs, and they are separate on purpose:

1. **A results log per role**, extended as each role is benched. A number here is only
   meaningful with its endpoint attached, so every row carries one.
2. **A risk register for the move from OpenRouter to DeepInfra direct.** Screening ran
   through a gateway; production runs direct. Everything the gateway did *for* us is a
   thing we lose, and the register exists so we find out which ones matter before the
   production release rather than after it.

Roles are named in `docs/src/assistant.md` § One model per job. Letters below map to that
table's rows in order: **A** interactive reasoner, **B** batch reasoner, **C** quarantined
extractor, **D** high-volume structurer, **E** local.

---

## Verdict: DeepInfra is a viable path. No show-stopper. Model choice is DEFERRED.

The operative conclusion, decided 2026-09-09. **Model selection is not made and does not need
to be until an agent is about to go live on the server and be connected to the app.** What was
needed now was a go/no-go on the provider, and that is a go.

⚠️ **The go/no-go does NOT rest on the benchmark.** It rests on API facts verified live
against the account — the key works, the base URL works, native tool calling works, JSON-schema
constrained decoding works, both finalists' ids resolve, retention needs no configuration,
concurrency limits are far beyond our load, and a full 10-case agent run costs under $0.02.
None of that depends on the bench being a good instrument.

**The bench only ranks models against each other**, and ranking is exactly the part being
deferred. So low confidence in the bench is compatible with high confidence in the verdict —
they answer different questions. The instrument's own known limits are stated in Part 1 (it
scores the path taken, not whether the answer was right) and Part 4 (it saturates at the top);
those are reasons not to over-trust the *ranking*, and they were documented before the ranking
was produced.

**Known non-blockers, each with its coverage:**

| Finding | Why it does not block |
|---|---|
| Reasoning tokens unreported (R4) | Instrumentation only. Costs are still billed and `estimated_cost` is reported per call. |
| `gpt-oss` returns tool calls under a schema (R8) | The verb loop already parses both channels. It corrupts *tax measurement*, not function. |
| No capability introspection (R6) | The `off_schema` canary catches in-run what a pre-flight would have caught before. |
| No `provider` field (R3) | `service_tier` replaces it, and direct routing removes what the pin was guarding against. |
| `-Ultra` 403, priority tier not delivered | Two latency levers closed; the working one (model id) is sufficient. |

**Genuine dependencies to carry into design, not blockers:**

- **Role C needs a vision model.** `deepseek-ai/DeepSeek-V4-Flash-Vision-Exp` is in the
  catalogue — note the `-Exp`. Untested. Images pass as `data:` URLs but requests 413 above
  ~5 MB, so downscaling is mandatory.
- **Phase D's LoRA question is still open** — confirm an adapter counts as "Customer Data"
  before the first upload.
- **Phase C (retrieval) does not touch DeepInfra** — `fastembed` runs locally. It is unblocked
  regardless of anything on this page.

---

## Part 1 — What the instrument measures

Ten fixed cases (`agent/src/bench.rs`, `CASES`), each a natural-language request paired with
the verb a correct run must reach. Two of the ten expect *prose with no verb* — there is no
write path, and saying so is the right answer.

Each model runs up to two arms:

- **free-form** — verb definitions sent as native `tools`.
- **schema-constrained** — `response_format` sent, no tool definitions. Reaches endpoints
  that offer no `tools` parameter at all, which is the only reason `llama-4-scout` was
  benchable.

The **constraint tax** is free-form score minus constrained score. It is withheld, not
guessed, when the constrained arm was never really constrained (`off_schema` canary) or when
one arm did not run.

✅ **The score captures whether the answer was right, since 2026-09-14** — Part 4's levers 1
and 2 are built. A case now carries an `Answer` expectation alongside its verb: `Contains`
(every listed token must appear in the final answer, case-insensitively, tokens drawn from
the seeded corpus), `Absent` (the record does not exist, so the reply must say so), or
`Unchecked` (the two type-schema cases and the two write refusals, which have no content
ground truth).

A case passes only on **reached AND answered AND content**. The three are reported apart —
`WA` in the per-case column means the right path and the wrong answer, and `content N
case(s) routed correctly and answered wrongly` summarises it — because a content regression
and a routing regression are different findings and one percentage cannot carry both.

⚠️ **What it still does not capture:** whether an `Unchecked` case answered well, and whether
an `Absent` case abstained for the right *reason*. The absence check is a phrase heuristic
(`ABSENCE_SIGNALS`), generous by design: the failure it must catch is a model inventing a
record, and an invented record contains none of those phrases.

---

## Part 2 — Results by role

### Role A — interactive reasoner · benched 2026-09-09 · **DECIDED 2026-09-10: `openai/gpt-oss-120b-Turbo`**

⛔ **Settled — `[llm] model` is pinned to the `-Turbo` id.** The assistant had been running the
**base** tier because `openai/gpt-oss-120b` is the one slug identical in OpenRouter's dialect
and DeepInfra's (R2), so the migration re-resolved every other row and waved this one through.
Direct, the tier is not a routing pin but a separately-priced model id (R1) — so "no suffix"
silently meant "slowest tier".

**Measured on the live ask/answer loop, same question, same corpus, same 5,366-token prompt:**

| Tier | Per-call latency | Whole answer | End to end at the hub |
|---|---|---|---|
| `openai/gpt-oss-120b` (base) | 11.7 / 13.4 / 4.8s | **90,804ms** | +99s |
| `openai/gpt-oss-120b-Turbo` | 0.97 / 1.24 / 1.74 / 2.62s | **6,642ms** | **+18s** |

**13.7× on the answer**, against 4.1× the input rate — ~$0.0006 a question. Accuracy is not a
trade here: both rows are the *same model*, and the bench scored `deepinfra/turbo` 10/10 on the
free-form arm, equal to `bf16`. ⚠️ The 9s of the 18s is the test harness's own cold process
start (`--ask-event` boots a second agent); a real client authors in-process, so the
user-facing figure is **~9s** — 6.6s model, ~2.4s sync.

⛔ **Do not "optimize" the verb loop for latency.** Turn count was the obvious-looking lever and
it was the wrong one: `list_types` cost 11.7s on base and 0.97s on Turbo. The tier was the whole
effect. Prompt caching (R4b) remains genuinely unexploited but needs a DeepSeek id, which this
decision declines.

⚠️ **Stage 3 must set this in the box's own `credentials.toml`.** The deployed agent reads a
mounted file, not this repo's.

Nine slate rows over eight distinct models, all pinned to DeepInfra tags through OpenRouter.
Scorecards in `runs/20260909-093234/` (gitignored). Latency is the **free-form** arm's,
which is the shipping path wherever `tools` exists.

| Model | Endpoint | Free-form | Constrained | Median | Worst | Tax |
|---|---|---|---|---|---|---|
| `openai/gpt-oss-120b` | `deepinfra/turbo` | **10/10** | 9/10 ⚠️ | **3.5s** | 6.3s | not measurable |
| `openai/gpt-oss-120b` | `deepinfra/bf16` | **10/10** | 10/10 ⚠️ | 12.8s | 18.0s | not measurable |
| `deepseek/deepseek-v4-flash` | `deepinfra/fp8` | **10/10** | **10/10** | 8.9s | 15.3s | +0 |
| `qwen/qwen3.6-35b-a3b` | `deepinfra/fp8` | **10/10** | **10/10** | 37.3s | 110.3s | +0 |
| `qwen/qwen3.5-397b-a17b` | `deepinfra/fp8` | 9/10 | 9/10 | 8.0s | 26.1s | +0 |
| `z-ai/glm-5.3` | `deepinfra/fp4` | 9/10 | 8/10 | 10.4s | 21.0s | −10 |
| `deepseek/deepseek-v4-pro` | `deepinfra/fp8` | 9/10 (1 err) | 5/10 (4 err) | 16.2s | 18.7s | −40 † |
| `meta-llama/llama-4-scout` | `deepinfra/fp8` | — | 7/10 | 4.1s | 9.6s | n/a ‡ |
| `meta-llama/llama-4-maverick` | `deepinfra/base` | — | 2/10 (8 err) | 2.0s | 5.9s | n/a ‡ |

⚠️ = the constrained arm returned native tool calls despite carrying no tool definitions
(8 replies on turbo, 6 on bf16), so it was silently a second free-form arm and the tax
describes nothing.
† rate-limited throughout; the −40 is upstream congestion, not a model property.
‡ endpoint offers no `tools`, so no free-form arm exists to subtract from.

**Leading candidate: `gpt-oss-120b` on a low-latency tier.** `deepseek-v4-flash` is the close
alternative — equal accuracy, clean on both arms, and the only candidate spending **zero**
reasoning tokens, at roughly 2.5× the latency.

**Cost, against the live DeepInfra catalogue** (both arms, 20 cases; prices read from
`/v1/openai/models` metadata 2026-09-09, not from a marketing page):

| Direct model id | in / out per 1M | prompt | completion | est. cost |
|---|---|---|---|---|
| `openai/gpt-oss-120b` (base) | $0.037 / $0.17 | 60,584 | 5,438 | **$0.0032** |
| `openai/gpt-oss-120b-Turbo` | $0.15 / $0.60 | 60,584 | 5,438 | **$0.0124** |
| `openai/gpt-oss-120b-Ultra` | $0.20 / $0.95 | — | — | (unbenched) |
| `deepseek-ai/DeepSeek-V4-Flash` | $0.09 / $0.18 | 62,123 | 3,096 | **$0.0061** |
| `deepseek-ai/DeepSeek-V4-Flash-0731` | $0.06 / $0.18 | 62,123 | 3,096 | **$0.0043** |

⚠️ **Speed is a priced product here, not a routing choice.** `-Turbo` costs **4.1× the base
input rate** and `-Ultra` 5.4×. Read against measured latency, the three-way trade is a real
frontier rather than a ranking: base is cheapest and slowest, `-Turbo` is ~4× the cost for the
3.5s tier, and **`DeepSeek-V4-Flash` sits between them on both axes** — about half `-Turbo`'s
cost at ~2.5× its latency.

In a multi-turn verb loop prompt tokens outnumber completion tokens **11:1 to 20:1**, so the
*input* price dominates and the output rate barely registers.

⚠️ **Prompt caching is an unexploited lever, and it acts on the dominant term.** Both DeepSeek
entries carry a `prompt_cache` tag and a `cache_read_tokens` rate of $0.015–0.018 — **4–6×
below their input rate**. The verb loop resends a fixed system prompt and tool definitions
every turn, which is exactly the shape caching pays for. **No `gpt-oss` variant carries the
tag.** Unmeasured, and potentially larger than the differences ranked above.

**Reasoning-token share of output** (free-form): `qwen3.6-35b-a3b` 71% · `glm-5.3` 68% ·
`qwen3.5-397b` 61% · `gpt-oss-120b` ~50% · `deepseek-v4-flash` 0%.

#### Role A re-run DIRECT · 2026-09-09 · `runs/20260909-201045-direct/`

The production path: no gateway, no pin, tier selected by model id. Same ten cases, same
seeded corpus. **Cost is rate-card arithmetic over both arms** (see R4b — the API reports
`estimated_cost` and we do not yet read it).

| Direct model id | Free-form | Constrained | Median | Worst | Tax | Est. cost |
|---|---|---|---|---|---|---|
| `openai/gpt-oss-120b-Turbo` | **10/10** | 9/10 ⚠️ | **3.6s** | **6.1s** | not measurable | $0.0129 |
| `deepseek-ai/DeepSeek-V4-Flash-0731` | **10/10** | 8/10 | 5.0s | 18.4s | **−20** | **$0.0047** |
| `openai/gpt-oss-120b` (base) | 8/10 (1 err) | 10/10 ⚠️ | 15.0s | 36.6s | not measurable | $0.0030 |

⚠️ = `off_schema` fired 6× (R8). "of which reasoning 0" on every row is **not reported**, not
zero (R4).

**R1's mapping is confirmed.** Direct `-Turbo` at 3.6s median / 6.1s worst reproduces the
gateway's `deepinfra/turbo` (3.5s / 6.3s) almost exactly, and base at 15.0s tracks `bf16`
(12.8s). The OpenRouter tags and the direct model ids are the same stacks. **Screening through
the gateway is therefore validated as a method** — the numbers transfer.

**The role-A trade, stated plainly.** `-Turbo` costs **2.8× `Flash-0731`** and buys 1.4s of
median latency — but the tail is the real difference: **6.1s worst against 18.4s**. For an
interactive assistant the worst case is what a user actually feels, and an 18s pause reads as
broken where 6s reads as slow. `Flash-0731` is otherwise the better instrument: equal
free-form accuracy, a third of the cost, a genuinely *measurable* constraint tax, and prompt
caching still unexploited on top.

⚠️ **A 429 arrived on the base model at one request at a time**, nowhere near the 200-concurrent
cap — the documented "occasional 429s while capacity autoscales" (R7). The bounded backoff
handled it. Confirms keeping the retry while dropping the spacing.

### Role B — batch reasoner · **NOT DECIDABLE, instrument saturated**

Four of the nine rows scored a perfect free-form 10/10 — but they are only **three distinct
models**, because the two `gpt-oss-120b` rows are one model on two serving tiers. The
instrument cannot separate the top, and B is scored on quality where A is scored on latency,
so A's tiebreak does not transfer.

⛔ **Do not break this tie by preference.** That is the argued-not-measured choice the role
table exists to prevent. B's real work — derived beliefs, overnight review — does not exist
yet. The seat is published as *unmeasured*, not *pending*.

### Roles C and D — unmeasured, no caller

⚠️ **On record (2026-09-08/09): these stay unbenched until something calls them.** C is a
trust boundary rather than a performance tier; D is judged on *abstention*, which this
instrument does not measure at all. Benchmarking work nothing performs measures a
configuration we cannot ship.

**Role E is no longer in this bucket.** It gained a caller when retrieval landed, and it is
measured by a *different instrument* — see Part 5. The two do not share a scorecard on
purpose: E runs locally with no endpoint, no credentials and no token cost, so folding it in
here would have tied a local, deterministic number to a vendor being up.

*Open question carried into the next bench session:* raising bench difficulty (Part 4) was
discussed as arriving "with category C". C specifically has no caller, so either the
difficulty work lands on a re-run of A/B instead, or a C caller is built first. Not resolved.

---

## Part 3 — OpenRouter → DeepInfra migration risk register

Status values: **verified** (checked against docs or code this session) · **open** (needs the
direct smoke test) · **unresolved**.

### R1 · Serving tier direct — **RESOLVED 2026-09-09, and it is a different mechanism**

Through OpenRouter, `openai/gpt-oss-120b` had three DeepInfra endpoints (`bf16`, `turbo`,
`fp8`) selected by a `provider.only` pin and differing **3.7× in latency**.

Direct, the tier is not a pin at all — **it is a separate model id, separately priced.** The
live catalogue carries `openai/gpt-oss-120b`, `openai/gpt-oss-120b-Turbo` and
`openai/gpt-oss-120b-Ultra` as three distinct entries. So the fast tier *is* reachable, which
is the good news; the cost is that speed is now an explicit line item (4.1× and 5.4× the base
input rate) rather than a free routing preference.

✅ **The mapping is confirmed** (benched 2026-09-09): direct `-Turbo` reproduces the gateway's
`deepinfra/turbo` at 3.6s vs 3.5s median, and direct base tracks `bf16` at 15.0s vs 12.8s.
Gateway screening and the direct path measure the same stacks, so the whole OpenRouter slate
remains valid evidence rather than becoming historical.

All three pass `refusal_reason` (`openai/` is mixed-vendor, open prefix `gpt-oss`).

**Two of the three levers turned out to be closed. Probed live, 2026-09-09:**

- ⛔ **`-Ultra` returns HTTP 403 Forbidden.** It is in the public catalogue but not reachable
  on this account — catalogue presence is not entitlement. Dropped from the slate. Cheap to
  discover by probe; it would have been 15 wasted minutes inside a bench.
- ⛔ **`service_tier: "priority"` is not delivered for `gpt-oss-120b` on this account.** The
  request is accepted, but the response echoes `"service_tier": "default"` and
  `estimated_cost` is unchanged at the standard rate — which is the documented behaviour when
  a model does not support it. So the tier premium is not silently charged, and the lever
  simply does not apply here.

That leaves **the model id as the only working tier control**, which makes R2's discipline —
resolve the exact id, pin the dated form — the whole of the reproducibility story direct.

**Small-prompt latency probe** (3 calls, ~900-token prompt, 20 max tokens — prefill-dominated,
so a floor rather than a workload): base **1.64s**, `-Turbo` **0.83s**, `Flash-0731` **2.51s**.
Cost per call tracked the rate card exactly (base $0.000035, `-Turbo` $0.000140 — 4.0×).
⚠️ Variance is real and large: single calls ranged to 4.5s on base and 13.0s under the
rejected priority flag. Medians over a 10-case bench are the number to trust, not these.

### R2 · Every slate pin is in OpenRouter's dialect — **verified against the live catalogue**

Ids differ on both halves of the slug and are case-sensitive:
`deepseek/deepseek-v4-flash` → `deepseek-ai/DeepSeek-V4-Flash`; `z-ai/glm-5.3` → `zai-org/…`.
Only `openai/gpt-oss-120b` happens to match.

`OPEN_WEIGHT_VENDORS` already aliases the **vendor** half; nothing covers the **model** half.
Every row of `scripts/bench-slate.sh` must be re-resolved before any direct re-run —
`bench-deepinfra.sh` pre-flights the id against the catalogue for this reason.

⚠️ **Prefer the dated id `deepseek-ai/DeepSeek-V4-Flash-0731`.** It is better on all three
axes at once: it pins a version (the reproducibility mechanism that replaces the endpoint
pin), its description calls it the official release superseding the preview "with
substantially enhanced agentic capabilities", **and it is cheaper** — $0.06 input against
$0.09 for the undated id, same output rate. The undated id is what was benched; the dated one
is what should be.

### R3 · Pin attribution loses its evidence channel — **verified**

`ChatResponse.provider` reads a top-level `provider` field that **OpenRouter** sets
(`openai_compat.rs:352`). DeepInfra's response has no such field, so it parses to `None` and
`bench-slate.sh`'s "NOT ALL CALLS WERE SERVED BY DEEPINFRA" check finds nothing to compare —
it passes vacuously on every direct run. Not a bug; the guarantee simply does not transfer.

Candidate replacement: log the `service_tier` the response echoes back (R1), which at least
evidences the tier.

### R4 · `reasoning_tokens` is NOT reported direct — **CONFIRMED 2026-09-09, live**

A raw response body direct carries exactly:

```
prompt_tokens · completion_tokens · total_tokens · estimated_cost · prompt_tokens_details: null
```

**No `completion_tokens_details`, so no `reasoning_tokens`** — while the message itself carries
a `reasoning_content` field, i.e. the model is demonstrably reasoning. `Usage::from_response`
(`chat.rs:193`) reads that path with `unwrap_or(0)`, so **every direct scorecard reports
reasoning = 0**, and absence is indistinguishable from a true zero.

⚠️ **Read every direct run's "of which reasoning" as NOT REPORTED, never as zero.** It would
otherwise erase `deepseek-v4-flash`'s one genuinely distinguishing property by making every
model look the same on it. `total_tokens` = `prompt` + `completion` exactly, so reasoning is
inside `completion_tokens` — billed, just not broken out.

**Work item (deliberately not done mid-migration):** make `reasoning_tokens` an `Option<u32>`,
mirroring `provider: Option<String>` — `None` for not-reported, `Some(0)` for a real zero.
Deferred because any new/changed field breaks the explicit `Usage` literals in `session.rs`
and `bench.rs` tests, and editing source under a running bench is what produced this project's
one fabricated constraint tax.

### R4b · `estimated_cost` is reported direct, and we ignore it — **work item**

Every direct usage object carries **`estimated_cost`**, the provider's own figure for what the
call cost. The gateway did not give us this, and every cost number in Part 2 is rate-card
arithmetic reconstructing something the API will simply state.

Parse it into `Usage` alongside the R4 fix; it retires the reconstruction and, unlike token
counts, needs no per-model price table to stay correct when prices change.

### R5 · The `extra_body` provider block must be dropped, and its guarantees with it — **verified**

`bench-openrouter.sh` sends
`{"provider":{"only":[…],"allow_fallbacks":false,"zdr":true,"data_collection":"deny","require_parameters":true}}`.
None of these are DeepInfra parameters.

**`require_parameters` is the load-bearing loss.** It made "this endpoint ignores
`response_format`" arrive as a *routing error*; direct, an ignored parameter is silent — the
exact failure mode this harness has been bitten by repeatedly.

**`zdr` and `data_collection` have no direct equivalent and need none.** Corrected 2026-09-09:
an earlier draft of this entry claimed they become account settings to verify in a dashboard.
They do not — DeepInfra's published policy is **blanket zero-retention by default**, SOC 2 and
ISO 27001 certified, with no per-account toggle found. Nothing to configure, so **nothing to
forget to configure** — which is a better posture than the per-request flag it replaces. The
one carve-out is the familiar one: the policy does not cover Google or Anthropic models
reached through the same key, and `refusal_reason` is what guards that, not any request field.

Whether DeepInfra rejects or ignores an unknown top-level `provider` key is not documented.
Strip it rather than find out in production.

### R6 · The pre-flight capability check is gateway-only — **verified**

The pre-flight queries `openrouter.ai/api/v1/models/{id}/endpoints` for `supported_parameters`
and aborts before measuring a constrained arm that was never constrained. DeepInfra offers no
equivalent machine-readable introspection — capability appears as human-readable tags on model
pages. The guard does not port.

Fallback already exists: the run's own `off_schema` canary, which is what actually caught
`gpt-oss-120b`.

### R7 · Rate-limit shape inverts — **verified**

OpenRouter: a shared upstream pool, where 429s arrive from *other people's* traffic
(`upstream_provider_shared_pool`). That is why `OMNI_AGENT_LLM_MIN_INTERVAL_MS=1000` and the
bounded backoff exist. DeepInfra direct: **200 concurrent requests per model**, 429 on
exceed — a concurrency cap, not a rate cap. The agent issues one request at a time.

⚠️ **Do not inherit the 1s spacing as a production default.** It is a gateway workaround that
would cap the agent at one request per second for no reason. Keep the backoff — the docs note
occasional 429s below the limit while capacity autoscales.

Also: `deepseek-v4-pro`'s −40 tax was rate-limit noise and may simply disappear direct.

### R8 · Tool calls under a constrained request — **PERSISTS direct. It is the model.**

Through OpenRouter, `gpt-oss-120b` returned `finish_reason: "tool_calls"` on requests carrying
**no** tool definitions, silently turning the constrained arm into a second free-form arm.

⚠️ **A single raw probe suggested this had gone away direct, and that read was wrong.** One
hand-built call — no system prompt, a two-field schema — came back as clean schema-conforming
JSON, and it was reported here as resolved. The full bench then fired the `off_schema` canary
**6 times on each `gpt-oss` run**, and the tax is NOT MEASURABLE on both, exactly as through
the gateway.

**The lesson is the probe, not the model.** A capability probe and a workload differ in the
ways that decide this behaviour — a system prompt full of verb documentation, a richer schema,
tool-flavoured conversation history. A probe can prove a capability *exists*; it cannot prove
a behaviour *absent*. Ordering matters too: this is why the canary lives in the run rather
than in a pre-flight.

So the two-channel behaviour travels with the weights, not the gateway. `deepseek-v4-flash`
does not do it — its constrained arm scored a real, measurable **−20pp** tax. Keep the
`off_schema` canary; it is the only reason either number is trustworthy.

### R3b · `service_tier` replaces `provider` as the attribution channel — **verified**

Direct responses carry no `provider`, but **do** carry a top-level `service_tier`. That is the
substitute named in R3, confirmed to exist rather than assumed. Worth logging where `provider`
is logged today.

### R9 · `llama-4-maverick`'s HTTP 405 — **unresolved**

Eight 405s from `deepinfra/base` on multi-turn. Gateway routing fault and endpoint fault are
indistinguishable from here. Only matters if Maverick is ever wanted.

### R10 · The harness itself is OpenRouter-shaped — **verified**

`bench-openrouter.sh` lifts its key from `[openrouter]`, enforces a DeepInfra *pin*, and
pre-flights against OpenRouter's API. A direct run needs either a sibling script or a mode
flag. Work item, not a risk to a number.

### R11 · Sweep cost does not reconcile — **open**

The recorded sweep cost is ~$3 (`tasks.md`). Priced against DeepInfra's direct rates the two
`gpt-oss-120b` rows come to ~$0.003 each and `deepseek-v4-flash` to ~$0.006; the whole slate
should land well under $0.20. That is a ~60× gap.

Either the ~$3 was an estimate rather than a billed figure, or gateway pricing is far above
the direct rate — and if it is the latter, that is itself a migration finding in DeepInfra's
favour. **Check the OpenRouter billing page for the actual charge** rather than resolving it
by arithmetic.

---

## Part 4 — De-saturating the bench · **levers 1 and 2 BUILT 2026-09-14**

Saturation is structural, not a matter of the cases being too easy. With five verbs and a
near-obvious request→verb mapping, a competent model picks right almost always, and the score
records only *which verb was reached* — never whether the answer was right. Difficulty has to
come from somewhere other than verb naming.

Levers, ordered by how much they buy per unit of "still a reasonable request":

1. ✅ **Score the answer, not just the path. BUILT.** `Answer::Contains` on the five cases
   with solid ground truth (`list_types`, the rent search, the 03-14 journal read, the
   dentist search, the routine list, the grocery note). Tokens are picked to survive
   paraphrase — a bare number or a proper noun, never a phrase the model could reword, and
   `evening` rather than `winddown` because "wind-down" is equally correct.
2. ✅ **Absent answers. BUILT.** Four cases whose honest answer is "that is not in here":
   a journal date outside the seeded range, a note that does not exist, a topic never
   written about, and a spend figure nothing carries.

   Two design choices in these worth keeping. **Each is the twin of a positive case** —
   same request shape, same expected verb, only the record missing — so holding the path
   fixed isolates abstention from retrieval. And **the verb is still required**, so
   answering "no" without looking does not pass: abstention without checking is a guess
   that happened to be right. This is also role D's abstention instrument.
3. **Distractor density.** Seed a much larger corpus with near-miss content. "Find anything I
   wrote about rent" is trivial across a dozen notes and hard across hundreds where forty
   mention rent and one is the one meant. Realistic — the live corpus is ~14,412 events.
4. **Multi-hop composition.** Questions needing two records combined. Realistic, and it is
   what a personal assistant is actually for.
5. **Token pressure.** Long records that push prompt size. Real, but the weakest lever on its
   own and the one that most easily becomes difficulty for its own sake.

The remembered POC result — needle obscurity plus token size made the difference — is levers
3 and 5. They work, but 1 and 2 buy more here and 1 is nearly free. **3–5 remain unbuilt.**

⚠️ **The case count moved from 10 to 14**, so one case is worth ~7 points of top-1 rather
than 10. Still coarse: any gap under ~7 points is inside the noise floor and decides nothing.
`the_case_mix_keeps_the_noise_floor_and_the_abstention_arm_honest` guards the mix — at least
14 cases, at least 3 absent, and at least half checking the answer — so adding cases to only
one side fails a test instead of quietly skewing the instrument.

### It worked, and it found a bug on its first run · 2026-09-14

Re-run against **`openai/gpt-oss-120b-Turbo` direct**, deliberately the control: the model this
file records as scoring **10/10 (100%)** free-form on the old instrument.

| Arm | Old | New | Notes |
|---|---|---|---|
| free-form | 10/10 (100%) | **10/14 (71%)** | median 3.4s, worst 5.0s; 85,800 prompt / 4,229 completion tokens, 55 turns |
| schema-constrained | — | **9/14 (64%)** | median 3.1s, worst 6.4s; 38 turns |

⛔ **The constraint tax was correctly WITHHELD** — 7 constrained replies ignored the schema, so
this endpoint treats `response_format` as a hint rather than a grammar. The `off_schema` canary
fired exactly as designed; the seven harness traps are intact.

**29 points of headroom opened on a model that previously scored perfect.** All ten original
cases still pass, so lever 1 de-saturated nothing on its own — the old 10/10 was not masking
wrong answers. **Every point of the spread came from lever 2.**

✅ **The instrument discriminates rather than failing everything:** case 10 **passed** its
abstention check in the constrained arm (`list_types → describe_type → read`, then said the
date has no entry). `Absent` accepts a genuine abstention; it is not an always-fail.

🔴 **The first run found a real defect — `tasks.md` § Answering "not found", now fixed.**
Free-form cases 11–13 ended `stopped=TurnBudget` and **never answered at all**; 3 of 28
requests died that way, all three absent-retrieval.

⛔ **The first diagnosis was wrong, and the correction is the useful part.** It read as a
progress-free retrieval loop. Pulling the actual tool arguments showed **exploration**: narrow
the type, drop the filter, broaden the term, enumerate, try another record type — one repeated
call in six turns. The same question at budget 12 answered correctly in **8** turns.

**So the finding is an asymmetry, not an incapacity:** confirming a record exists costs **2–4**
turns, establishing one does not exist costs **8**, and the default of 6 sat between them.
⚠️ **A turn budget sized on positive lookups structurally cannot answer a negative one.**
Fixed by `LAST_TURN_NUDGE` (tools withheld on the final turn, so prose is the only reply) plus
the default moving 6 → 10.

⚠️ **These numbers therefore measure the OLD budget.** A re-run at the shipped default is owed
before the instrument decides a seat — three of fourteen cases were scoring the budget.

⚠️ **One expectation to revisit before this decides a seat:** case 10 requires `read`, and
free-form reached `list` instead — a legitimate way to establish that a journal date is
absent. It failed the content check too, so the verdict stands either way, but `list` should
probably be an accepted path.

⚠️ **Any difficulty increase must not reintroduce the seven harness traps** that made an
earlier constraint-tax number a measurement of the harness rather than the model. A harder
bench has more places to hide one.

---

## Part 5 — Retrieval (role E) · `--bench-retrieval`

A **different instrument** from Parts 1–4, deliberately. Those score an LLM's verb choice over
a network: credentials, an endpoint, a rate limit, tokens spent. This one is local,
deterministic and free, so it is a thing you can run on every change rather than once per
provider decision. `agent/src/retrieval_bench.rs` holds it; `--bench-retrieval` runs it.

### What it asks

For a question whose correct answer is a known record: **does that record come back, and at
what rank?** Three arms over identical data — keyword only (what shipped before retrieval
landed), fused (BM25 + vectors combined by reciprocal rank), and fused plus each reranker.

Reported per arm: `found` (the right record appeared in the top 10 at all), `top-1` (it
appeared *first* — the number that matters most, because the model reads the head of the list
and stops), and MRR, which separates "second" from "ninth" where `found` treats them alike.

### The split column is the honesty mechanism

Cases are labelled **lexical** (the question shares a distinctive term with its answer) or
**semantic** (they share no content word at all), and MRR is reported for each separately.

⚠️ **This exists because a fixture written by the same hand that writes the questions can
prove anything.** A corpus of pure paraphrase cases would show embeddings winning by
construction — keyword search cannot match words that are not there. The lexical column is the
guard: it is the half BM25 can win and a vector index can *lose*, so a change that lifts the
semantic column while sinking the lexical one shows up as what it is. A test in the module
enforces the labels rather than trusting them, comparing whole words, and it has already
caught two cases labelled semantic that shared a term with their answer.

Remaining limits, stated rather than discovered later:

- **44 records is not a corpus.** BM25 is corpus-relative, and rankings over a few dozen
  documents behave unlike rankings over the ~14,412-event live log. Absolute numbers here are
  weaker evidence than the *ordering between arms*, which is what the default is set from.
- **Sixteen cases means one case is six points.** Differences smaller than that are noise.
- The fixture is fictional throughout — no real names, places or amounts — which is a privacy
  requirement and also means it does not carry the live corpus's idiosyncrasies.

### Cost is half the result

The deployment host has **3820 MB total, no swap, 2 vCPU** (re-measured on the box 2026-09-10;
an earlier revision of this section said 2988 MB, which was the `free -m` **available** column
mistaken for the total — see decision 3). A reranker is resident for as
long as the agent is, so its memory is a permanent floor rather than a transient peak, and
zero swap makes pressure an OOM kill rather than a slowdown — with no guarantee the kernel
picks the agent over the sync server. So every arm reports resident memory and per-query wall
clock alongside its rank metrics. **A model that wins on rank and does not fit has not won.**

⚠️ Two caveats on those figures, both structural. RSS is process-wide and allocators rarely
return freed pages, so a model measured after another reads smaller than it is — treat the
deltas as a floor. And the timings are the dev machine's; the host has two cores, where
per-query cost scales with the reranker and barely at all with the embedder.

### Results · run 2026-09-10 · release build, dev machine (12 cores, 7.4 GB)

Corpus: 24 notes, 20 journal entries. 16 cases, 7 lexical / 9 semantic. Top-10.

**Run twice, and every rank metric reproduced exactly** — all six arms identical on found,
top-1, MRR and both split columns. Only timings (±10%) and RSS (±5%) moved. Worth stating
because it settles two things at once: the ranking half is deterministic, so a future
difference is a real change rather than run-to-run noise, and the numbers are not from a stale
binary — the second run rebuilt from source first. (`--bench`'s own documentation records a
scorecard once produced from a binary eight minutes older than the fix it was testing.)

| arm | found | top-1 | MRR | MRR lex | MRR sem | median | worst | RSS (abs / Δ) |
|---|---|---|---|---|---|---|---|---|
| keyword only | 19% | 19% | 0.188 | 0.429 | **0.000** | 1ms | 2ms | — |
| fused `bge-small-en-v1.5` | 100% | 75% | 0.865 | **1.000** | 0.759 | 15ms | 22ms | 215M / +178M |
| + `jina-turbo` | 100% | 62% | 0.756 | 1.000 | 0.567 | 204ms | 236ms | 411M / +179M |
| + `bge-reranker-base` | 94% | 75% | 0.820 | 0.875 | 0.778 | 556ms | 623ms | 1792M / +1491M |
| + `jina-v2-multilingual` | 100% | **88%** | **0.922** | 0.893 | **0.944** | 621ms | 666ms | 1820M / +743M |
| + `bge-reranker-v2-m3` | 100% | 75% | 0.849 | 0.929 | 0.787 | 1638ms | 1823ms | 1911M / +784M |

### Re-run 2026-09-10, after the keyword pass was fixed

Same fixture, same binary settings. The keyword pass no longer requires every word of the
question (`@N,OR@` plus an application-side stopword filter), and the two retrievers are no
longer fused as peers — the semantic ranking decides the order and keyword supplies only what
it missed. Decision 6 below is the argument.

| arm | found | top-1 | MRR | MRR lex | MRR sem | median | worst | RSS (abs / Δ) |
|---|---|---|---|---|---|---|---|---|
| keyword only | 50% | 38% | 0.438 | 0.857 | 0.111 | 2ms | 2ms | — |
| combined `bge-small-en-v1.5` | 100% | 75% | 0.865 | **1.000** | 0.759 | 13ms | 21ms | 217M / +180M |
| + `jina-turbo` | 94% | 62% | 0.750 | 1.000 | 0.556 | 191ms | 232ms | 409M / +175M |
| + `bge-reranker-base` | 94% | 75% | 0.819 | 0.873 | 0.778 | 555ms | 700ms | 1793M / +1517M |
| + `jina-v2-multilingual` | 100% | **88%** | **0.919** | 0.886 | **0.944** | 657ms | 773ms | 1848M / +772M |
| + `bge-reranker-v2-m3` | 100% | 75% | 0.849 | 0.929 | 0.787 | 1762ms | 2189ms | 1952M / +826M |

The keyword row roughly doubles on every metric; the combined row is unchanged to three
decimals; every reranker row is within 0.006 of its previous value. Strictly better, nothing
regressed — which is the whole claim.

⚠️ **`MRR sem` on the keyword row should be 0.000 and is 0.111.** That is one case, and it is
a fixture defect rather than a retrieval result: "the leak in the kitchen is getting worse" is
labelled semantic against an entry reading "The tap has got **worse**". The label test excuses
it because *its own* stop list contains "worse" — along with "day", "back", "up", "out", "get"
and "much", none of which are function words. Two definitions of "function word" now exist in
the repo and they disagree. Fixing it re-baselines this table, which is why it was not fixed in
the same change that produced it.

⚠️ **`jina-v2-multilingual` moved 0.922 → 0.919, and the cause is real rather than noise.** The
reranker's candidate pool now includes records only the keyword pass found, so it is judging a
slightly different set. That is deliberate — it is the one path by which a keyword-only find can
reach the top — and it cost 0.003 MRR here.

### What it decided

**1. Reranking stays OFF by default.** `assistant.rerank` defaults to `false`.

**Three of the four rerankers made retrieval worse than not reranking at all.** That is the
finding, and it is not the one that was expected. Fusion alone already returns the right
record 100% of the time and puts it first in 75% of cases; against that, a cross-encoder has
little room to help and plenty to break. Only `jina-v2-multilingual` improved on it.

**2. The default model changed from `jina-turbo` to `jina-v2-multilingual`** — because the
default is what you get when you flip the switch, and flipping it must not make things worse.
`jina-turbo` was the provisional default on size alone (151 MB, the only one that certainly
fits the host). It is the worst performer here: MRR 0.865 → 0.756 and top-1 75% → 62%,
almost entirely on semantic cases (0.759 → 0.567). A 37M-parameter cross-encoder is
apparently too small to improve on a 384-dimension bi-encoder's ordering, and confident
enough to damage it.

⚠️ **Sample-size honesty: 16 cases means one case is six points of top-1.** The gap that
carries weight is `jina-turbo` *hurting*, which shows in three separate columns and in the
direction fusion alone already argues for. `jina-v2-multilingual`'s win — top-1 75% → 88%, two
cases — is suggestive, not established. It is enough to pick between two models; it is not
enough to claim reranking is worth its cost.

**3. The reranker that helps is blocked by cores, not by memory.** ⚠️ **This reverses what this
section said on 2026-09-10 before the host was re-measured.** `jina-v2-multilingual` costs
~750 MB resident (floor; likely nearer its 1.1 GB of weights) and 621 ms per query on *twelve*
cores.

The host has **3820 MB total with 2989 MB available** and the sync server resident at 279 MB, so
a 1.1 GB model plus the agent's own ~215 MB embedder and ≤256 MiB HNSW cache **does** fit, with
roughly a gigabyte to spare. The earlier "does not fit" read the available column as the total
and then subtracted from it a second time.

What does not fit is the **wall clock**. 621 ms on twelve cores implies something in the region
of three to four seconds per query on two, and a reranking pass is per-question latency the user
waits on. Doubling the box to four cores would still leave roughly two seconds. So reranking is
not a box-size problem with a purchase attached to it; it is a problem that this class of
machine does not solve at any tier Hetzner sells cheaply.

**4. It also trades, rather than simply winning.** `jina-v2-multilingual` lifts semantic MRR
0.759 → 0.944 and *drops* lexical 1.000 → 0.893. Net positive here, but it is the split column
doing its job: reranking is not free accuracy, it is a different set of mistakes.

**5. Resizing the box is priced, and it does not buy the thing it looks like it buys.**

The host is a Gen2 **CX22** (2 vCPU Intel Skylake, 4 GB, 40 GB, Nuremberg), created 2026-05-18 —
*before* Hetzner's 15 June 2026 price increase, so it bills at a grandfathered rate. The nearest
double-the-RAM tier is **CX33** (4 vCPU / 8 GB / 80 GB) at €8.49/mo.

⚠️ **The rescale is technically reversible but financially one-way.** Hetzner's "CPU and RAM
only" option keeps the disk and can be undone, and the operation costs a power-off and an
automatic restart — minutes. But rescaling re-prices the server onto the current sheet, and
scaling back down afterwards returns to CX23's €5.49, never to the pre-June rate. The real
question is therefore not "+€4/mo" but "give up the grandfathered rate permanently".

And it would buy the wrong resource. Per decision 3, memory is not the constraint — the current
box already has the headroom. The constraint is two cores, and CX33's four still leave a
reranking pass around two seconds per query.

⛔ **CLOSED 2026-09-10: the box stays as it is.** The user's reasoning ends the thread rather
than deciding it — box sizing is entirely downstream of running a reranker, and reranking is
off on the measurement above. The sizing work is recorded here so it need not be redone, not
because a purchase is pending. Reopen only if a reranker is actually adopted.

Two things apply to the host regardless, and neither is about reranking:

- **Add swap.** With none, memory pressure is an OOM kill rather than a slowdown, and nothing
  guarantees the kernel picks the agent over the sync server.
- **Set `SURREAL_HNSW_CACHE_SIZE` explicitly on the agent process** — it is the only process
  that builds an HNSW index; the sync server never calls `init_schema`. ⚠️ Verified in
  `surrealdb-core-3.0.4` (`cnf/mod.rs:199`): it parses as a **raw `u64` of bytes** via the plain
  `lazy_env_parse!`, not the `bytes` variant, so `64MB` does not fail — it falls back to the
  256 MiB default, silently.

Measured on the box 2026-09-10, closing the half that was previously unmeasured: **3820 MB
total, 830 MB used, 2989 MB available, 0 swap; 38 GB disk with 29 GB free (20% used); the sync
server (`omni-me-private`) resident at 279 MB.** The agent is its own device with its own
embedded SurrealKV, so it adds a **second full replica** — a disk cost as well as a memory one —
and 29 GB of free disk is the budget it has to fit inside.

### The keyword baseline is handicapped, and the number must be read with that

⚠️ **19% is not BM25's ceiling. It is an AND.** SurrealDB's `@@` operator defaults to
`BooleanOperator::And` (verified in `surrealdb-core-3.0.4`, `sql/operator.rs:249`), so
`store::search` requires **every word of the question** to appear in the record. All three
cases it won are ones where that holds — `sourdough`, `passport renewal`, `coffee grinder
settings`. "when is my dentist appointment" returns nothing despite a note titled *Dentist
appointment*, because no record contains "when".

Two consequences, and they point in opposite directions:

- **The keyword-vs-fused gap overstates the win.** A keyword system with OR semantics and
  stopword handling would score well above 0.188. Nothing here measured one.
- **The reranker comparison is unaffected.** Every reranker arm sits on the *same* fused
  baseline, so which reranker wins is measured independently of this. The C2 decision does not
  rest on the handicapped row.

**Fixed 2026-09-10** — see the re-run table above and decision 6 below. `@N,OR@` plus an
application-side stopword filter took the keyword row from 0.188 to 0.438 and its lexical
column from 0.429 to 0.857. ⚠️ The claim above that it "also changes the app's own search box"
was **wrong**: the app's search UI calls `queries::search_generic_notes`, a `CONTAINS`
substring match that never touches a full-text index.

**6. Fusing the two retrievers as peers is retired — it was never doing anything, and once the
keyword pass worked it did harm.**

This is the finding the fix surfaced, and it is larger than the fix. Ranking on the semantic
pass alone scores **100% / 75% / 0.865 / 1.000 / 0.759** — identical, to three decimals and on
every column, to what this document has been calling the *fused* row. Adding the reranker on
top of semantic-only reproduces the fused reranker rows too. **The keyword pass had never
altered a single ranking**, because AND matching kept it silent on 81% of cases and `merge`
drops an empty ranking before fusing. What was measured as fusion was the embedder wearing a
second name.

The moment the keyword pass started returning results, fusing them cost **19 points of top-1
(75% → 56%) and 0.139 MRR (0.865 → 0.726)**.

⚠️ **The cause is not a noisy tail, and that was tested.** Truncating the keyword ranking to
its top 3, 5 or 10 before fusing changed MRR by at most 0.010 and left top-1 at 56% in every
case. Reciprocal Rank Fusion at `k = 60` is nearly flat — rank 1 contributes `1/61`, rank 20
contributes `1/80` — so *presence in both lists* outweighs *first place in one*. A wrong record
placed fifth and eighth scores 0.031; the right record placed first in one list scores 0.016.
RRF is built on the premise that agreement between retrievers is evidence, and a retriever that
matches any record sharing any word with the question makes agreement cheap.

So retrieval now combines by precedence: **semantic ranks, keyword contributes only what
semantic missed, placed below it.** With no embedder the semantic ranking is empty and keyword
order is the answer, which is what a build without the feature should do. `fusion::fuse`
remains in the source, unwired — the argument for RRF survives, it just needs a second
retriever of comparable precision to be true.

⚠️ **What this does NOT establish.** Semantic-only scores a perfect **1.000** on the lexical
column, so this fixture contains no case a keyword index could win. The claim is that fusion is
unevidenced and currently harmful here, **not** that a keyword pass is worthless. Cases built
to defeat an embedder — an account number, an error code, an exact date, an unusual proper noun
— are the prerequisite for reopening this.

---

## Part 6 — Document extraction (role C1) · `--bench-extraction`

Role C is three jobs and only one of them has an oracle. **C1** reads a transaction statement,
and a transaction statement can be checked: the corpus already contains, for the same account
and the same month, a CSV export of exactly those transactions. `agent/src/extraction_bench.rs`
holds the instrument; `--bench-extraction` runs it.

### The corpus labels itself

⛔ There is **no hand-labelled gold set** and there will not be one. Instead
`omni_me_core::statement::parse` reads the CSV twin and its output *is* the label set. The
labels are therefore machine-generated, free, and regenerate whenever the parser improves.

What that buys, measured on the real corpus:

| | |
|---|---|
| month directories holding both a CSV and at least one PDF | **233** |
| transactions the parser labelled across them | **9,481** |
| directories that failed to parse | **0** |
| directories the parser flagged as partially read | **0** |
| usable at the 40-row ceiling (see below) | **140**, over 157 PDFs |

The corpus is not in the repo and never will be. `OMNI_BENCH_CORPUS` points at it and the
account directories are **discovered, never listed**, so no institution name is written into
a source file.

### Two difficulty tiers over identical truth

The same month is usually stated by two different PDFs, and they are not equally hard:

- the **brokerage-format** PDF uses the CSV's own vocabulary and splits debit and credit into
  separate columns, so a model that reads the table structure gets the sign for free;
- the **cash-format** PDF uses prose merchant names where the CSV says `Withdrawal`, and
  writes the minus sign as an en dash.

Both are scored against the one label set. A model that reads the first and not the second has
a table-parsing ability rather than a document-reading one, and the split makes that visible
instead of averaging it away.

### Amounts are the key, and that is a constraint not a preference

Scoring matches **amounts as a multiset**, not descriptions and not dates.

Descriptions cannot be the key because the two PDFs of one period describe the same
transaction differently — one names the merchant, the other names the transaction type — while
both state the same figure. Dates cannot be compared per row **at all**: `ExtractionResult`
carries one date for the whole document, not one per posting. A multiset rather than a set,
because two coffees at the same price on the same statement are two transactions, and a set
would score a model that found one of them as having found both.

Four columns per document, and the separation is the point:

- **MATCH** — labelled amounts the model also returned. Recall.
- **SIGN** — matches found only after ignoring the sign. A systematic sign flip is a prompt or
  column-mapping defect; a misread figure is a vision defect. Collapsing them hides which one
  you have, and they have different fixes.
- **FAB** — returned amounts with no labelled twin. Fabrication.
- **LATENCY** — per document, because a statement is a large prompt and C1 is a batch role
  whose cost is real.

### The abstention arm

A period with no transactions measures one thing only: whether the model invents rows when the
honest answer is none. Recall is undefined there, so those cases are reported on their own line
rather than folded into the mean.

The corpus has **no** empty months, so the arm is fed instead by `OMNI_BENCH_ABSENT` — a
directory of documents that are not transaction statements. It is **named rather than
discovered**, because the directories holding such documents are named after the institutions
that issued them. A run without it prints that the arm was **skipped**, rather than reporting a
number that quietly measured only the positive half.

⛔ This matches the standing rule from role A and role D: **abstention is weighted above field
accuracy.** A model that reads statements well and hallucinates on everything else is not
deployable in a role that is handed whatever the user uploads.

### Three traps this instrument had to be built around

**`NullExtractor` answers `Ok` with an empty draft.** An unconfigured or refused extractor
therefore looks exactly like a model that found nothing — a 0% recall scorecard that is
measuring the config. The bench checks `name()` and refuses to score rather than print that
number. A consequence worth knowing: `--bench-extraction` with no credentials is a **zero-token
dry run** that reports the corpus statistics and the sampling plan, which is how the table
above was produced without spending anything.

**The extraction request sets no `max_tokens`**, so the ceiling is whatever the provider
defaults to. A statement long enough to be truncated scores as recall failure by the model —
which is the Stage 1 mistake in a new costume, an instrument measuring the harness. `MAX_ROWS =
40` excludes the 93 long directories; the excluded count is printed so the exclusion is a
stated limit rather than a silent one.

**A scorecard gets pasted into this file, which is public.** Every printed identifier is an
FNV-1a tag of the directory name, four hex digits, deterministic across runs so two scorecards
compare. No institution name, account number, filename or path is ever printed.

### The arithmetic arm — receipts and paystubs, where no CSV twin exists

A receipt has no machine-readable counterpart, so the parser-as-oracle trick does not reach
it. Two weaker oracles stand in, and the honest framing is that **neither is a gold set** —
they are checks a wrong answer usually fails, not checks a right answer must pass.

**Oracle 1 — self-consistency, via the product's own verifier.** `extraction::verify` already
cross-checks `sum(|line items|)` against the document's stated `total`, and the confirm-draft
screen already routes a failure into the manual-review lane. The arm calls that same function
rather than reimplementing the rule, so the number it reports *is* the operator's future
workload: `REVIEW` counts the documents a human would have to open.

⚠️ The predicate for "did the arithmetic complain" is a **substring of another module's
warning text**, which is a coupling rather than an interface. A test builds a genuinely
mismatched extraction, runs the real `verify`, and asserts the substring still fires — so a
reword fails loudly instead of the arm reporting every document as sound forever.

**Oracle 2 — text-layer grounding, which does have ground truth.** All 96 paystubs in the
corpus are born-digital, carrying roughly 750 characters and 170 digits of real text layer
each. So every figure the model returns can be checked for *existence in the source*: an
amount appearing nowhere in the document was invented. Poppler is already a hard dependency,
so this costs nothing.

The figure set is built by scanning for numeric tokens with separators stripped and the sign
discarded — a deduction's sign lives in the column heading, not the digits. It **over-collects
on purpose**: years, hour counts and employee numbers land in the set beside money. That makes
grounding a **floor on fabrication rather than a ceiling**, and the floor is the safe
direction, because disproving a figure wrongly called invented would mean opening the real
document.

| column | meaning |
|---|---|
| POST | postings returned |
| TOTAL | did the model return the reference total its hint asks for |
| ARITH | line items sum to that total, within `verify`'s one-cent tolerance |
| UNGRND | figures appearing nowhere in the source text; `n/a` with no text layer |
| REVIEW | what the confirm-draft screen would do — `FLAG` or `auto` |

Photographs are the other half: `~/omni-spike-images/`, real phone captures from the POC.
They have no text layer, so UNGRND reads `n/a` and self-consistency is all they carry. Both
directories are **named by env var, never discovered**, for the same reason the abstention
directory is.

### A product finding from the photographs: a document captured twice cannot be read whole

`DocumentExtractor::extract` takes **one** `&[u8]` and one MIME type. `Payload::Images` is a
vector and the request builder handles N images correctly — but the only thing that ever
produces more than one is `rasterize_pdf`. So a two-page document photographed as two JPEGs
has no path through the extractor: each page becomes an independent request seeing half the
figures, and its line items then fail to sum to a total printed on the other page.

This is not hypothetical. **Four of the eight real POC photographs are pages of two-page
documents**, and phone capture is the flow images exist for. The bench excludes them and says
so in its plan line — scoring them would report a harness limitation as a model error, which
is the Stage 1 mistake again. Fixing it is a trait-signature change touching every
implementation and caller, so it is a design call rather than a repair.
### What it cannot see

- **Per-row correctness beyond the figure.** Right amount with the wrong merchant scores as a
  match. The schema has no per-posting date to check and the descriptions genuinely differ
  between the two PDFs of one period.
- **The long half of the corpus.** 93 of 233 directories exceed 40 rows, and long statements
  are plausibly where the interesting failures are.
- **Categorisation**, which the parser does not label either.
- **Compensating errors on the arithmetic arm.** Two line items misread in opposite directions
  still sum to the stated total, and both figures may exist elsewhere on the page. Passing
  ARITH and UNGRND together is evidence, not proof.
- **Whether a grounded figure was the *right* figure.** Containment proves an amount appears in
  the document, never that it belongs in the field the model put it in.
- **C2 and C3.** C2 has no oracle and shares role D's scorecard. C3 has no producer at all.

### Two more product findings, from the statement arm

Both are design calls rather than repairs, and neither was changed:

1. **`statement_extraction_to_drafts` applies one date to every draft.** `ExtractedPosting`
   carries no date, so the statement's *closing* date lands on all N transactions, and
   `fallback_date()` is `Utc::now()`. The prompt asks for exactly this, so it is deliberate —
   but a PDF-only statement therefore imports as N transactions on one day. Fixing it is a
   schema, prompt, mapper and UI change.
2. **`ExtractionResult.total`'s doc comment claims "statement closing balance"**, a meaning no
   statement prompt asks for. `check_total` verifies an identity against the sum of absolute
   amounts, which would be wrong arithmetic for a closing balance. It is unreachable today
   because the prompt tells the model to leave `total` null for statements, so this is a latent
   trap plus a docs inconsistency, not a live bug.

### Fixed in the same change

`build_extractor` moved from `server/src/lib.rs` to `core::llm::provider`, next to
`build_llm_client`, so no host can disagree with another about which provider a `[llm]` section
selects. The move exposed a gap and closed it: the text path had refused closed-weight models
since role wiring landed and **the document path had not** — although a statement is the most
identifying payload this system sends, carrying a name, an address and an account number.
`allow_closed_weights` remains the documented escape and is tested.
