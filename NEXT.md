# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — stage 4: merge dev to live
Stage 3 CLOSED 2026-10-14 (`tasks.md` § release, item 3: what is on dev, what the S9 showed).
⛔ Do not re-review or re-check it. Stage 4, in the order his rulings set:
1. S-W1 history rewrite on the public branch, then re-merge the overlay once (`tasks.md` § release,
   item 4; details in `reviews/2026-10-12-security.md`, WSL laptop only). It rewrites published
   history and force-pushes the public repo: confirm with him before running it.
2. Draft the go-live sequence (it is drafted at stage 4, not before). His fixed points: gate 1 (live
   backup) first; the tab goes on only after server AND app are updated; the official import needs
   the updated live server and a wipe first; his conventions note comes after the re-import.
⛔ Live is the pause: every live-affecting step waits for his go.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Before any overlay build, dispatch overlay `ci.yml`
with `-f public_ref=<full public SHA>`: public-only changes broke it twice at stage 3.

## ⛔ Waiting on him
- **S-W1 go-ahead**: the force-push above.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first;
  then his conventions-note rewrite (`tasks.md` § release, stage 4).
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
