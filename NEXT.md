# NEXT

**Next action: make the routine-rollup rule undriftable.** `core::routines::roll_up` and the
screen's inline derivation (`pages/routines.rs` ~line 349) encode one rule twice and agree today.
Decision (user, 2026-09-08): **do not wire them** — the async round-trip is not worth it — so a
test must carry the guarantee. Extract the screen's derivation into a pure fn (inline in rsx,
untestable now), add ONE shared JSON fixture of cases, have a test each side read it and assert
the same result. Copy `token_regex_in_editor_js_has_not_drifted` in `pages/journal.rs`: it reads
the *other* artifact and pins it; its doc says fix the code first, then the pin. Frontend tests
run natively (`cargo test --manifest-path tauri-app/frontend/Cargo.toml`, 115 passing). Also fix
`roll_up`'s doc: it claims to be the single reader, and is not. ⚠️ Say plainly in the new doc that
a fixture guarantees agreement on covered cases, not on all inputs.

**Then close Phase B:** measure the constraint tax on OpenRouter. Hetzner cannot serve both halves
of `--bench` (8/10 free-form vs 4/10 constrained, on already-suspect constrained decoding) —
invocation and env vars are in `agent/src/bench.rs`.

## Decisions in force — inherit, do not re-derive
- **Frontend↔core share NO Rust code, by design.** Root `Cargo.toml` excludes the frontend; all
  crossing is JSON over Tauri IPC, `types.rs` hand-mirrors ~70 structs, `src-tauri` is the bridge.
- **The read catalog is NOT `RecordType`.** That is a *document form* schema (generalization
  decisions 2 + 9) describing frontmatter keys and cannot express a routine. The assistant reads
  `core/src/assistant/catalog.rs` — **code, not events**; `describe_type` merges live RecordType.
- **The verb set is flexible, currently five** (user, 2026-09-08). Load-bearing is *why* it stays
  small — accuracy degrades past ~15–20 tools — not the number. Propose changes, with cause.
- **A skipped routine item counts as done** (user, 2026-09-08). ⚠️ `roll_up` owns that rule for
  **the assistant only**; the screen re-derives it — see the next action.
- **Skip asks for a reason via preset chips + free text** (user, 2026-09-08). No reasonless-skip
  escape. Presets are presentation only — free text is stored, so vocabulary needs no migration.
- **SKIP is visible on mobile, hover-gated only at md+** (user, 2026-09-08). Touch has no hover.
- **`--ask` / `--bench` are test scaffolding** (`--probe` is permanent); model choice stays
  **DEFERRED to Phase D**. ⛔ **FINANCES DEFERRED INDEFINITELY** — its catalog entry is a fixture.
- **Do not re-survey:** generalization · the model bake-off · hosting · the Phase A checks ·
  **whether SurrealDB FULLTEXT works on SurrealKV — proven, and now load-bearing.**

## Open threads
**A rule for reaching to widen a mechanism whose original purpose was never read** — to design
later (user, 2026-09-08): the tell is *permanent + unversioned* **plus** *intent unverified*, not
"built for something else"; [[feedback_prefer_integration_over_rewrite]] points the wrong way.
· Routine search matches group names, not item names · Gemini has no `chat` impl.
