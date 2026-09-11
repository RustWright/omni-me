# NEXT

**Next action: extraction's two gaps — ⚠️ no downscaling and no rasterization, so a phone photo
413s today** — and they land before extraction is pointed at DeepInfra. Findings are already in
`tasks.md`; don't re-read the module to answer "what exists". Behind it: ⚠️ a **dev sync server**
before any real-device test (only `LISTEN_ADDR`, a hardcoded const, is missing). Rest in
`tasks.md` §§ Awaiting on-device confirmation, Carried backlog. ⛔ No code-review debt.

## Decisions in force — inherit, do not re-derive
- ⛔ **THE JOURNAL IS NEVER PROPOSABLE** (user, 2026-09-10). Sole author; everything else is "free
  game to the agent". **Readable** and citable. ⚠️ It does not lapse because machinery supports it.
- ⛔ **APPROVAL LIVES WHERE THE DATA LIVES** (user, 2026-09-10). Never route extracted drafts or
  auto-import batches into the assistant inbox: they share an event shape, but the **review UI**
  varies and is the whole value. Its inbox keeps only proposals with no other home.
- ⛔ **`propose` holds no writer.** It validates and returns; the agent records proposals
  afterwards from the run's **trace**. ⛔ Never thread an `EventWriter` into `dispatch`.
- ⛔ **Evidence is an overwrite, not a refusal** — `answer::proposals` inserting the
  trace-derived list is the only thing stopping the model citing itself. Beliefs stay
  **ask-only**, confidence three words; ⚠️ the two wider triggers are **deferred, not rejected**.
- ⛔ **The turn budget is the loop's only halting guarantee** — no terminal verb, so a model that
  never emits prose never stops. Tunable three ways, ⛔ **never uncappable**: `int_range` clamps
  in `validate` *and* `int_of`. ⚠️ `propose` is exempt but bounded.
- ⛔ **Fan-out stops at `build_events`.** `routine.complete`/`.skip` take `item_ids` but each
  event carries the singular `item_id` the app writes; ⛔ never add a plural payload key. ⚠️ **No
  partial approval** — the chosen trade.
- ⛔ **Notes append, never rewrite** (user, 2026-09-11); `note.update` is **rejected**. ⚠️ The
  append fold is the one non-idempotent projection arm — `applied_appends` guards it and ⛔ a
  timestamp must never replace it. ⚠️ **Text in the middle is a later conversation.**
- ⛔ **Irreversible actions can never be granted autonomy**, refused at the write site. ⛔ **The
  user grants; the assistant has no route.** ⚠️ Granted still means a real, auto-approved
  proposal: it *is* the audit trail. ⚠️ `routine.complete` **is** grantable.
- ⛔ **Box deploy DEFERRED** until a new app release · **`--ask`/`--bench` STAY** · **Role A
  DECIDED** (`MODEL_BENCH.md`); do not "optimize" the verb loop for latency.
- ⚠️ **Every cargo command runs in its own cgroup** or `systemd-oomd` takes the terminal with it:
  `systemd-run --user --scope -p MemoryMax=5G -p MemorySwapMax=1500M env CARGO_BUILD_JOBS=1 cargo …`

## Do not re-survey
Retrieval is **closed** — semantic ranks, keyword fills the gap; never re-wire RRF, stopwords in
Rust, reranking off. ⛔ Do not re-survey KNN/match grammar, analyzer filters, fastembed, box
sizing; do not re-verify Phases D–G.
