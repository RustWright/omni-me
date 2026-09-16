# Model selection thresholds — pre-registered

**Status: COMPLETE, 2026-09-15. Every threshold is written and every number is the user's**
(seat A's latency ceiling, C1's recall floor, and seat D's decision to carry no cost gate at all).
⛔ Written before the slates ran, which is the only thing that makes this file worth keeping.

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
| **Hard gates** | **median ≤ 15s** · **worst case ≤ 30s**, both over **successful requests only** · open weights unless `allow_closed_weights` · answers under a constrained request at all (R8: some models do not) |
| **Ranking** | 1. bench score · 2. p50 latency · 3. cost per run |
| **Tie-break** | cost per run, then open weights over permissive-licence |
| **Publish unmeasured if** | two or more candidates tie on score at the top of the range |

⚠️ **Anchors, not proposals.** The incumbent `openai/gpt-oss-120b-Turbo` measured **3.5s p50,
6.3s worst case** and scores **10/14 (71%)** on the de-saturated instrument.
`deepseek/deepseek-v4-flash` is equally accurate, several times cheaper, and roughly **2.5×** the
latency (so ≈8.8s p50). A third endpoint serving the *same weights* measured **12.8s**.

**The ceiling is set at 15s / 30s** (user, 2026-09-15, before any run). It is deliberately
permissive, and the reason is the ranking order rather than the latency itself: p50 outranks cost,
so raising the ceiling never hands the seat to the cheaper model — on a score tie the faster one
still wins. The ceiling therefore decides exactly one thing, **whether latency may override
accuracy**, and 15s answers no. Every measured candidate but one reaches the ranking, and score
does almost all the deciding.

It beat a 6s ceiling, which would have left `gpt-oss-120b-Turbo` as the sole survivor and decided
the seat before the instrument ran — contradicting the line below. What the gate still catches is
what "unusable" actually looks like: `qwen3.6-35b-a3b` at 37.3s p50, 110.3s worst.

⚠️ **The accepted risk is a 12.8s answer to a typed question** if the slower endpoint outscores
the rest. [[feedback-latency-optimize-then-mask]] applies to the winner afterwards — optimise it,
then mask what is left — rather than to the gate.

**The gate reads successful requests only** (user, 2026-09-15, taking the recommendation). The
pre-registration originally left this undefined, which R14 exposed: `bench.rs` pushes every case's
elapsed time into the latency set *including the failures*, and the failures are the slow tail —
so the printed `worst` measured the transport, not the model (`deepseek-v4-flash` printed 65.4s
against a slowest **successful** request of 16.5s).

**Why successful-only, and what it beat.** An errored case **already** costs the model a point in
the score, because it is counted incorrect. Letting it also inflate latency charges one failure
twice, and — as the void A/B run showed — charges the model for a failure that belonged to the
local link. The alternative, counting every case, was rejected on that double-counting rather than
on which model it favours. ⚠️ Error **rate** stays visible as its own column and is read before the
ranking; it is not being hidden, it is being counted once.

⛔ **Settled before the re-run it governs, not after.** The run this would have decided
(2026-09-15 A/B) is void, so no candidate's standing was known to depend on this when it was
chosen. ⛔ Do not revisit it once the re-run's numbers exist.

⚠️ **"p95" was renamed to "worst case", and that is a correction, not a loosening.** At 14 cases
nearest-rank p95 **is** the maximum (`ceil(0.95 × 14) = 14`), so the two were always the same
number — and `bench.rs`'s own comment had said so since it was written. The threshold said p95
because I wrote it without checking against the instrument. ⛔ The hand-computed "p95" column in
`MODEL_BENCH.md` Part 2 was worse than redundant: it took the **second**-slowest run and dressed
it as a percentile. Distinguishing a real p95 needs n ≥ 20 and means little below ~40.

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
| **Hard gates** | accepts images · open weights unless `allow_closed_weights` (⚠️ a statement carries a name, an address and an account number) · **MATCH ≥ 70%** on the statement arm |
| **Ranking** | 1. sign-flips (ascending) · 2. misread figures (ascending) · 3. amount recall · 4. `REVIEW` rate from `verify` · 5. cost per document |
| **Tie-break** | cost per document |
| **Publish unmeasured if** | candidates tie on sign-flips and misreads |

**Why sign-flips rank above misreads.** A misread figure is wrong and looks wrong next to the
document. A sign-flip is *arithmetically plausible* — it balances, it passes `verify`, and it
silently inverts an expense into income. The instrument counts them separately for this reason.

⚠️ The `REVIEW` column is literally the operator's future workload, produced by the product's own
`verify`. It ranks below accuracy because a model that abstains into review is behaving
correctly; it is a cost, not an error.

**The recall gate is 70%** (user, 2026-09-15, before any C1 run). It is a floor, not a quality
bar: the ranking above already decides quality, so the gate's only job is to exclude a model that
cannot read a statement at all. At 70% roughly one row in three still needs completing by hand,
which is what the `REVIEW` queue exists to absorb.

It beat 85%, which would have been a genuine quality bar and carried a high chance of emptying the
slate — dense financial tables read out of rendered images are hard, and no open-weights vision
model has been measured on this corpus. It also beat 50%, which would have let a model be seated
at ~55% recall on the strength of having the fewest sign-flips.

⚠️ **An empty slate here is a result, not a failed run.** "No open-weights vision model reads
statements well enough" is a real finding and the seat publishes unfilled; ⛔ it is never a reason
to lower this number afterwards. Editing it after the run it governs is an amendment and must say
so, with the date and the reason.

## Seat D — note structurer · `--bench-structuring`

Runs on every note, so **cost per call is a real constraint** — and it is judged on abstention.

| | |
|---|---|
| **Hard gates** | supports tool calls at all · open weights unless `allow_closed_weights` |
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

**There is deliberately no cost gate** (user, 2026-09-15, before any run). Cost is already ranking
key 4 and the tie-break, so a ceiling either excludes nothing and is decoration, or excludes
something and duplicates the ranking it sits above. The absolute numbers are what make this safe:
one call is roughly 1.5k prompt and 150 completion tokens, which across the whole slate spans
about $0.0001 to $0.0023 — at 20 notes a day, $0.70 to $17 a year.

⚠️ **The accepted risk is a pathological outlier** — a model many times dearer than the slate
would still reach the ranking and could win on fabrications. Watch the measured token counts, not
the sticker price: hidden reasoning tokens are where this would come from, and they vary by more
than price does (`gpt-oss-120b` spends them, `deepseek-v4-flash` spends **zero**).

## Seat E — local retrieval

**Already decided and measured** by a different instrument (`--bench-retrieval`,
`MODEL_BENCH.md` Part 5). No endpoint, no credentials, no token cost. Nothing in this file
applies to it and it is listed only so its absence is not read as an oversight.

## Seat C2 — document reading

**In this round as of 2026-09-15.** The blocker was never the code: `derive_fields` had no
caller, so Part 2's *unbenched until something calls them* excluded it. The scheduled pass it was
waiting for is built (`docs/src/archive.md`), and `--bench-reading` scores it
(`MODEL_BENCH.md` Part 8).

**Ranked lexicographically, in this order. ⛔ Not a weighted sum.**

1. **Fabrication** — ungrounded field values + invented dates + eager fields on probes that state
   nothing. ⛔ A field value absent from the document violates the prompt's own "copy exactly as
   printed, never include a value the document does not state", so this is the prompt's rule
   measured, not a stricter one.
2. **Recall** — stated values returned, over stated values asked for. Second so a model cannot
   win by answering nothing, exactly as in role D.
3. **Agreement** — runs agreeing on `kind` and the date across repeats.

**Gates, both disqualifying rather than ranked:**

- ⛔ **Obeying the injection probe disqualifies outright.** `document_prompt` promises the model
  treats a document as data; a model steered by the document it is reading fails a different exam
  than the one being marked, and no score redeems it.
- ⚠️ **Read `ERR` before the ranking, every time.** A model that mostly errors has numbers
  describing nothing — the same rule `PROSE` and `errors` carry for role D.

**Noise floor: 6 probes × 3 runs.** ⛔ Refuse to decide inside it. With 18 calls per model, a
one-or-two-fabrication gap is not a difference; declare the seat unmeasured and widen the probe
set rather than breaking the tie.

⚠️ **This instrument is text-only, so it does not measure vision at all.** C2 reads PDFs and scans
in production. ⛔ Do not publish a C2 seat as vision-capable on this evidence — that half is role
C3's bench.

## Seat C3 — transcription · ✅ **DECIDED 2026-09-16: `deepseek-ai/DeepSeek-V4.1-Flash`**

**Decided by the user on the run-off** (14-model slate → 5 finalists → 30 stratified documents →
retry pass; `MODEL_BENCH.md` Part 9). Runner-up **`meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8`**
— equal coverage, **2× faster** (22.1s vs 44.7s median), one point behind on both recall keys.
⚠️ Kept on file deliberately: it is the candidate to re-test first when a faster or newer model
appears, and the latency headroom is where a future transcription budget comes from.

⚠️ **The decision overrode the strict lexicographic order, deliberately and on the record.**
`inclusionAI/Ling-3.0-flash-VL` won the first key — **2 invented figures against 3** — and was set
aside because that margin is **one figure out of 780**, comfortably inside the noise floor this
seat already declares, while its coverage gap is not: it structurally could not read **11 of 30**
documents (37%, matching the corpus's 36.5% over-4-pages). ⛔ The tie-break was **not** preference:
the pre-registration's own gate says *read the error count before the ranking*, and the error count
is the only measurement here that is outside noise.

| on the 19 documents all three read | invented | figures | words | median |
|---|---|---|---|---|
| `Ling-3.0-flash-VL` | **2** | **99.9%** | 99.5% | 100.6s |
| `DeepSeek-V4.1-Flash` | 3 | 99.6% | **99.7%** | 44.7s |
| `Llama-4-Maverick` | 3 | 99.5% | 99.0% | **22.1s** |

| across all 30 | invented | figures | words | refused |
|---|---|---|---|---|
| **`DeepSeek-V4.1-Flash`** | 24 | **98.5%** | **99.5%** | **0** |
| `Llama-4-Maverick` | 27 | 97.8% | 98.4% | **0** |
| `Ling-3.0-flash-VL` | 2 | 40.1% | 50.6% | 11 |

⛔ **A primary/fallback split was evaluated and rejected** (user's proposal, tested on purpose).
Ling is 3.3× cheaper and leads on ≤4 pages by **99.9% vs 99.6% — one figure in 780**. Routing would
buy no measurable quality, save roughly **$1 across the whole archive**, cost a second code path,
run *slower* (Ling is 2.3× DeepSeek's latency), and carry R19's hazard that a structural refusal
must never be written as an empty transcription.

⚠️ **Still true of the winner, and it is the seat's standing limitation:** every case was
born-digital. See the ⛔ paragraph at the end of this section — **do not publish C3 as proven on
scans.**

---

C3 was *blocked*, not deferred: it had an event, an envelope
and a projection, and no producer at all. It has one now
(`core/src/extraction/transcribe.rs` + `document_enrichment::enrich_text_once`), and
`--bench-transcription` scores it against the text layer a born-digital PDF already carries
(`MODEL_BENCH.md` Part 9).

**Ranked lexicographically, in this order. ⛔ Not a weighted sum.**

1. **Invented figures** — figures in the transcription the file does not state.
2. **Figure recall** — figures the file states that came back. ⚠️ A misread figure scores in both
   keys at once, deliberately; a single accuracy number would let the two cancel.
3. **Word recall**, as a set.

**Gate:** ⚠️ read the error count before the ranking. A document that errored is scored by nothing,
and a model erroring on most of the corpus has numbers describing nothing.

**Noise floor: 6 documents by default** (`OMNI_BENCH_TRANSCRIBE_SAMPLE`). ⛔ Refuse to decide
inside it — raise the sample before breaking a close tie, since unlike the probe-based arms this
one's corpus is large and widening it costs only tokens.

🔴 **The deciding limitation, and it must be stated wherever this seat is published: every case is
born-digital, because that is where the free answer key lives. The documents C3 actually exists
for are scans and photographs, and they have no oracle.** This measures reading on the easier half
and infers. ⛔ Do not publish a C3 seat as proven on scans.

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
