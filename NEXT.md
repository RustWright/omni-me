# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — close stage 3: dev deploy, then the S9 check
Stage 3 review TRIAGED AND FIXED 2026-10-13: every finding in `reviews/2026-10-12-*.md` carries a
disposition and a hash. ⚠️ `reviews/` is gitignored and lives ONLY on the WSL laptop (his choice).
⛔ Do not re-review. Remaining, in order:
1. Read CI run `38412992265`: its viewport check now fails on ANY app error (T-15). A red there is
   probably a pre-existing console error the old check ignored: read it, fix or report.
2. Merge public into the overlay (`dev/untested-overlay`; the stamp and private lock move together),
   deploy server + agent to dev, build the dev APK, install on the S9.
3. On the S9: Archive › + Photo / + PDF opens the new document (L-W18); a purge shows its report
   until Done (L-W17); Retention's Review list matches its count (L-C1).
Then stage 4 (merge to live). Stage 4 starts with the S-W1 history rewrite (`tasks.md` § release).

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`.
Dev workflows: `--ref dev/untested-overlay`. Dev actions need no permission; live ones do.

## ⛔ Waiting on him
- **C: drive is full** (481 MB free on 2026-10-13). `wsl --shutdown`, then `schtasks /run /tn
  "\WSL compact"` from Windows; check `C:\Users\erhie\wsl-maint\compact.log` says `done`.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **The official finances import**: go-live sequence, after the merge. Gate 1 (live backup) first;
  then his conventions-note rewrite (`tasks.md` § release, stage 4).
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
