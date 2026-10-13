# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — stage 3, Phase C: triage the review docs with him
Phase A+B done 2026-10-12: `reviews/2026-10-12-{logic,security,performance,bloat,test-gaps}.md`.
⚠️ `reviews/` is gitignored and lives ONLY on the WSL laptop (his choice): triage there.
One doc at a time in his order, one finding at a time: fix / defer / accept, written into the
doc's `Disposition:` line. Fixes land as small commits per finding. ⛔ Do not re-review: the docs
are the record. Items marked "per reviewer" get re-read at the code before deciding.
Then stage 4 (merge), then the go-live sequence.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Dev actions need no permission; live ones do.

## ⛔ Waiting on him
- **Triage**: security doc done (all fixed or accepted); logic next, then bloat, performance.
  Explain each finding in plain consequences, never by ID alone: he could not follow the shorthand.
- **Rulings inside the docs**: L-W2 (does the capture form count as review), L-W6 (vendor-scoped order keys).
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
