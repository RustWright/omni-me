# Model selection thresholds — pre-registered

**Status: DRAFT, 2026-09-15. Structure is mine; the numbers marked ⬜ are the user's call and are
deliberately blank.** ⛔ Nothing here has been run against.

This file is separate from `MODEL_BENCH.md` on purpose. That one grows after every run; this one
is a **pre-registration** — what would make a model win, written before any model is run, so git
history can prove it was not tuned afterwards. ⛔ Editing a threshold after the run it governs is
an amendment: say so in the line, with the date and the reason.

The rule this exists to enforce is already in `tasks.md` § ▶ RUNBOOK Stage 3 and in
`docs/src/assistant.md`: **a seat whose instrument saturated is published unmeasured.** A tie is
never broken by preference. If every candidate clears every gate and they cannot be separated,
the instrument has failed and the seat stays empty until it is fixed.

## How to read a seat

Each seat has three parts, applied in order:

1. **Hard gates.** Fail one and the model is out, however good the rest looks. A gate is a
   property, not a score — it is the thing that makes a model unusable for that job.
2. **Ranking order.** Lexicographic, not weighted. Compare on the first key; only if it ties do
   you look at the second. ⛔ No weighted sums anywhere in this file — any weight big enough to
   express "this dominates" behaves as an ordering anyway, and a number nobody can justify is a
   number that gets tuned until the preferred model wins.
3. **Tie-break.** What decides it when the ranking order runs out.

---

## Seat A — interactive reasoner · `--bench`

Called by a question someone typed. **Latency is the disqualifier**, per the role table.

| | |
|---|---|
| **Hard gates** | ⬜ p50 latency ceiling · ⬜ p95 latency ceiling · open weights unless `allow_closed_weights` · answers under a constrained request at all (R8: some models do not) |
| **Ranking** | 1. bench score · 2. p50 latency · 3. cost per run |
| **Tie-break** | cost per run, then open weights over permissive-licence |
| **Publish unmeasured if** | two or more candidates tie on score at the top of the range |

⚠️ **Anchors, not proposals.** The incumbent `openai/gpt-oss-120b-Turbo` measured **3.5s p50,
6.3s worst case** and scores **10/14 (71%)** on the de-saturated instrument.
`deepseek/deepseek-v4-flash` is equally accurate, several times cheaper, and roughly **2.5×** the
latency (so ≈8.8s p50). A third endpoint serving the *same weights* measured **12.8s**.

🔴 **The p50 ceiling is the whole decision and I will not invent it.** Set it below 8.8s and the
cheap alternative is disqualified on latency; set it above and cost decides the seat. That is a
product judgement about how long a person will sit looking at a spinner, and
[[feedback-latency-optimize-then-mask]] says faster always wins — but "wins by how much, against
several times the price" is not derivable from anything measured.

⛔ The incumbent is **not** grandfathered. It was decided on the saturated instrument; the re-run
can unseat it.

## Seat B — batch reasoner · `--bench`

Called by the scheduled check-in. Same instrument as A, **no latency gate at all** — it can
afford to be slow, which is the entire reason it is a separate seat.

| | |
|---|---|
| **Hard gates** | open weights unless `allow_closed_weights` · answers under a constrained request |
| **Ranking** | 1. bench score · 2. cost per run |
| **Tie-break** | cost per run |
| **Publish unmeasured if** | two or more candidates tie on score at the top |

⚠️ This seat has saturated once already — four candidates scored perfectly. Lever 2 (absent
answers) is what de-saturated A, and B runs the same cases, so it should now separate. If it does
not, the levers to reach for are 3–5 in `MODEL_BENCH.md` Part 4, still unbuilt.

## Seat C1 — document extractor · `--bench-extraction`

Reads statements and receipts. Feeds the ledger, so a wrong figure becomes a wrong balance.

| | |
|---|---|
| **Hard gates** | accepts images · open weights unless `allow_closed_weights` (⚠️ a statement carries a name, an address and an account number) · ⬜ minimum amount-match rate on the statement arm |
| **Ranking** | 1. sign-flips (ascending) · 2. misread figures (ascending) · 3. amount recall · 4. `REVIEW` rate from `verify` · 5. cost per document |
| **Tie-break** | cost per document |
| **Publish unmeasured if** | candidates tie on sign-flips and misreads |

**Why sign-flips rank above misreads.** A misread figure is wrong and looks wrong next to the
document. A sign-flip is *arithmetically plausible* — it balances, it passes `verify`, and it
silently inverts an expense into income. The instrument counts them separately for this reason.

⚠️ The `REVIEW` column is literally the operator's future workload, produced by the product's own
`verify`. It ranks below accuracy because a model that abstains into review is behaving
correctly; it is a cost, not an error.

## Seat D — note structurer · `--bench-structuring`

Runs on every note, so **cost per call is a real constraint** — and it is judged on abstention.

| | |
|---|---|
| **Hard gates** | supports tool calls at all · open weights unless `allow_closed_weights` · ⬜ maximum cost per call |
| **Ranking** | 1. fabrications (ascending) · 2. recall (descending) · 3. agreement (descending) · 4. cost per call |
| **Tie-break** | cost per call |
| **Publish unmeasured if** | candidates tie on fabrications *and* recall |

**Zero fabrications is not expected and must not be a gate.** Making it one would likely empty
the slate and, worse, would reward a model that calls nothing on every probe — which scores
perfectly on abstention and zero on recall. The ordering already handles it: recall is the second
key precisely so a model cannot win by silence.

⚠️ **`PROSE` and `errors` are not ranking keys, they are validity checks.** A model answering in
prose instead of calling tools has failed to use the interface; if that is most of its runs, its
other numbers describe nothing. Read those two columns before the ranking, every time.

## Seat E — local retrieval

**Already decided and measured** by a different instrument (`--bench-retrieval`,
`MODEL_BENCH.md` Part 5). No endpoint, no credentials, no token cost. Nothing in this file
applies to it and it is listed only so its absence is not read as an oversight.

## Seats C2 and C3 — not in this round

⛔ Neither is benched, and it is not a scheduling choice. **C2's `derive_fields` has no caller**
and **C3 has no producer at all.** `MODEL_BENCH.md` Part 7 has the detail and the difference
between the two. ⚠️ C2 is deferred behind a scheduler, not cancelled.

---

## Gates that apply to every seat

- ⛔ **Re-resolve every model id before the run** (R2). Ids are case-sensitive on both halves of
  the slug; prefer dated ids. Probe entitlement first — `-Ultra` is in the public catalogue and
  returns **403** on this account.
- ⛔ **Run the `off_schema` canary on both sides** (R5). Direct, an ignored parameter is silent,
  so a model can appear to honour a constraint it never received.
- ⚠️ **Vision parity is unproven per role.** R1's confirmation covered text and latency only, so
  C1's image path needs its own check before its numbers mean anything.
- ⚠️ **Screening runs on OpenRouter pinned to DeepInfra; the deciding run goes direct.** Screen
  generously — wide slate, repeat runs, more cases. Spending those credits is the goal.
- ⛔ **A cost number that does not reconcile is not a cost number** (R11, still open).
