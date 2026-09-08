# NEXT

**Next action: capture a skip reason in the UI.** `pages/routines.rs` calls
`invoke_skip_routine_item(.., None)` — the payload, command and column all support a reason and
the UI never asks, so every skip on record has none. Time-sensitive in a way nothing else here
is: a reason not asked for at skip time cannot be backfilled.

**Then close Phase B:** measure the constraint tax on OpenRouter. Hetzner cannot serve both
halves of `--bench` — 8/10 free-form against 4/10 constrained there, on a stack whose
constrained decoding was already suspect. Invocation and env vars are in `agent/src/bench.rs`.

## Decisions in force — inherit, do not re-derive
- **The read catalog is NOT `RecordType`.** That is a *document form* schema (generalization
  decisions 2 + 9) describing frontmatter keys, and cannot express a routine. The assistant reads
  `core/src/assistant/catalog.rs` — **code, not events**, since every table it names is made by a
  projection in code; `describe_type` merges in the live `RecordType` where one exists.
- **The verb set is flexible, and is currently five** (user, 2026-09-08). Load-bearing is *why*
  it stays small — accuracy degrades past ~15–20 tools — not the number. `list` was added because
  `search` matches text and "routine" appears nowhere in a routine called "Morning". Propose
  further changes and talk them through; there has to be a good reason.
- **A skipped routine item counts as done** (user, 2026-09-08). `routines::roll_up` owns that
  rule for the screen *and* the assistant; `skipped` rides alongside so the distinction survives.
- **`--ask` / `--bench` are test scaffolding** (`--probe` is permanent); model choice stays
  **DEFERRED to Phase D** — a dev endpoint is not a model choice.
- ⛔ **FINANCES DEFERRED INDEFINITELY.** The transaction catalog entry is a *test fixture*.
- **Do not re-survey:** generalization · the model bake-off · hosting · the Phase A checks ·
  **whether SurrealDB FULLTEXT works on SurrealKV — proven, and now load-bearing.**

## Open threads
**A rule for reaching to widen a mechanism whose original purpose was never read** — to design
later (user, 2026-09-08), with nuance: the tell is *permanent + unversioned* **plus** *intent
unverified*, not "built for something else"; [[feedback_prefer_integration_over_rewrite]] points
the wrong way. · Routine search matches group names, not item names · Gemini has no `chat` impl.
