# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — stage 4: draft the go-live sequence
Stage 3 closed and S-W1 done 2026-10-14 (`tasks.md` § release, items 3 and 4). ⛔ Do not redo either.
Draft the go-live sequence for him to approve, from his fixed points in `tasks.md` § release item 4
and the "Moved to the go-live sequence" line under stage 2: gate 1 (live backup) first; the tab goes
on only after server AND app are updated; the official import needs the updated live server and a
wipe first; his conventions note comes after the re-import; the agent goes live in it too.
⛔ Live is the pause: drafting needs no permission, every live-affecting step waits for his go.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Before any overlay build, dispatch overlay `ci.yml`
with `-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): they are still served
  by SHA. `tasks.md` § release item 4 has what to name.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
