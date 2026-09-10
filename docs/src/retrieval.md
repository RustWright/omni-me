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

What *is* comparable is position. Both passes produce an ordering, and
[Reciprocal Rank Fusion](https://dl.acm.org/doi/10.1145/1571941.1572114) merges orderings by
summing `1 / (k + rank)` across the lists a record appears in, with `k = 60` from the original
paper. A record both passes rank highly beats one that either ranks first alone.

⚠️ This is why `fusion::fuse` takes rankings and never scores. Feeding raw scores in would
reintroduce exactly the incomparability that forced the older, worse design: results grouped
per record type, because no honest ordering across types was available.

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

## What is still wrong

The keyword pass is weaker than a BM25 baseline should be, and the reason is not tuning.

SurrealDB's full-text match operator defaults to requiring **every** word of the query to be
present. "when is my dentist appointment" therefore returns nothing despite a note titled
*Dentist appointment*, because no record contains the word "when". This affects the app's own
search box, not only the assistant.

An any-word mode exists, but the text analyzer has no stopword filter, so switching to it would
match nearly everything on words like "is" and "my" and destroy the match tallies at the same
time. The fix is a design change rather than a flag, and it has not been made yet.

Read the keyword-only row of the scorecard with this in mind. It is not BM25's ceiling.
