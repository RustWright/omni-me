# Model selection thresholds — pre-registered

**Status: COMPLETE, 2026-09-15. Every threshold is written and every number is the user's**
(seat A's latency ceiling, C1's recall floor, and seat D's decision to carry no cost gate at all).
⛔ Written before the slates ran, which is the only thing that makes this file worth keeping.

**Seats as of 2026-09-16 — five filled, D pending, E not a model choice:**

| seat | model | basis |
|---|---|---|
| **A** interactive | `deepseek/deepseek-v4-pro` @ fp8 | 14/14, 6.8s p50, free-form arm |
| **B** batch | `deepseek/deepseek-v4-pro` @ fp8 | ties at 14/14, wins on cost |
| **C1** extraction | `deepseek-ai/DeepSeek-V4.1-Flash` | 0 flips / 0 misreads, 96.0% |
| **C2** reading | `google/gemma-4-31B-it` | fabrication tied, recall 23/24 |
| **C3** transcription | `deepseek-ai/DeepSeek-V4.1-Flash` | 98.5% figures, 0 refusals |
| **D** structuring | `deepseek/deepseek-v4-flash` @ fp8 | 0 fab, 27/27, 66/66 abstention |
| **E** retrieval | not a model choice | reranker default, already set |

⚠️ **Read before quoting any of these.** 🔴 **R26**: sampling is uncontrolled — `temperature`,
`top_p` and `seed` are set nowhere in `core/src/` — so every single-run number carries unmeasured
variance, and C1 took three runs to stop moving. ⚠️ **B and C2 each passed over a pre-registered
*publish unmeasured* line**, deliberately and on the record; both sections say so. ⛔ **B is
benched on seat A's workload** and has never been measured on its own. ⛔ **E is not decided** —
one read verb of five, two record types of six.

**These are good enough to ship and are not the end of the question.** Every seat carries a
re-test-first candidate, for the next comparison rather than for now.

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

## Seat A — interactive reasoner · ✅ **DECIDED 2026-09-16: `deepseek/deepseek-v4-pro` @ fp8**

**Decided by the user on the clean re-run** (`runs/20260916-035320-ab/`, `MODEL_BENCH.md` Part 2).
**14/14 (100%)** free-form, **6.8s p50**, 18.6s worst, no errors. It is the only model that
cleared every gate at the top of the range.

**The free-form arm scores this seat** (user, 2026-09-16), which is the same decision as *tool
calling is the channel that ships*. The evidence was the constraint tax: **never positive** on any
row where it was legitimate (`+0`, `+0`, `−7pp`, `−14pp`), and the two models that scored best
under constraint are the two whose constrained arm **was not constrained** — both `gpt-oss-120b`
tiers read the response schema as a hint, ignoring it on 22 and 17 replies. ⚠️ The user's reason
is the seat's job rather than the numbers: seat A is chat and tool calling, not document
production. ⛔ Revisit if production shows issues, not before.

🔴 **The incumbent `gpt-oss-120b` @ turbo was unseated by the gate, not by score.** It returned a
**successful** request in 39.4s against the 30s ceiling, with a 4.9s median — an 8× spread. At 14
cases the worst case *is* the maximum, which this section states explicitly, so one slow answer
fails it by design. ⛔ It was never grandfathered.

### Kept on file for the next comparison

⚠️ Recorded deliberately, on the C3 pattern: these are the candidates to re-test **first** when a
newer model appears, so the next round starts from evidence rather than from the catalogue.

| candidate | why it is on the list |
|---|---|
| `glm-5.3` @ fp4 | **Joint-best accuracy, 14/14.** Failed only on latency — 117.3s worst against an 11.6s median. A serving-stack tail, not a reasoning limit; re-test on any new tier. |
| `gpt-oss-120b` @ turbo | **Fastest on the slate, 4.9s p50.** Out on a single 39.4s answer. If that tail is fixable it is the latency headroom this seat has. |
| `qwen3.5-397b-a17b` @ fp8 | 13/14 with the **tightest spread of any row** — 8.0s p50, 12.9s worst. The safest row if predictability ever outranks peak accuracy. |
| `deepseek-v4-flash` @ fp8 | 12/14, cheapest credible candidate, 6.6s p50. The cost-driven answer if seat A's volume grows. |

⛔ **Research before the next comparison, not during it** (user, 2026-09-16): what architectures or
serving changes would move these numbers — speculative decoding, prompt caching, a warm tier — so
the next slate tests *hypotheses* rather than re-measuring the same catalogue. Filed in `tasks.md`.

⚠️ **One row's numbers describe nothing:** `llama-4-maverick` @ base errored **11 of 14** calls.
⛔ Not a capability finding.

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

## Seat B — batch reasoner · ✅ **DECIDED 2026-09-16: `deepseek/deepseek-v4-pro` @ fp8**

**Decided on the sound 2026-09-16 run** (`runs/20260916-035320-ab/`). It ties `glm-5.3` at
**14/14 free-form**, and with no latency gate the ranking falls through to key 2, **cost per
run** — where it wins on every axis measured:

| | prompt | completion | reasoning |
|---|---|---|---|
| **`deepseek-v4-pro`** | **71,107** | **2,534** | **0** |
| `glm-5.3` @ fp4 | 102,150 | 7,378 | 5,251 |

⚠️ **The *publish unmeasured* line was passed over deliberately** (user, 2026-09-16), and this is
an amendment in effect if not in wording. The justification: cost is this seat's ranking key 2
*and* its tie-break, so falling through to it is following the ranking rather than breaking it.
⛔ The line still holds where the lower keys cannot separate candidates either.

⚠️ Same model as seat A, which was not a criterion but is one fewer endpoint to operate.

**Kept on file:** `glm-5.3` @ fp4 — equal accuracy, and the only reason it lost is that it spends
**5,251 reasoning tokens** to v4-pro's zero. Re-test it first if reasoning-token pricing changes.

🔴 **Decided on the 03:53 run, not the 11:07 one.** The temperature-0 re-run was **rate-limited**
(81 backoffs on one row, 42 on another) and its scores are depressed by an unknown amount that
differs per row. ⛔ Do not rank from `runs/20260916-110711-ab-t0/` — it is R13's shape again.

⛔ **This seat is benched on seat A's workload**, which is a real limitation and not a caveat to
wave through: `check_in.rs` raises one question a day about reviewing beliefs, and none of the 15
slate cases resembles it. `MODEL_BENCH.md` Part 2 has the detail. The pick above is the best
available answer, not a measurement of the job.

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

## Seat C1 — document extractor · ✅ **DECIDED 2026-09-16: `deepseek-ai/DeepSeek-V4.1-Flash`**

**Decided on the confirm run** (`runs/20260916-101748-c1-confirm/`, 30 cases / 375 labelled rows).
Clean on both top keys and ahead on every other measure:

| model | flips | misread | recall | production | receipts sound | med s |
|---|---|---|---|---|---|---|
| **`DeepSeek-V4.1-Flash`** | **0** | **0** | **96.0%** | **92.1%** | **5/6** | 21.5 |
| `gemma-4-31B` | 0 | 0 | 87.1% | 82.6% | 2/6 | 30.4 |
| `gemma-4-26B` | 0 | 0 | 93.8% | 65.5% | 1/6 | 40.7 |
| `Llama-4-Maverick` | 0 | 11 | 66.9% | 59.3% | 2/6 | **5.6** |

⛔ **The "13 sign-flips" reading is withdrawn.** It came from a single wide-draw run and did not
reproduce on the identical cases — R26, and the reason this seat took three runs to settle.

⚠️ **`Llama-4-Maverick` fails the 70% gate on two independent draws** (67.7%, 66.9%) and is the
only model on the slate with misread figures. ⛔ Its 5× speed advantage does not buy it in.

**Kept on file:** `gemma-4-31B` — the conservative pick. ⚠️ It is **clean on the injection probe
where the seated model is not** (R23), at the cost of ~9 points of statement recall and most of
the receipt arm. Re-test it first if document-borne injection ever stops being theoretical.

⚠️ **R23 is a selection note here, not a blocker** (user, 2026-09-16). `DeepSeek-V4.1-Flash`
tripped the C2 injection gate ambiguously — correct `kind`, no fields, payload pinned to `title`
and no further, where obedience and correct cataloguing look identical. ⛔ **The seat's safety
does not rest on it**: extracted transactions land on a confirm-draft screen a person accepts,
and `verify` checks the arithmetic. 🔴 What `verify` cannot catch is a *balanced* fabrication —
which is exactly why sign-flips are ranked above misreads, and why the person at that screen is
doing real work.

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

## Seat D — note structurer · ✅ **DECIDED 2026-09-16: `deepseek/deepseek-v4-flash` @ fp8**

**Decided on the first D slate** (`runs/20260916-121154-d/`, 7 rows, 11 probes × 3 runs each) —
and decided by the ranking alone, with no judgement call:

| model | fabrications | recall | abstention | agreement | errors |
|---|---|---|---|---|---|
| **`deepseek-v4-flash`** | **0** | **27/27** | **66/66** | 24/33 | 0 |
| `gpt-oss-120b` @ bf16 | 0 | **0/27** | 66/66 | 33/33 | 0 |
| `gpt-oss-120b` @ turbo | 0 | 1/27 | 66/66 | 32/33 | 0 |
| `qwen3.5-397b-a17b` | 1 | 27/27 | 65/66 | 23/33 | 0 |
| `deepseek-v4-pro` | 3 | 26/27 | 63/66 | 23/33 | 0 |
| `qwen3.6-35b-a3b` | 3 | 27/27 | 60/63 | 17/32 | 1 |
| `glm-5.3` @ fp4 | 7 | 27/27 | 59/66 | 23/33 | 0 |

🔴 **The failure this section predicted actually happened.** Both `gpt-oss-120b` tiers scored
**zero fabrications with 0/27 and 1/27 recall** — a perfect abstention card earned by finding
almost nothing. The note below says making zero fabrications a gate "would reward a model that
calls nothing on every probe", and that recall is key 2 "precisely so a model cannot win by
silence." ⛔ Three models tied on key 1 and recall separated them 27/27 against 0/27. The
pre-registration did the work; nobody had to argue.

**Kept on file:** `qwen3.5-397b-a17b` — one fabrication behind on a card that is otherwise
equal, and the candidate to re-test first if D's volume makes cost-per-call bite harder.

⚠️ Measured with the bench sending `temperature: 0`, unlike seats A/B/C which were decided on
runs that sent no temperature at all (R26). ⛔ Do not compare D's card against theirs.

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

## Seat E — local retrieval · ⛔ **NOT decided — corrected 2026-09-16**

**This section previously read "already decided and measured", and that was wrong** (user,
2026-09-16). `--bench-retrieval` measures the ranking stack behind **one** of the assistant's five
read verbs (`search`), over **two** of the catalogue's six record types (notes and journal
entries). ⛔ Retrieval over `document`, `transaction`, `routine` and `belief` has never been
measured, and `list` / `read` / `list_types` / `describe_type` have no ranking instrument at all.

What the 2026-09-10 run does settle and keeps: the ordering between keyword, fused and reranked
arms on note/journal text, and the memory budget that ordering must fit. ⚠️ Carry the
lexical/semantic split and its enforcing test into any extension — the gap is coverage, not
method. `MODEL_BENCH.md` Part 5; work filed in `tasks.md`.

⚠️ Still true that it needs **no endpoint, credentials or tokens** — it is local and free, which
is why extending it is cheap relative to every other seat in this file.

## Seat C2 — document reading · ✅ **DECIDED 2026-09-16: `google/gemma-4-31B-it`**

**Decided by falling through the tie rather than breaking it** (user, 2026-09-16). The
pre-registered floor says a one-or-two-fabrication gap **is not a difference** — so
`gemma-4-26B`'s 3 against `gemma-4-31B`'s 5 is a **tie on key 1**, and the ranking moves to key 2,
recall, which separates them decisively:

| model | fabrication | recall /24 | run-errors /18 |
|---|---|---|---|
| **`gemma-4-31B`** | 5 *(tied)* | **23 (95.8%)** | 4 |
| `gemma-4-26B-A4B` | 3 *(tied)* | 18 (75.0%) | 3 |
| `GLM-5.3-Flash` | 5 | 13 (54.2%) | 1 |

⚠️ **This is an amendment in effect** and is recorded as one: the section below says *declare the
seat unmeasured and widen the probe set*, and that was not done. The reading taken instead — a
gap inside the noise floor is a tie, and a tie falls to the next key — is defensible but is **not
what was pre-registered**. ⛔ Anyone re-opening this should know the rule was passed over, not met.

🔴 **Only 3 of 14 models populate `fields` at all** (R21), and that is a prompt defect rather than
a model finding: `fields` is not in `document_schema`'s `required` and the prompt frames it as
discretionary. ⛔ The seat is filled from a field of three, and fixing the prompt should re-open it.

**Kept on file:** `gemma-4-26B-A4B` — wins outright if fabrication is ever read as a real gap
rather than noise, and it is the one to re-test first after the prompt fix.

⚠️ Third place `GLM-5.3-Flash` is **disqualified** on the injection gate regardless of score, and
so are `Llama-4-Scout` and `Qwen3-VL-30B`. Two more, `Llama-4-Maverick` and
`DeepSeek-V4.1-Flash`, tripped the gate in a way the instrument cannot distinguish from correctly
describing the document — see R23. ⛔ Resolve that before either is considered for C1 or C2.

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
