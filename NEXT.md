# NEXT

**Next action: the notes fork — ⚠️ ask before building**, both options are written out in
`tasks.md`. Behind it: batched `routine.complete` (agreed, not started — now a *card* argument,
not a budget one), then ⚠️ **extraction's two gaps — no downscaling, no rasterization, so a
phone photo 413s today** — before pointing it at DeepInfra. ⚠️ A **dev sync server** is required
before any real-device test. What is waiting on hardware is in `tasks.md` § Awaiting on-device
confirmation; smaller items in § Carried backlog. ⛔ No code-review debt.

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
- ⛔ **Evidence is an overwrite, not a refusal** — `answer::proposals` inserting the
  trace-derived list is the only thing stopping the model citing itself. Beliefs stay
  **ask-only**, confidence three words; ⚠️ the two wider triggers were wanted *with* Phase F
  and are **deferred, not rejected**.
- ⛔ **The turn budget is the loop's only halting guarantee** — no terminal verb, so a model
  that never emits prose never stops. Tunable three ways but ⛔ **never uncappable**:
  `int_range` clamps in `validate` *and* in `int_of`, because validation never runs over a
  value an older build already stored. ⚠️ `propose` is exempt but bounded.
- ⛔ **Irreversible actions can never be granted autonomy**, refused at the write site. ⛔ **The
  user grants; the assistant has no route** — no verb, no action. ⚠️ A granted action still
  produces a real proposal, auto-approved: the proposal *is* the audit trail. ⚠️
  `routine.complete` **is** grantable on purpose.
- ⛔ **Box deploy DEFERRED** until a new app release · **`--ask`/`--bench` STAY** · **Role A
  DECIDED** (`MODEL_BENCH.md`); do not "optimize" the verb loop for latency.

## Do not re-survey
Retrieval is **closed** — semantic ranks, keyword fills the gap; never re-wire RRF, stopwords
in Rust, reranking off. ⛔ Do not re-survey KNN/match grammar, analyzer filters, fastembed,
box sizing. ⛔ Do not re-verify Phases D–G. ⛔ **Extraction surveyed 2026-09-11** — findings in
`tasks.md`; don't re-read the module to answer "what exists".
