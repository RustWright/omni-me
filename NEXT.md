# NEXT

**Next action: Phase D — `propose` + the approval inbox.** The first time the assistant
*does* something rather than answering. ⛔ No code-review debt exists.

## Decisions in force — inherit, do not re-derive
- **The ask/answer interface is DONE and verified, 2026-09-10.** Stage 1: a question authored
  on one device is answered by the resident agent and syncs back — **model 6.6s**. Stage 2
  (the tab): ask → thread → answer → citation → follow-up → Back, zero console errors, no
  overflow at 390px. 799 core / 119 frontend tests green, clippy ×4. ⛔ Do not re-verify.
- ⛔ **Deploying the agent to the box is DEFERRED (user, 2026-09-10)** until they are ready
  to release a new app version, which they are not. **Do not propose it.** Phase D was
  unblocked by the ask/answer surface and is what makes the thing useful.
- **Memory is NOT the constraint — measured 2026-09-10: agent peak RSS 278 MiB** with the
  embedder loaded (budget said 0.1–1 GB); + server/SurrealKV 106 MiB = ~384 MiB of a CX22's
  ~3.1 GiB. ⚠️ Debug binary, empty-corpus sweep.
- ⛔ **`--ask` and `--bench` STAY** — the "goes when the chat surface lands" note was wrong,
  now corrected in `ask.rs` + `tasks.md`: the tab prints prose, `--ask` prints the **trace**,
  and `--ask --constrained` is the smoke test `bench-openrouter.sh` branches on.
- ⛔ **Role A DECIDED: `openai/gpt-oss-120b-Turbo`** (`MODEL_BENCH.md` § Role A). Base was
  **13.7× slower**, same model and accuracy. ⛔ Do not "optimize" the verb loop for latency.
- ⚠️ **`dx serve` needs `npm run copy:editor:dev` (from `tauri-app/`) or the app FREEZES** —
  without `editor.bundle.js` the first tab switch calls the undefined `destroyEditor` through
  an uncaught wasm-bindgen import and **aborts the wasm**, which looks exactly like broken nav.
  Tell: "Initializing editor environment...". ⚠️ Playwright's `[active]` is DOM focus, not tab
  state; read `window.__omniCanGoBack`.
- ⚠️ **Every `db.query` needs `.check()?`** (`.await?` catches only transport errors) · ⚠️
  **array-of-objects under SCHEMAFULL: declare the subfields**, not `FLEXIBLE` · ⛔
  **`assistant_messages` stays out of `assistant::catalog`** — Phase E's call.
- **Retrieval is CLOSED**: semantic ranks, keyword supplies what it missed; ⛔ never re-wire
  RRF. **Stopwords in Rust. Reranking OFF.** `CONTAINS` box **SUPERSEDED** — ⛔ never debt.
  ⛔ **Do not re-survey** KNN/match grammar, analyzer filters, fastembed, box sizing.
- ⚠️ **`source scripts/fetch-onnxruntime.sh` from the REPO ROOT** before any linking build
  (clippy never links). `CARGO_BUILD_JOBS=1`; this box OOMs. ⚠️ Two `dx serve` both bind.

## Open threads
Citation chips show the record *kind*; a real title needs a local lookup + a deleted-record
fallback · untested: Shift+Enter, hardware back, "Thinking…", failure-sentence rendering ·
prompt caching unexploited (needs a DeepSeek id Role A declines) · stemming needs a migration
· `server/Dockerfile` has no ONNX · `MEMORY.md` 20KB / 25KB cap.
