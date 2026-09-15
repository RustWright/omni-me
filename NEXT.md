# NEXT

**Next action: `tasks.md` § ▶ RUNBOOK Stage 3 — write thresholds per seat, THEN run the slates.**
⛔ **Stages 0, 1, C1 and role D's scorecard are BUILT** — don't redesign them. Design lives in
`docs/src/isolation.md` and `MODEL_BENCH.md` Parts 4, 6 and 7.

## 🔴 Owed before anything decides a seat
- **Nothing has run against a real endpoint. Zero tokens spent.** C1 and D are dry-run-verified.
- **Thresholds and tie-break order, per seat, written into the repo BEFORE the first run.**
- **RE-RUN Role A's bench — UNBLOCKED.** Needs a seeded local hub and spends tokens.
- **`omni_require_dev_volume`'s docker read is unproven** — ⛔ prove before Stage 4; recipe in `deploy/smoke-guard.sh`'s header.

## ⛔ Decisions in force — inherit, do not re-open
- ✅ **NEW: C2 and C3 are OUT of model selection** (2026-09-15). C2's `derive_fields` has **no
  caller**; Part 2's *unbenched until something calls them* already decided it, and the rule went
  unapplied because role C was split **by oracle** and callers were never re-crossed with that
  split. ⚠️ C2 is deferred behind a scheduler, **not** blocked like C3 (which has no producer at
  all). [[feedback-deferred-is-not-cancelled]] · [[project-role-c-split-never-recrossed-callers]].
- ✅ **NEW: role D is scored lexicographically** — fabrications, then recall, then agreement.
  ⛔ Not a weighted sum: any weight meaning "abstention dominates" behaves the same, and a number
  nobody can justify gets tuned until the preferred model wins.
- ✅ **NEW: three role-D prompt findings RECORDED, not fixed** (Part 7) — no entry date is ever
  passed for relative dates · no sentence permits an empty category · no untrusted-input warning.
  ⛔ Fixing first would bench a prompt written to pass the bench.
- ✅ **Both envelopes admit absence, checked 2026-09-15 before any run.** ⛔ Abstention outranks
  field accuracy. ⚠️ `"nullable": true` (6 sites, 2 schemas) is OpenAPI dialect, inert only while
  `strict: false` — it forces fabrication the day strict is switched on.
- ✅ **Two C1 product findings stay RECORDED, not fixed** (Part 6). ⛔ Design calls, his to make.
- ✅ **"No real data" was always a WRITE rule.** ⛔ Intact. The box and his phone are outside the
  grant, always. ⛔ No deploy, no SSH, no `reset-db.sh` against anything live.
- ✅ **STOP AND WAIT on a significant failure**, and write down what for and what came next.
- ⛔ **NO HAND-LABELLED GOLD SET.** C1 arithmetic-checkable · D truth-by-construction probes.
- ⛔ **No seat decided on a saturated instrument**, nor ever on preference. ⚠️ OpenRouter credits
  are a GOAL — screen generously, deciding run goes direct. ⛔ Re-resolve every model id;
  `-Ultra` is 403 on this account. ⚠️ Vision parity is still unproven.
- ⛔ **Do not re-survey, ALL CLOSED:** catalogue retrieval · blob store + LRU · archive page ·
  `imap_real.rs` · Phases D–G · finance propose · notify-and-route · role wiring · isolation
  guard · bench de-saturation · the 413 hazard · multi-part documents. ⛔ **No `HANDOVER.md`.**
- 🔴 **`cargo test --workspace` FILLS THE DISK** — `omni-me-app`'s test binary links the whole
  Dioxus frontend. ⛔ Scope runs to changed packages. [[project-dev-machines-and-build-limits]].
