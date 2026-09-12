# NEXT

**Next action: a FRESH PLANNING SESSION for note mid-text editing** (user, 2026-09-11 — ⛔ plan it,
don't open by implementing), then **finance / inbox / document-archive** (⚠️ also its own session).
⛔ **IMAP swap DONE** (rust#79813 cleared, 1094 green) — ⚠️ **no live fetch since**. 🔴 **`omni-me-private`
holds UNCOMMITTED work the session hooks do NOT reach** — `Cargo.lock` + `Dockerfile` + 2 examples.

## Decisions in force — inherit, do not re-derive
- ⛔ **THE JOURNAL IS NEVER PROPOSABLE** (user, 2026-09-10) — sole author, **readable** and citable;
  ⚠️ it does not lapse because machinery supports it. ⛔ **APPROVAL LIVES WHERE THE DATA LIVES**: never
  route extracted drafts or auto-import batches to the assistant inbox — the **review UI** is the value.
- ⛔ **`propose` holds no writer** — it validates and returns; the agent records proposals from the run's
  **trace**, ⛔ never an `EventWriter` in `dispatch`. ⛔ **Evidence is an overwrite, not a refusal**:
  `answer::proposals` inserting that list is all that stops the model citing itself. Beliefs **ask-only**.
- ⛔ **The turn budget is the loop's only halting guarantee** (no terminal verb); ⛔ **never uncappable** —
  `int_range` clamps in `validate` *and* `int_of`. ⛔ **Fan-out stops at `build_events`**: ⛔ no plural key.
- ⛔ **Notes append, never rewrite**; `note.update` **rejected**. ⚠️ Its fold is the one non-idempotent
  projection arm — `applied_appends` guards it, ⛔ never a timestamp. ⛔ **Irreversible actions can never be
  granted autonomy** (refused at the write site); ⛔ the user grants, the assistant has no route to.
- ⛔ **Extraction: text first, rasterize only on empty** · ⛔ **over 8 pages refused, never truncated** ·
  ⚠️ budget on **base64 length** · ⛔ in the **extractor, not the route** · ⛔ **`json_schema`, never
  `json_object`** — under the latter a model put "HAND WASH" in `commodity` with arithmetic perfect, so
  `verify` passed it at 1.0. ⚠️ **Every guard inspects numbers; none inspects whether a field means what it claims.**
- ⛔ **One model per job is expressible in config**: `[llm.extractor]`/`.batch`/`.interactive`/`.structurer`
  override `[llm]` **field by field**; ⛔ no role E. ⛔ **Role C selection DEFERRED to the end of this block**
  (user, 2026-09-11) — sequenced, not descoped: a fair test needs a stable substrate, and live runs only show
  the pipeline works. ⛔ Never quote one as a ranking.
- 🔴 **A POC finding is NOT a shipped fix — bitten twice** (downscaling, `total`, the latter written up as
  "a verified one-line change"). ⚠️ In `.archive/poc/llm-quality-spike/` the prompt, `response_format` and
  image handling all differ from the shipped path — read it before trusting any comparison to it.
- ⛔ **The workspace is rustls-only — keep it so.** `ClientConfig::builder()` **panics** here (two
  providers compiled in); name `ring` explicitly. ⚠️ `cargo tree --workspace` unions features, so **one
  dev-dep missing `default-features = false` re-added openssl**. Re-derive, never assume.
- ⛔ **Box deploy DEFERRED** · **`--ask`/`--bench` STAY** · **Role A DECIDED** (`MODEL_BENCH.md`) · 🔴 **the
  5G `--scope` recipe once here CAUSED 3 failed runs** (7G box) — detached **service**, `JOBS=1`,
  `DEV_DEBUG=0`, ⛔ + `source scripts/fetch-onnxruntime.sh`, which cost another run (⚠️ **clippy never
  links**, so it passes a tree whose tests cannot build). Exact form: memory `project-dev-machines-and-build-limits`.
  ⚠️ logs OUTSIDE the repo — SessionEnd pushes untracked files to a **PUBLIC** remote.
- ⛔ **Never hardcode a catalogue-derived count** — derive from `catalog::visible`. ⛔ **Do not re-survey:**
  retrieval · KNN/match grammar · analyzer filters · fastembed · box sizing · Phases D–G · `extraction/media.rs` · `auto_import/imap_real.rs` — all closed.
