# NEXT

**Next action: the IMAP crate swap** (`tasks.md` § IMAP crate swap) — ⛔ the tree's only dated
external deadline: `imap-proto 0.10.2` trips rust#79813 and CI runs `stable` unpinned, so it breaks
on a **Rust release**, not on anyone's commit. `imap` v2.4.1 → **`async-imap` 0.11.3**
(`--no-default-features --features runtime-tokio` ⇒ imap-proto 0.16.7) + `tokio-rustls`; contained to
`auto_import/imap_real.rs` behind the one-method `ImapFetcher`. Then the **note mid-text-editing
design call**, then the **finance / inbox / document-archive planning session** (⚠️ its own session).

## Decisions in force — inherit, do not re-derive
- ⛔ **THE JOURNAL IS NEVER PROPOSABLE** (user, 2026-09-10) — sole author, **readable** and citable;
  ⚠️ it does not lapse because machinery supports it. ⛔ **APPROVAL LIVES WHERE THE DATA LIVES**: never
  route extracted drafts or auto-import batches to the assistant inbox — the **review UI** is the value.
- ⛔ **`propose` holds no writer** — it validates and returns; the agent records proposals from the run's
  **trace**, ⛔ never an `EventWriter` in `dispatch`. ⛔ **Evidence is an overwrite, not a refusal**:
  `answer::proposals` inserting that list is all that stops the model citing itself. Beliefs **ask-only**.
- ⛔ **The turn budget is the loop's only halting guarantee** (no terminal verb); ⛔ **never uncappable** —
  `int_range` clamps in `validate` *and* `int_of`. ⛔ **Fan-out stops at `build_events`**: ⛔ never a plural
  payload key, ⚠️ no partial approval.
- ⛔ **Notes append, never rewrite**; `note.update` **rejected**. ⚠️ Its fold is the one non-idempotent
  projection arm — `applied_appends` guards it, ⛔ never a timestamp. ⛔ **Irreversible actions can never
  be granted autonomy** (refused at the write site); ⛔ the user grants, the assistant has no route.
- ⛔ **Extraction: text first, rasterize only on empty** · ⛔ **over 8 pages refused, never truncated** ·
  ⚠️ budget on **base64 length** · ⛔ in the **extractor, not the route** · ⛔ **`json_schema`, never
  `json_object`** — under the latter a model put "HAND WASH" in `commodity` with arithmetic perfect, so
  `verify` passed it at 1.0. ⚠️ **Every guard inspects numbers; none inspects whether a field means what it claims.**
- ⛔ **One model per job is expressible in config**: `[llm.extractor]`/`.batch`/`.interactive`/
  `.structurer` override `[llm]` **field by field**; ⛔ role E absent (fastembed is not an endpoint).
  ⛔ **Role C selection DEFERRED to the end of this block** (user, 2026-09-11) — sequenced, not descoped:
  a fair test needs a stable substrate. Live runs only show the pipeline works; ⛔ never quote as a ranking.
- 🔴 **A POC finding is NOT a shipped fix — bitten twice** (downscaling, `total`; the latter was even
  written up as "a verified one-line change"). ⚠️ Read `.archive/poc/llm-quality-spike/` before trusting
  any comparison to it — prompt, `response_format` and image handling all differ from the shipped path.
- ⛔ **Box deploy DEFERRED** · **`--ask`/`--bench` STAY** · **Role A DECIDED** (`MODEL_BENCH.md`) ·
  🔴 **the 5G `--scope` recipe once here CAUSED 3 failed runs** (7G box; oomd kills the *terminal*) —
  detached **service** + `JOBS=1` + `DEV_DEBUG=0`, exact form in memory `project-dev-machines-and-build-limits`;
  ⚠️ logs OUTSIDE the repo — SessionEnd pushes untracked files to a **PUBLIC** remote.
- ⛔ **Never hardcode a catalogue-derived count** — derive from `catalog::visible`; `belief` landing made `vector_store`'s `1` stale, and it reads as a regression.
- ⛔ **Do not re-survey:** retrieval **closed** (RRF, stopwords, reranking) · KNN/match grammar ·
  analyzer filters · fastembed · box sizing · Phases D–G · `extraction/media.rs`, live-confirmed.
