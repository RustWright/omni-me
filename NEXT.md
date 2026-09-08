# NEXT

**Next action: intelligence Phase A.** Phase 0 CLOSED 2026-09-07 (contract + both spikes done;
harness archived to `.archive/poc/llm-quality-spike/`). Phase A is two things, in order:
**(1) move `guard_event_type` into `core`** — it is private to the Tauri crate and takes
`&AppState`, so it needs re-seating on `ResolvedConfig`; it is the blocker for any write path.
**(2) the headless-device skeleton** — own binary, own device id + config, syncs and backfills,
runs a chosen projection set (**not** `JournalFile`). **No LLM in Phase A.** Verify it backfills,
projects, and can append an event that reaches the phone. Detail in `tasks.md` item 3.

## Decisions in force — inherit, do not re-derive
- **Model choice is DEFERRED to Phase D, deliberately.** Benchmarking stopped because the only
  discriminating tasks failed for lack of an **index**, not model capability — and Phase C's
  retrieval deletes exactly those tasks. Ranking now would optimise for a workload the finished
  system will not have. Selection happens through **shadow mode** against real approved
  proposals, which is the design's own instrument.
- **Development model is PROVISIONAL, never a choice.** Free Hetzner endpoint, or Maverick
  (~8s) when speed helps iteration. Do not let a dev default become the production selection.
- **Goal is best performance at a cost the user can carry.** Never design jobs down to fit a
  small model. The plan's **"27–35B ceiling" and "one resident general model" are both VOID** —
  each came from the rejected owned-hardware branch. Assign a model **per job, statically**,
  benchmarked by us and written into the code, never chosen by a runtime orchestrator.
- **`docs/src/assistant.md` is the contract** — model-neutral, supersedes the plan's prose. It
  defines **reversibility**, **capture trust by author**, and **what leaves your machine**.
- **Privacy:** `omni-me-private/ASSISTANT_EXPOSURE.md` tracks this instance's exposure. Two
  build actions come from it: **strip EXIF deliberately at the boundary** (currently only an
  accident of resizing), and **never log prompt or completion bodies by default**.
- **The pre-commit privacy guard now blocks credential shapes** as well as identity terms.
  Other machines need `omni-me-private/privacy-guard/install.sh` re-run to pick it up.
- **Dictation needs its own ASR track** — no audio on these endpoints.
- ⛔ **FINANCES DEFERRED INDEFINITELY.**

## Do not re-survey
Generalization · the v1.1.x pass · the hosting market survey · RAM sizing · the model bake-off
(results in the archived spike README; **n=1 per task, ranking is NOT trustworthy** — a frontier
model scored 1.00 then 0.33 on a byte-identical prompt).

## Open threads
Box has **no swap, no container memory limits** — fix before a second resident process lands. ·
Server stays **1.1.0**; overlay lock expects 1.1.2. · `GET /feedback` broken (200 text/plain)
[XS]. · `ExtractionResult.total`: confirmed schema+prompt fix (`tasks.md` item 3). · Server does
not enforce `feature.llm`. · Leg **7b** held. · `chunk_for_push` 413 unverified. ·
New-projection history gap: A vs B undecided. · **Tasks/projects gate estimation calibration**
(`tasks.md` item 4). · Curiosities→concepts + memory prune owed.
