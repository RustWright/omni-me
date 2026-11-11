# NEXT

⛔ **Next action, and what is blocked on him. Nothing else.** (His instruction, 2026-09-27.) Decisions,
findings and what-not-to-re-survey live in `tasks.md`; rationale lives in `docs/src/` and at the edit
site. ⛔ If a line here could be derived from either, it does not belong here — point, do not restate.

## ▶ NEXT ACTION — finish F1's sweep, then a release so F2's note can go live
The live agent is healed and sweeping (`tasks.md` § Stage 4, "✅ LIVE HEALED"); F2's review is done
("✅ F2 REVIEW DONE"). Then:
1. F1: `f1_watch.py` reports the sweep done; check journal, notes and documents were all indexed;
   restart dev (stopped for the sweep, ruled).
2. A release (1.2.3) carrying `ff94381` + `0b0bccf`: the note must not reach live before it
   (`tasks.md`, "⛔ Ordering trap"). Then the note goes on live as "Ledger conventions", his go.
3. F3 after: ws-api into `/opt/omni-ws`, live's two `_disabled` bank sections renamed, bank sources
   on, then he turns the tab on.
⚠️ Two pre-November drafts sit in his review queue (`tasks.md`, "✅ E4 DONE"): his to dismiss.
⛔ Live is the pause: every live step waits for his go, one at a time. Production deploys, releases
and writes to live data are his to run (the permission check refuses mine); hand him the command.
⚠️ A fast-forward of public `main` closes the draft PR (now #7); open a new one after each.

⛔ **Never `cargo build`/`test` on this box** — `check`/`fmt`/`clippy` only; anything that links
goes to CI. `project-dev-machines-and-build-limits`. Tooling: `~/omni-dev-tools/devsrv.py`,
`livesrv.py`, `set_config.py`, `refile_live.py`, `f1_watch.py`. Overlay workflows: dispatch from the
overlay branch that pairs with the public ref, and before any overlay build run overlay `ci.yml`
with `-f public_ref=<full public SHA>` (`project-overlay-ci-blind-spot`).

## ⛔ Waiting on him
- **Three live refiles** (MPI 1, convenience 2, tips 2): `refile_live.py`, commands in `tasks.md` F2.
- **The two pre-November drafts** in review: dismiss or keep.
- **The design session's decisions**: `DESIGN_PREREAD.md` § Decisions, Parts 1 and 2 (read first).
- **R2**: his Cloudflare account, a bucket and a scoped API token, for the off-box blob copy.
- **GitHub Support purge** of the pre-S-W1 commits (optional, his ruling): `tasks.md` § release item 4.
- **F3's open half**: whether multi-page reading should be told pages can repeat.
- **Tap targets**: a visual redesign across screens, his call. `tasks.md` § Open.
- **R27's account side**: his to check; ⛔ no list of affected runs exists, by his decision.
- **The public-repo identity scrub**: authorised, end of a session only. `tasks.md` § Owed at cycle close.
