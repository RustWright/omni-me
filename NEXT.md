# NEXT

**Next action: roadmap step 6 — email ingest prep**, in a *fresh* context. Prep every email
inbox so the app ingests it cleanly, fixing bugs and adding features ad hoc as they surface.
Sequence in `tasks.md` § AGREED RELEASE ROADMAP + memory `project-v1-release-roadmap`; steps
7–9 are import catch-up → box wipe → Phase 6 OTA, each its own push.

## Decisions in force — inherit these, don't re-derive

- **The pre-v1 code review is CLOSED** (2026-08-28, all four phases). It was the v1 gate; it
  is not reopened by finding a new bug. File new bugs as ordinary work.
- **One roadmap push per fresh context.** The user compacts between steps deliberately. Do the
  current step and stop — do not run ahead into the next.
- **`reviews/` is gitignored on purpose.** `project.md`'s session-log rows are the durable
  record — its top three rows carry Phases C, B and D in full, for when the docs are gone.
- **Two security fixes are code-only until the next image build + deploy** (Gemini header,
  `poppler-utils`). Deliberate: they ride the deployment that accompanies the DB reset,
  roadmap steps 8–9. Not an oversight, don't re-fix.
- **Concepts post S2 ("what an ABI is") is held, not scheduled.** Trip-wire: a *second*
  decision turning on ABI stability. Zero concepts posts is a normal cycle.
- **`wireless-app-updates` logbook entry is blocked on the OTA device round-trip** (step 9).
  The other three entries are written and live in `mylearnbase` as drafts; the user handles
  reviewing and publishing them there, not in this repo.
- **`budget.rs` must be covered before it is split** — refactoring untested code at a release
  gate injects bugs into the thing the gate catches. Mutating path tested; read-side is the
  trip-wire.

## Do NOT re-survey

- The four Phase A perspective documents and their triage. Every finding is dispositioned with
  a dated marker and every deferral carries a trip-wire. Re-reading them is the expensive
  non-answer.
- Formatting. Both workspaces are rustfmt-clean and CI now enforces it.
- `project.md`'s Cycle 4 backlog line and the Phase 3 Known Gaps list — both swept 2026-08-28
  for items that had shipped but were still listed open. They are current as of that sweep.

## Open threads

- Known flake, documented not chased: a SurrealDB tempfile race in the suite — passes on
  rerun, never once a product defect. `ui-checklist.md` is stale (describes a deleted nav).
