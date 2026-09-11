# NEXT

**Next action: `routine.complete` / `routine.skip`, carrying evidence from the trace** —
agreed 2026-09-11. ⚠️ Then the two extraction gaps (below) **before** pointing it at
DeepInfra. ⛔ No code-review debt.

## Decisions in force — inherit, do not re-derive
- ⛔ **THE JOURNAL IS NEVER PROPOSABLE** (user, 2026-09-10). He is its sole author;
  everything else is "free game to the agent". Fully **readable** and citable. A product
  invariant, not a phase boundary — it does not lapse because the machinery would support it.
- ⛔ **APPROVAL LIVES WHERE THE DATA LIVES** (user, 2026-09-10). Never route extracted drafts
  or auto-import batches into the assistant inbox: the two share an event shape, but the
  **review UI** is what varies and it is the whole value. The assistant **notifies and
  deep-links** — a read. Its inbox keeps only proposals with no other home (note, belief).
- ⛔ **`propose` holds no writer.** It validates and returns; the agent records proposals
  afterwards from the run's **trace**. ⛔ Never thread an `EventWriter` into `dispatch`.
- ⛔ **Beliefs are ask-only** (user, 2026-09-10). ⚠️ He wanted the two wider triggers built
  *with* Phase F and I did not — **deferred, not rejected**. ⛔ **Evidence is system-filled
  from the trace**, never offered to the model; confidence stays three words.
- ⚠️ **`routine.complete` is evidence-backed, not an unverifiable assertion** — I argued the
  opposite and was wrong. The journal is readable *precisely* so it can see what he wrote
  about his day. Build it with `EvidenceFromTrace`, like `belief.record`.
- ⛔ **Irreversible actions can never be granted autonomy**, refused at the write site; a
  perfect record does not override it. ⛔ **The user grants; the assistant has no route** —
  no verb, no action. ⚠️ A granted action still produces a real proposal, auto-approved: the
  proposal *is* the audit trail.
- ⛔ **Box deploy DEFERRED** until a new app release · **`--ask`/`--bench` STAY** · **Role A
  DECIDED** (`MODEL_BENCH.md`); do not "optimize" the verb loop for latency.

## Do not re-survey
Retrieval is **closed** — semantic ranks, keyword fills the gap; never re-wire RRF,
stopwords in Rust, reranking off. ⛔ Do not re-survey KNN/match grammar, analyzer filters,
fastembed, box sizing. ⛔ Do not re-verify Phases D–G. ⛔ **Extraction surveyed 2026-09-11**
— findings in `tasks.md`; don't re-read the module to answer "what exists".

## Open threads
⚠️ **Extraction: no downscaling and no rasterization** — a phone photo 413s today; fix both
before switching provider · **dev sync server before any real-device test** (only
`LISTEN_ADDR`, a hardcoded const, is missing) · untested on hardware: the Tauri decide
command, a decision syncing to a second device, a check-in firing, auto-approval · the
dual-model boundary is untyped and `Provenance` does not exist · citation chips show the
*kind*, not a title · prompt caching unexploited · stemming needs a migration.
