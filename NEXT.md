# NEXT

**Next action: generalization Phase A — the config foundation.** Item 2 is **designed, not
built**: 12 decisions settled 2026-09-05, phases in `tasks.md` § Generalization, reasoning in
`~/.claude/plans/lets-continue-deep-blanket.md`. Order A → B → C, each its own session; item 3
(AI/LLM/ML) follows. ⏰ **Phase B (inert feature toggles) is the only deadline-bearing item** —
it lands before the next feature ships, or every feature added this year is a retrofit owed.

## Decisions in force — inherit these
- ⛔ **FINANCES ARE DEFERRED INDEFINITELY.** Don't start, propose, or "just fix" an item. State
  only: tab offline, both bank sources OFF, categorization to `Unmatched`. This also puts
  statement layout strings **out of generalization's scope**, despite item 2 naming them.
- **`docs/src/invariants.md` is the contract** (mdbook → Pages): check the implementation
  **against** it, never back-fit it to whatever got built.
- **Client customization is data; server customization may be code.** From fork cost, not
  taste: the overlay swaps a binary's `main` and builds in CI; a client fork means NDK +
  signing + publishing your own `/updates`. The client has no composition point at all.
- **Declared record types are the target** — identity rule, template, properties, completeness,
  auto-close. Journal is one instance. **No new code may assume one journal type.** First run
  applies the *minimal* preset; the user's three prompts ship as the *reflective* preset.
- **"Off" means inert**: no tab, commands refuse, **projections and schedulers never start**;
  reversible by replay. Features do **not** map 1:1 onto projections (`NotesProjection` serves
  journal *and* notes), so an explicit feature → (tab, projections, schedulers, commands,
  settings) map is required.
- **UI stops at schema-driven forms** — no view DSL, no UI plugins (WASM can't load UI at
  runtime). **Both platform doors held open, neither built:** no new client code spawns
  processes or assumes POSIX; native behaviour is a labelled shim, never a pattern to copy.
- **Config = event-sourced baseline + per-device override; the device wins when set.** Startup
  reads the config projection's **persisted table** (no replay) before registering projections
  ⇒ a toggle set elsewhere applies **at next launch**. Record types have no such lag.

## Do NOT re-survey
Generalization design is **CLOSED** — inherit the decisions, don't reopen. The server is
**already** open-core (overlay = sibling crate with its own `main`; subprocess contract frozen)
and runs **zero** projections (`server/src/lib.rs:207`). Verification PNGs are throwaway.

## Open threads
⚠️ **Disk AND RAM both bind.** Never `cargo test -p A -p B` (~1.4G second artifact set); one
crate at a time, `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0`. · `npm run copy:editor:dev` after
every dx rebuild or CodeMirror 404s. · Curiosities→concepts + memory prune owed.
