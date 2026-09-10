# How retrieval is built

> Status: **built and measured 2026-09-10.** Unlike [The assistant](assistant.md), this page
> describes machinery that exists rather than a design committed to in advance. Every number
> on it comes from `--bench-retrieval`, a deterministic local harness; the full scorecard is
> in the repository's `MODEL_BENCH.md` § Retrieval.
>
> This is the engineering half. What retrieval *promises* — that nothing leaves your machine,
> that the assistant reads and cannot write — is on the assistant page, and that page is the
> contract. This one explains how the promise is kept, and where it is expensive.

Finding a record has three passes, and only the first two run by default.

A **keyword** pass over SurrealDB's full-text index. A **semantic** pass over vectors produced
by an embedding model running on your own machine. Their results are fused into one ranked
list. A third **reranking** pass can reorder that list with a larger model, and is off.

The reason for more than one pass is a single failure the first cannot fix. Ask about a rent
increase and a journal entry reading "the landlord bumped the rate again" shares not one word
with the question. No amount of keyword tuning finds it. That gap, rather than ranking quality,
is what the vector pass exists for.

## Their scores cannot be compared, so they are not compared

The two passes produce numbers on different scales. A BM25 score is relative to the corpus it
was computed against — its document count, its average length — so a note scoring 1.7 and a
journal entry scoring 1.7 were graded on different curves. A cosine distance is not
corpus-relative at all. Averaging them, or thresholding them together, would be arithmetic on
incomparable units.

What *is* comparable is position, so combining the two passes is done on rank alone. Which
raises the question of how much each ranking should count for, and that turned out to have a
measured answer rather than an obvious one.

## Why the two retrievers are not equals

The first design fused them as peers, using
[Reciprocal Rank Fusion](https://dl.acm.org/doi/10.1145/1571941.1572114): sum `1 / (k + rank)`
across the lists a record appears in, `k = 60` from the original paper, so a record both passes
rank highly beats one that either ranks first alone. It is a good default and it is wrong here.

**It was measured, and it lost.** With the keyword pass returning results, fusing the two
dropped top-1 accuracy from 75% to 56% and MRR from 0.865 to 0.726. Ranking on the semantic
pass alone scored exactly what fusion scored before — identical on every metric, to three
decimals — which is the more uncomfortable half of the finding: the keyword pass had never
changed a single ranking. It had been silent, because it required every word of the question to
be present, and fusion had been the vector ranking wearing a second name.

The mechanism is that RRF at `k = 60` is nearly flat. Rank 1 contributes `1/61`, rank 20
contributes `1/80` — a spread of 31%. So *appearing in both lists* outweighs *ranking first in
one*: a mediocre record placed fifth and eighth scores 0.031, while the right record placed
first in one list scores 0.016 and loses. That is the correct behaviour when both retrievers
are about equally trustworthy, because then agreement really is evidence. When one of them
matches any record sharing any word with the question, agreement is cheap and stops being
evidence at all.

So the passes are combined by precedence rather than by vote. **The semantic ranking is the
order.** The keyword pass contributes records the semantic pass did not find, placed below
everything it did — nothing is lost, and nothing is reordered. With no embedding model present
the semantic ranking is empty and keyword order is the answer, which is exactly what a build
without the feature should do.

⚠️ **What this does not establish.** The fixture cannot show the keyword pass at its best:
ranking on vectors alone scores a perfect 1.000 on the lexical half, so there is no case left
for BM25 to win. Rare identifiers, error codes, account numbers and exact dates are where a
keyword index earns its place, and the fixture has none. The honest claim is that fusion is
*unevidenced and currently harmful*, not that it is useless — which is why `fusion::fuse`
survives in the source, unwired, with the argument for it intact.

One ranked list across every kind of record is the visible consequence. The guard against it is
a **floor of one slot per matching type**, applied *after* ranking rather than during it, so a
journal with hundreds of entries cannot bury the single note that held the answer. When the
requested limit is smaller than the number of matching types the floor cannot be honoured for
all of them, and the plain ranking wins.

The match tally reported alongside results counts keyword matches only, and is named
`keyword_matches` for that reason. There is no meaningful count on the semantic side — every
record has *some* similarity to every query — so a combined total would be a number with no
referent.

## Vectors live beside the records, not inside them

Embeddings go in their own table, `record_embeddings`, one row per chunk.

Two reasons, and the first is arithmetic. A long journal entry needs several vectors, and a
column holds one. The second is ownership: the record tables are shared with a client that will
never run a model, and widening them with a field only one process can populate would put a
model-shaped hole in every other device's schema.

The index's `DIMENSION` is fixed when the table is defined, so `init_schema` takes the width
from the loaded model rather than a constant. A model of a different width then fails loudly at
definition time instead of writing vectors the index cannot accept.

### Chunking is a correctness bound

`MAX_CHUNK_BYTES` is not a tuning knob. fastembed truncates at the model's `max_length` — 512
tokens for the BGE small family — and does it **silently**. An over-long chunk is not an error;
its tail simply never reaches the index and is unfindable, with nothing reporting that anything
was dropped. English averages roughly four bytes per token, but dates, identifiers and code are
far denser, so the bound sits well under 512 × 4.

Chunks overlap. Without overlap a sentence spanning a boundary is split across two vectors and
matches neither well, which is the retrieval equivalent of a torn page.

### Questions and passages are embedded differently

The models in use are trained *asymmetrically*: a query carries an instruction prefix that a
stored passage does not. BGE v1.5 English expects one specific sentence on the query side; E5
uses `query:` and `passage:`; MiniLM uses neither.

⚠️ The prefix is therefore **per-model and wrong to generalise**. Applying BGE's prefix to a
model not trained with it raises no error — it quietly shifts every query away from its own
passages and costs recall. The symptom is "semantic search is disappointing", which is not a
symptom anyone debugs.

### The sweep is deliberately not a projection

Everything else in omni-me materializes from events. The vector index does not.

A projection's `apply` fires once per event, and journal autosave emits dozens of update events
for a single day's entry. Replaying that would re-embed the same text dozens of times, slowest
precisely at the cold start where it hurts most. Instead a sweep walks the already-materialized
rows and compares a content hash, which costs one pass and is naturally idempotent.

The hash covers the model name as well as the text, because after a model change the stored
vectors are the right *shape* and the wrong *meaning* — a text-only hash would skip all of them
as current. `clear` exists as the forced-rebuild escape hatch on top of that.

A sweep reports what it skipped, not only what it embedded. A sweep that silently embedded
nothing and a sweep that correctly found nothing to do are otherwise indistinguishable.

### The index is used only if you ask for it exactly right

⚠️ SurrealDB's KNN operator dispatches on the *shape* of its arguments. `<|K,EF|>` with two
integers takes the approximate HNSW path. `<|K,COSINE|>` parses perfectly well, returns correct
results, and **silently performs a brute-force table scan** — the index is never touched. The
keyword is `DISTANCE`, not `DIST`.

Nothing in the query result distinguishes these. The failure surfaces only as latency that
grows with the corpus, which is the last place anyone looks.

## The third pass, and why it defaults to off

The embedder is a **bi-encoder**: it turns a passage into a vector once, offline, without ever
seeing the question. That is what makes it cheap enough to run across everything you have ever
written, and also what caps it — it must summarise a passage without knowing what will be asked
of it.

A reranker is a **cross-encoder**. It reads the question and one candidate together and scores
that pair directly. Nothing can be precomputed, so every candidate costs one inference. That is
affordable over twenty candidates and impossible over twenty thousand, which is why it runs
last, over a pool wider than the final list — a cross-encoder that only sees the top few can
reorder them but can never rescue a right answer that fusion ranked eleventh.

**It is off because it was measured, not because it is risky.** On the retrieval fixture,
**three of the four available reranking models scored worse than not reranking at all.** Fusion
alone already returns the right record in every case and puts it first in three out of four,
which leaves a third pass little room to help and plenty to break.

Two consequences worth stating plainly:

- The default model is **not** the smallest one. The smallest, `jina-turbo`, is the worst
  performer — it drops ranking quality noticeably, almost entirely on the semantic cases it was
  supposed to help. A default that makes things worse when switched on is worse than a default
  that needs a larger machine.
- Reranking is not free accuracy, it is **a different set of mistakes**. The one model that
  helps improves semantic ranking substantially and *degrades* lexical ranking while doing it.

The sample is sixteen cases, and the page will not pretend otherwise: that is enough to choose
between two models, and not enough to claim reranking is worth its cost.

## Nothing is behind a compile-time gate except the model itself

Embedding needs ONNX Runtime, so it sits behind an `embeddings` feature. Fusion, chunking and
the merge logic do not, and are deliberately kept gate-free so they stay testable on a build
with no ONNX Runtime in it.

The mechanism is that the semantic and reranking passes enter as **traits** rather than
concrete types. Signatures carry no `#[cfg]`, take `Option<&dyn …>`, and compile identically
whether or not the host built with the feature. `None` is keyword-only, which is exactly the
behaviour that shipped before any of this existed.

⚠️ A reranker returning nothing means **"no opinion", never "no results"**. Callers keep the
fused order. A reranker that failed to load, timed out, or was handed nothing must degrade the
*ordering* and never the answer — and for the same reason the semantic pass returns a plain
list rather than a `Result`. Retrieval degrading to keyword-only is a worse answer, not a
failed one, and a model handed an error there spends a turn recovering from something it
cannot fix.

## Where it runs, and what that costs

The assistant runs on the deployment host as its own process, with its own embedded database,
syncing over HTTP exactly as a phone does. So the host carries a **second full replica** of the
data — a disk budget as well as a memory one — and the vector index exists in that process
only. The sync server never builds one.

Measured on that host: **3820 MB of memory, no swap, two cores**, with the sync server resident
at 279 MB and 29 GB of the 38 GB disk free.

Against it, embedding-only retrieval costs about **215 MB resident** and single-digit
milliseconds per query. The one reranker that improves ranking costs roughly **750 MB to 1.1 GB
resident and 621 ms per query on twelve cores**.

The memory would fit. **The time would not.** Six hundred milliseconds on twelve cores is
something in the region of three to four seconds on two, and that is latency a person waits
through on every question. Doubling the machine to four cores still leaves about two seconds.

**So the decision is to stay on that host, with reranking off** — and the reason is the core
count, not the memory budget. A larger machine would buy a quality difference the measurement
above explicitly declines to call established, and would not make the pass fast enough to want.

Two operational notes follow from it:

- **Swap is worth adding even though the budget fits.** With no swap, memory pressure is an
  OOM kill rather than a slowdown, and nothing guarantees the kernel chooses the assistant over
  the sync server.
- **Set the HNSW cache size explicitly** rather than inheriting SurrealDB's 256 MiB default.
  ⚠️ `SURREAL_HNSW_CACHE_SIZE` is parsed as a raw byte count. A value like `64MB` does not
  fail — it fails to parse and falls back to the default, silently.

## Questions are stripped before they reach the keyword index

SurrealDB's full-text match operator defaults to requiring **every** word of the query to be
present. "when is my dentist appointment" therefore returned nothing against a note titled
*Dentist appointment*, because no record contains the word "when" — which is most of why the
keyword pass had never contributed a ranking.

Matching now succeeds on any word. That alone would be worse, not better: the analyzer has no
stopword filter, so every record containing "is" would match, and the tally reported next to
results would become the size of the corpus. So function words are removed in the application
before the query is sent — there is no analyzer filter to do it, and the one mechanism
SurrealDB offers for token rewriting needs a file present on every device.

⚠️ **Stopwords here are not a ranking device.** BM25's own term weighting already scores a
match on "the" near zero. Their job is to keep the match count meaningful.

⚠️ **A question made entirely of function words is sent unchanged.** "who am I" could
legitimately be answering to a note by that name, and the alternative is an empty query, which
matches nothing and is indistinguishable from an honest miss. The rule means the step can
weaken a ranking but can never remove an answer.

The stopword list is deliberately shy: a missed function word costs a little tally precision,
while a content word wrongly listed costs recall silently and permanently. So "may" and "march"
are months, "can" and "will" are nouns, and none of them are on it.

## What is still wrong

**Stemming is not applied.** "appointments" does not match "appointment". SurrealDB ships a
Snowball filter that would fix it, but the analyzer is defined with `IF NOT EXISTS`, so adding
one changes nothing on any device that already has the old definition — and overwriting it
means rebuilding every full-text index that names it. That is a migration, not a setting.

**The benchmark cannot currently adjudicate the keyword pass.** Its lexical cases are ones the
embedding model also happens to win, so the measurements above establish that fusion hurts here
without establishing what a keyword index is worth. Cases built to defeat an embedder — an
account number, an error code, an exact date, an unusual proper noun — are what the fixture
needs before this question is reopened.

**One benchmark case is mislabelled**, and the mechanism that was supposed to prevent it has a
loophole. Cases are marked lexical or semantic, and a test asserts that a semantic case shares
no distinctive word with its answer — but it checks against its own list of words to ignore,
and that list has grown to include "worse", "day", "back", "up" and "out". Those are content
words. "the leak in the kitchen is getting worse" is consequently labelled semantic against an
entry reading "The tap has got worse", and the keyword pass finds it by that shared word. The
split column exists precisely to stop the semantic half being flattered, so the fix is to hold
both halves to one definition of a function word rather than two.
