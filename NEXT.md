# NEXT

**Next action: `tasks.md` § ▶ RUNBOOK Stage 2 — build the C1 / C2 / D scorecards.**
⛔ **Stages 0 and 1 are BUILT and VALIDATED and the bug Stage 1 found is FIXED** — don't
redesign them. Design: `docs/src/isolation.md`, `MODEL_BENCH.md` Part 4, and
`docs/src/assistant.md` § Proving something is absent.

## 🔴 Two things still owed
- **RE-RUN THE BENCH before it decides any seat.** Control went **10/10 (100%) → 10/14 (71%)**,
  ⛔ but at the OLD turn budget — three of fourteen cases were scoring the budget, not the
  model. ⛔ All the spread came from lever 2; content scoring alone de-saturated nothing.
- **`omni_require_dev_volume`'s docker read** — no docker daemon here and he declined one.
  ⛔ Prove it before Stage 4 leans on it; recipe in `deploy/smoke-guard.sh`'s header. ⚠️ Its failure modes all bias toward over-refusing.

## ⛔ Decisions in force — inherit, do not re-open
- ✅ **NEW: a budget sized on positive lookups cannot answer a negative one.** Confirming a
  record exists costs 2–4 turns; proving one absent costs **8**. Default now 10, and the last
  turn **withholds tools** so running out yields prose, not silence. ⛔ The loop rules already
  said "not found is a correct answer" and were ignored — advice it can act on later is not
  advice it must act on now.
- ✅ **NEW: config/marker disagreement REFUSES TO BOOT** (user, 2026-09-14), symmetric both ways;
  `unknown` is refused by tooling because absence is not permission. ⚠️ The live box has no
  marker until a stamping server ships, so the first reset there needs `OMNI_CONFIRM_PRODUCTION`
  — deploy, then reset. ⛔ **Check every caller before guarding a script:** `restore-snapshot.sh`
  is `remote-deploy.sh`'s auto-rollback, and a naive guard turns a deploy failure into an outage.
- ✅ **STOP AND WAIT on a significant failure**, and ⛔ **write down what for and what the run
  would have done next** — he recalibrates from it. Flaky test = trivial.
- ✅ **"No real data" was always a WRITE rule** — junk lands on the clone. ⛔ Intact, not
  relaxed. ⚠️ **Never widen a stated constraint past its direction — ask which way it points.**
- ⛔ **NO HAND-LABELLED GOLD SET** ("no time"); correction-log labels are **negative-only**.
  ⛔ **Role C is THREE jobs:** C1 arithmetic-checkable · C2 **no oracle** · C3 **no producer**,
  and ⚠️ C2 and D share one scorecard.
- ⛔ **No seat decided on a saturated instrument**, nor ever on preference. Thresholds first;
  14 cases = ~7 points, gaps inside that decide nothing. ⚠️ **OpenRouter credits are a GOAL** —
  screen generously, deciding run direct. ✅ Parity holds **text/latency only**; ⚠️ **vision
  unproven**. ⛔ Re-resolve every id (R2); `-Ultra` = 403 here.
- ⛔ **Do not re-survey, ALL CLOSED:** catalogue retrieval · blob store + LRU · archive page +
  viewer bugs · `imap_real.rs` · Phases D–G · finance propose · archive plan 1–5 · notify-and-
  route · role wiring · **isolation guard + bench de-saturation**. ⛔ **No `HANDOVER.md`.**
- ⛔ **Read [[project-dev-machines-and-build-limits]] BEFORE building** — it held every trap.
