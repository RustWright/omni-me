# Assistant architecture: what exists, and what to adopt

Research for ruling R4-3 (2026-10-11): were we re-inventing the wheel, and is there an option
significantly better even at a higher token cost? Scope and test set: `tasks.md` § "Round 4
early feedback". Safety lines are held fixed (journal unwritable, irreversible never grantable,
approval gate, beliefs ask-only).

## What we have, in one paragraph

A hand-written tool loop in Rust (`core/src/assistant/session.rs`): five generic verbs
(`search`, `read`, `list_types`, `describe_type`, `propose`) over a runtime catalogue; a budget
of 10 model calls with tools withheld on the last; earlier turns replayed as prose only;
hybrid keyword + vector retrieval; `propose` validates an intention and the agent turns the
*calls* into proposal events that sync to the phone's inbox; per-action grants. Interactive
model on dev: `deepseek-ai/DeepSeek-V4-Pro` on DeepInfra, open weights only by policy.

## The frameworks, judged against our case

| Option | What it really gives | Fit here |
|---|---|---|
| **LangGraph** (Python) | Explicit state graph, checkpoints, `interrupt` for human approval, tracing. | Its strongest feature, human-in-the-loop, we already have in a stronger form: approval is an event that syncs and can wait days. It would sit in a Python sidecar calling our Rust core over HTTP for every verb and every safety check. Gives structure for planning, which we can write directly. |
| **OpenAI Agents SDK / PydanticAI** (Python) | Typed tools, handoffs, input/output guardrail hooks, tracing. | Thin loops, like ours. Guardrails are a *place* to put a check, not the check. Same sidecar cost. |
| **Letta / MemGPT** | Tiered memory: always-in-prompt "core" blocks the agent edits, recall, archival; "sleep-time" agents consolidating memory while idle. | Core blocks are our beliefs; agent-edited memory and sleep-time consolidation conflict with *beliefs are ask-only*. It is a server that owns the agent's state, so it would duplicate our event log. Worth borrowing one idea: a small always-loaded profile block, written only through approval. |
| **DSPy** | Prompt/program optimisation against a metric and a labelled set. | Not a runtime. Useful later as a tuner once we have a scored test set; worthless before. |
| **Rig** (Rust) | Provider clients, agent with tools, vector-store adapters (incl. SurrealDB). | Would replace code we already have and have hardened (constrained mode, turn budget, off-schema handling). No gain on any failure below. |
| **Goose** (Block, Rust) | A desktop coding agent built on MCP. | Wrong product; the takeaway is MCP (exposing our verbs as an MCP server), which is a distribution choice, not a quality fix. |

Anthropic's own guidance on agents: "start by using LLM APIs directly"; frameworks "obscure
the underlying prompts and responses"; add complexity "only when it demonstrably improves
outcomes". Our loop is that recommendation already. *(Verified from source.)*

## The failures, and what actually fixes each

None of the six failures is a loop-orchestration failure, so none is fixed by changing the
loop's framework. Each maps to a published pattern:

| Failure | Cause | Pattern that fixes it |
|---|---|---|
| R3-7/8 says "done ✅" having done nothing | Prose is unchecked against the calls. | Footer (shipped) makes it visible. Next: a **trace check** before the reply is stored: any claim of a change must match a proposal in this run, else the reply is regenerated with the mismatch named (AgentLTL's grounding-as-trace-constraint). Deterministic check of entities, cheap model check of verbs. |
| R3-9 "most recent" picks 09-02 over 09-25 | `search` ranks by relevance only; the model cannot ask for order. | **Make the tool do the work** (Anthropic, writing tools): `search` gains `sort: recent` and a date range. A tool fix, not a model fix. |
| R3-10 routine needs two turns | Steps need the new routine's id, which exists only after approval. | **Consolidate calls that always go together**: one `routine.create` proposal that carries its steps. Plus a weekday frequency, a domain gap. |
| R3-11 research-then-write ran out of turns, empty reply | 10 calls, all spent searching; last call has no tools, so it could not even propose. | **Plan, then execute** with a budget per phase; or an **orchestrator-worker** split where a research sub-run returns a digest and the writer proposes from it. Also: `propose` stays available on the last call. Token cost rises; allowed. |
| R4-2 transaction ignores ledger conventions | Nothing grounds a write in existing records. | Three layers, cheapest first: (1) **validate** against the ledger: an account, payee or tag the journal does not have is refused with the nearest real ones in the error (hledger's `--strict` idea, plus "actionable errors"); (2) **exemplars**: the propose step is handed the k most similar past transactions (by payee/narration) as the pattern to follow, the approach beancount's `smart_importer` uses with plain TF-IDF; (3) the proposal card shows those exemplars so a mismatch is visible at approval. Same for categorize/tag/update. |
| Belief recency (10-04) | Old evidence weighed like new. | Already handled by the month window; same `sort/date` tool change helps. |

What most changes quality, by the evidence found: tool design and grounding (Anthropic's tool
evals; τ²-bench, which scores exactly "act through APIs under a domain policy"), then the
agent's control pattern (architecture choice moves success by up to ~12 points on one model),
then the model. *(The 12-point figure is from one benchmark paper, not ours.)*

## Recommendation

**Keep the Rust loop; do not rebuild onto a framework.** A rewrite would buy a Python sidecar
and a second state store, and fix none of the six failures; every fix above is code we would
write either way. What beat it: LangGraph in a sidecar, the strongest framework candidate,
rejected because its distinctive feature (approval interrupts) is the one we already have
better, and a plan step is a small addition to our own loop (my estimate, not measured).

Adopt, in this order:

1. **A scored test set first**: the R3/R4 requests as bench cases (we have the bench harness),
   so every change below and any model swap is measured, not argued.
2. **Ledger grounding** (R4-2): validation, then exemplars, then the card.
3. **Tool fixes**: `search` sort/date; composite `routine.create`; `propose` on the last call.
4. **Plan-then-execute** for multi-step requests, with a larger budget (cost allowed).
5. **Trace check** on claims, regenerating once on mismatch.
6. **Then re-run the set against stronger models**, since cost is secondary; open-weights
   policy unchanged unless you rule otherwise.

Sources: [Anthropic, Building effective agents](https://www.anthropic.com/engineering/building-effective-agents);
[Anthropic, Writing tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents);
[Letta memory docs](https://docs.letta.com/guides/agents/memory);
[smart_importer](https://pypi.org/project/smart-importer/);
[τ²-bench](https://github.com/sierra-research/tau2-bench);
[AgentLTL, trace verification](https://arxiv.org/pdf/2607.02599);
[ReAct vs plan-and-execute benchmark](https://github.com/TheDivyanshShukla/plan-once-ground-locally);
[Reason-Plan-ReAct](https://arxiv.org/html/2512.03560v1);
[Framework comparison, Speakeasy](https://www.speakeasy.com/blog/ai-agent-framework-comparison).
