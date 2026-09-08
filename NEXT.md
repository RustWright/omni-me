# NEXT

**Next action: close Phase B — measure the constraint tax on OpenRouter.** Hetzner cannot
serve both halves of `--bench` (8/10 free-form vs 4/10 constrained, on already-suspect
constrained decoding), so today's number measures the harness rather than the model — the
three ways that happens are in [[project-assistant-bench-harness-traps]]. Invocation and env
vars are in `agent/src/bench.rs`. ⚠️ Needs an OpenRouter key and spends money: confirm first.

## Decisions in force — inherit, do not re-derive
- **Frontend↔core share NO Rust code, by design.** Root `Cargo.toml` excludes the frontend; all
  crossing is JSON over Tauri IPC, `types.rs` hand-mirrors ~70 structs, `src-tauri` is the bridge.
- **Cross-boundary rules are pinned by a shared fixture, not by wiring.** The day-completion
  rule lives twice on purpose — `core::routines::roll_up` and the frontend's
  `routine_progress::day_progress` — because the async round-trip was rejected (user,
  2026-09-08). `fixtures/routine_day_agreement.json` feeds both the same cases. ⚠️ It pins
  agreement on the **covered cases only**, and nothing forces the screen to keep *calling*
  `day_progress`. Editing an expectation there is a product decision, never a tidy-up. The
  older twin of this pattern is `token_regex_in_editor_js_has_not_drifted` in `pages/journal.rs`.
- **A skipped routine item counts as done** (user, 2026-09-08) — now enforced on both sides.
  Skips are reported separately by core; the screen shows `done/total` and computes no
  aggregate skip count (user, 2026-09-08), which is why the fixture's `skipped` is core-only.
- **The read catalog is NOT `RecordType`.** That is a *document form* schema (generalization
  decisions 2 + 9) describing frontmatter keys and cannot express a routine. The assistant reads
  `core/src/assistant/catalog.rs` — **code, not events**; `describe_type` merges live RecordType.
- **The verb set is flexible, currently five** (user, 2026-09-08). Load-bearing is *why* it stays
  small — accuracy degrades past ~15–20 tools — not the number. Propose changes, with cause.
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
