# The assistant, and what it is allowed to do

> Status: **decided 2026-09-07**, ahead of the work it describes; the read half
> **built 2026-09-08**. Most of this page is still marked **(planned)**: it is a
> committed design written down before the implementation so the implementation can
> be checked against it, following the same practice as
> [What omni-me insists on](invariants.md). Sections marked **(today)** describe
> machinery that exists now.
>
> The design is checked against this page, not the other way round — but the page is
> not frozen either. Where building something showed the design to be wrong, the page
> changes and says why.

omni-me accumulates a lot about one person: journal entries going back years, notes,
routines, records of things bought and done. An assistant with reach across all of it is
useful in a way that an assistant over any one slice is not, and it is dangerous in the
same proportion. This page is the contract that makes the first true without the second.

It is written for two readers: someone deciding whether to let software like this near
their life, and someone extending omni-me who needs to know which lines are load-bearing.

## Five verbs, and the list stays small **(read half today)**

The assistant acts through exactly five tools:

| Verb | What it does | |
|---|---|---|
| `search` | Find records matching a query. Returns summaries, not bodies. | **(today)** |
| `read` | Fetch one record in full, by identity. | **(today)** |
| `list_types` | List the kinds of record this app holds. | **(today)** |
| `describe_type` | Describe one kind: how it is identified, and its properties. | **(today)** |
| `propose` | Offer a change for approval. The only route to any write. | **(planned)** |

The four read verbs work. `propose` does not exist yet, and until it does the
assistant is **read-only in the strongest sense** — there is no write path behind
any tool it holds, so "it cannot change anything" is a property of the build
rather than a promise about behaviour.

Search matches on **meaning as well as wording**. Ask about a rent increase and an
entry saying "the landlord bumped the rate again" comes back, despite sharing not one
word with the question. Keyword matching alone could not find it, and that gap — not
ranking quality — was the thing worth fixing.

Two retrievers run over every question: the keyword index, and a vector index built by
an embedding model that runs **on your own machine**. Neither sends anything anywhere.
Their results are combined by *rank* rather than by score, because the two produce
numbers that are not on a common scale — a keyword score is relative to the collection
it was computed against, while a vector distance is not.

That difference is also why results come back as **one ranked list across every kind of
record**, rather than a pile per kind. A single honest ordering was not available while
only keyword scores existed. Every kind that matched anything keeps at least one place
in the list, so a journal with hundreds of entries cannot bury the one note that
actually held the answer.

A third pass is available and **off by default**: a *reranking* model that reads your
question and a candidate record together and scores that pair directly. The two
retrievers above cannot do this. The embedding model has to summarise a record into a
fixed set of numbers before it knows what will be asked about it, which is what makes
it fast enough to run over everything you have ever written — and also what caps how
well it can judge any single one. A reranker has no such limit, and no such speed: it
costs one model run per candidate, so it only ever sees the couple of dozen the first
two passes agreed were plausible.

It is **off by default, and that is a measured decision rather than a cautious one**. On
the retrieval tests, three of the four available reranking models made results *worse*
than not reranking at all — the first two passes already return the right record every
time and put it first three times in four, which leaves a third pass little to improve
and plenty to break. One model did better, and it needs more memory than a small
machine has.

So this is a knob for a well-resourced machine and a corpus larger than the one it was
measured on, not a free upgrade. Switching it on, and choosing which model, are both
settings — no rebuild, and nothing new leaves your machine either way.

The interface did not change when any of this landed: the same `search` verb, the same
arguments. What changed is what comes back.

The obvious alternative is a tool per feature: `create_journal_entry`, `add_routine`,
`log_expense`. It is rejected, and the reason is not taste. Tool-calling accuracy degrades
measurably once a model is choosing among roughly fifteen to twenty tools, with a
documented bias toward whichever tools appear early in the list. A per-feature surface
therefore gets *worse at exactly the rate the app gets richer*, which is the opposite of
what a personal system needs over years of use.

So the verbs are generic over the kinds of record, and what those kinds are is a
catalogue the assistant reads at runtime through `list_types` and `describe_type`.
**New features add entries, never tools.** The tool count is the same on the day
omni-me holds forty kinds of record as on the day it holds two.

The catalogue is deliberately **not** the same thing as a
[record type declaration](invariants.md#journal-is-one-record-type-not-a-special-case).
A declaration describes a *document form* — its properties, what makes it complete,
what its template looks like — so the app can render an editor for it. The catalogue
describes how a collection is *searched and fetched*: its identity, the fields worth
matching on, and what belongs to it. Journal entries and notes have both, which makes
them look like one idea; a routine has only the second, because a routine is a small
tree of rows rather than a document. Where a kind of record does have a declaration,
`describe_type` reads it, so the properties it reports are the ones you declared.

**What it costs you:** the model has to make two or three calls to do what one bespoke tool
could do in one, and it must be able to read a type declaration and work out what to do with
it. That is a real tax on every single request, paid deliberately to avoid a surface that
rots.

## The assistant is a device, not a feature **(planned)**

It runs as its own process, with its own database, its own device id, syncing over HTTP
exactly as a phone does.

Partly this is forced. The embedded store is opened per process, and pointing two processes
at one embedded database is a known way to corrupt it. But given the choice was forced, the
device shape is also the one worth wanting:

- **Its failure is contained.** An assistant that wedges, loops or exhausts memory takes down
  the assistant. Sync, which every other device depends on, keeps running.
- **It inherits the machinery instead of duplicating it.** Config, feature switches, record
  type declarations, the backfill path a fresh device already uses, and audit identity all
  work for it because they work for any device.
- **Its proposals arrive everywhere for free.** They are events, so they sync to phone and
  desktop like anything else. The approval inbox needs no delivery mechanism of its own.
- **It can move.** If the model ever runs on different hardware, the assistant follows it
  without a redesign, because talking to the rest of the system over sync is all it ever did.

**What it costs you:** a second copy of the log on whatever host runs it, and a second
process to supervise.

## Everything it does is signed **(planned)**

The assistant authors under **its own device id**, never a shared one and never the id used
by automated imports.

This sounds like bookkeeping and is actually the foundation of everything below it. "What
has the assistant done on its own?" has to be a query with an exact answer, not a
reconstruction from timestamps and guesswork. Without it, granting autonomy is unauditable,
and unwinding a misbehaving assistant means unpicking its work from yours by hand.

## It proposes; you dispose **(planned)**

There is no write verb. `propose` records an intention, and nothing changes until you accept
it.

The shape is not new: omni-me already proposes batches of imported transactions and waits
for review **(today)**, and that design keeps the whole proposed set in the event rather than
only the accepted part, so a rejection stays legible as *"this was seen and declined"* rather
than vanishing. The assistant generalizes it.

That one structure does four jobs, which is why it is worth getting right:

1. **The audit trail** of what was suggested and what you did about it.
2. **The evidence** for granting autonomy later, because an approval rate per action type is
   exactly what a promotion decision needs.
3. **A training corpus**, if the model is ever personalized on your own accepted and rejected
   proposals.
4. **An evaluation set**, so a candidate model can be scored on whether it would have
   proposed what you actually approved.

## Autonomy is earned, one action type at a time **(planned)**

The assistant starts able to do nothing but propose. Permission is granted per action type,
recorded as events like everything else, and every grant is revocable.

The destination is a single rule: **reversible things it may do freely; irreversible things
it always asks about.** That is not the starting state, because trusting it on day one would
be a guess. It is where sustained evidence is allowed to lead.

### Reversibility, precisely

Every action type declares whether it is reversible **from the day it is defined**, including
the ones that begin with no permission at all. The predicate is the destination of the whole
model, so it cannot be retrofitted onto a hundred action types later.

An action is **reversible** only if all three hold:

1. **Undo is derivable from the log.** Restoring the prior state needs nothing that is not
   already recorded. In an event-sourced system this is usually true for content, which is
   why creating, editing, and even deleting a record are all reversible: the log still holds
   what was there.
2. **The effect never left the log's jurisdiction.** Anything that writes outside the event
   log, calls an external service, sends a message, or moves money is irreversible, because
   no event appended afterwards can retract it.
3. **Nothing outside the system observed it.** Reversibility is a property of *state*, not of
   attention. A notification that has been read cannot be unread. If the entire purpose of an
   action is to reach a person, it is irreversible whatever happens to the record afterwards.

Anything that costs money is irreversible by rule 2, even when the record of it is trivially
deletable. When a case is arguable, it is irreversible: the failure mode of guessing wrong in
that direction is an unnecessary approval prompt, and in the other direction it is an
unrequested act you cannot undo.

## Two models, and one of them is never trusted **(planned)**

Documents and messages arriving from the outside world are read by a **quarantined** model
that has no tools, cannot call anything, and can only emit fields matching a fixed schema.
Its output is data.

The **privileged** model is the one holding the five verbs. It never sees a raw email body, a
raw document, or the bytes of a photograph. It sees typed fields the quarantined model
extracted, and it knows they came from outside.

Neither model can fetch anything from the network. Combined with running the model on
infrastructure you control, this removes an entire class of attack: text hidden in a document
that instructs an assistant to send your data somewhere has nowhere to send it and no tool to
send it with.

The split partly exists already, since document extraction is schema-constrained and
tool-free **(today)**, but today that is a coincidence of how it was built. It becomes a
boundary that is declared and enforced.

## Provenance travels with the value **(planned)**

Every value the assistant handles carries where it came from, and values derived from
untrusted input are marked as such. A marked value can never silently become a tool argument.

One mechanism, two jobs. As **security**, it is what stops instructions smuggled inside a
receipt from steering a real action. As **epistemics**, it is what lets you ask why the
assistant believes something and get the actual evidence back rather than a plausible
retelling.

## What it believes is as reviewable as what it does **(planned)**

The assistant will accumulate conclusions: that you sleep badly in weeks you travel, that you
underestimate long tasks. These are stored as ordinary records with a statement, a confidence,
links to the evidence events behind them, and a way to be superseded when they stop being
true. They are proposed through the same gate as any other change.

**The risk, stated plainly:** a system that accumulates opinions about you can be wrong in
ways that compound quietly, and each wrong belief shapes the next suggestion. The mitigation
is that every belief cites its evidence and the whole set is listable, so it can be audited
rather than merely trusted. That is a mitigation, not a solution, and it is the part of this
design most likely to need revisiting once it meets real use.

## Capture: who authored the content, not who pressed the shutter **(planned)**

Dictating an entry or photographing something is how content gets in. Whether it is trusted
depends on **who wrote what is being captured**, and that is a sharper line than it first
looks.

- **You authored it**: dictated speech, a photograph of your own handwritten page. The content
  is yours, and it goes in as ordinary content.
- **Someone else authored it**: a receipt, an invoice, a statement, a printed notice, a
  whiteboard someone else wrote on. That you chose to photograph it does not make it yours.
  It is an outside document that arrived by camera instead of by email, and it takes the
  quarantine route above, unchanged.

Getting this backwards is the tempting mistake, because the capture *feels* deliberate and
personal. But a receipt can be printed with text aimed at whatever reads it, and the fact that
you photographed it deliberately changes nothing about who wrote it.

Either way, a transcription or extraction is a machine's reading, so it lands as a draft you
review and correct rather than a saved record, on the same principle that already governs
captured content **(today)**.

Speech recognition runs on your own device or your own server. Shipping your voice to a third
party would give away exactly what hosting the rest of this yourself was meant to keep.

## What leaves your machine **(planned)**

Everything else in omni-me stays on your devices and your own server. The assistant is the one
part that does not, and pretending otherwise would be the wrong kind of documentation.

**Content.** Prompts carry your records. Two consequences are easy to miss: retrieval means a
narrow question can send a wide slice of unrelated entries, and a multi-step request re-sends
everything it has gathered on each subsequent step. Keeping the retrieved slice small is
therefore a privacy control, not only a cost one.

**Metadata, which no zero-retention promise covers.** Retention guarantees are about content.
Providers keep billing metadata regardless, and request timestamps plus token counts describe
when you write, how much, and when you sleep. This is inherent to using a metered API and no
provider choice removes it. Running the model on your own hardware is the only thing that does.

**Nothing else.** No image keeps its EXIF: capture time, device model and firmware are stripped
at the boundary, on every path, because they are a device fingerprint that accumulates. No
model context can fetch from the network. And responses are never written to logs by default,
because a model quotes your records back and a log is the easiest place for that to escape.

**What you get in exchange for choosing a self-hosted model instead:** content and metadata both
stay put, and the exchange is cost and capability. Both are supported configurations, and the
[configuration decision](#where-the-model-runs-is-a-configuration-decision) below is where that
trade is made.

## Where the model runs is a configuration decision

This one is deliberately *not* an invariant, and saying so is the point: everything here
speaks the same OpenAI-compatible protocol, so the model behind it is swappable without an
architectural change.

For the person this was built for, the reasoning is about **incentive alignment rather than
physical control**. Owning the hardware is the only true structural fix, and it is out of
reach; any rented machine is somebody else's computer. Given that, the axis that actually
varies between providers is what they stand to gain from a creative reading of their own
terms. A provider that does not own the models it serves, and is not racing to train better
ones, has materially less to gain than one that does.

That yields the criteria used to choose:

1. **Does not own the models it serves.** The core of the argument, and the one that does
   the most work.
2. **Contractual zero data retention**: no training on inputs, no human review.
3. **A real service level agreement.**
4. **You end up owning any personalized adapter**, and the corpus it was trained on stays
   yours.
5. **Serves open weights, speaks OpenAI-compatible.**

**An earlier version of this list ranked EU jurisdiction and residency second, and that was
a mistake worth recording rather than quietly deleting.** Jurisdiction addresses a *different*
threat than the one above: state access, not commercial misuse. It is also weaker than it
looks — the US CLOUD Act reaches a provider, not a location, so "US company, EU datacentre"
buys very little, and the EU-jurisdiction candidates that clear the other criteria carry
substantial US operations anyway. Meanwhile the actual worry — a provider with something to
gain from a creative reading of its own terms — is answered by criterion 1 alone. So
jurisdiction is now a tiebreak, held honestly, rather than a headline.

The provider chosen against this list is **DeepInfra**: it owns no models, states zero
retention by default, runs its own hardware rather than renting from a hyperscaler, and
publishes a real SLA. **The accepted cost is stated plainly: it is US-jurisdiction and
reachable by US legal process.** Structural protection against that is not purchasable here —
inference requires the model to see plaintext, so the encryption answer does not exist.
Confidential-computing endpoints (inference inside an attested GPU enclave, where the host
operator genuinely cannot read the prompt) are the one real answer and are worth revisiting;
today they lack the SLA and the fine-tuning path.

Criterion 4 works differently than expected, and better. DeepInfra does not train fine-tunes —
it serves an adapter you trained elsewhere. So the approved-proposal corpus, the most
sensitive derived artifact in the system, never leaves your machine at all.

If you are running your own omni-me, none of this is prescribed. A commercial frontier model
is a supported configuration and will be better at the hard reasoning. It is a different
trade, not a worse one, and the interface does not change — set `allow_closed_weights = true`
in the `[llm]` section to say so. Left unset, a closed model is **refused**, because an
open-weights provider that also proxies one applies the model owner's policy to it rather than
its own, and the two sit one line apart in the same catalogue.

## One model per job, chosen by measurement

There is no single model behind all of this. Jobs differ in what they actually demand, so
each of these roles earns its own, benchmarked rather than argued:

| Role | What it does | What decides the choice | Filled by |
|---|---|---|---|
| **Interactive reasoner** | the assistant loop and chat | latency — a slow model disqualifies itself | *leading candidate, see below* |
| **Batch reasoner** | overnight review, habits, derived beliefs | quality; it can afford to be slow | **not yet** — the benchmark saturates |
| **Quarantined extractor** | receipts, statements, photographed documents | reads images, and **never holds tools** | **not yet** — no caller exists |
| **High-volume structurer** | note extraction, categorization | cost per call, and knowing when to abstain | **not yet** — no caller exists |
| **Local** | search embeddings, reranking, speech recognition | never leaves your machine at all | **filled for search** — see `MODEL_BENCH.md` § Retrieval |

An empty seat here means *unmeasured*, not *pending selection*. Three of them have no
caller in the system yet, and benchmarking a model for work nothing performs would measure
a configuration we cannot ship. They are filled by the work that needs them.

### What the first measurement found (2026-09-09)

Ten multi-turn cases over a seeded corpus, every candidate pinned to the production
provider's own serving stack. The leading interactive candidate is **`openai/gpt-oss-120b`
served on a low-latency tier**, at a 3.5s median and a 6.3s worst case with a perfect verb
score; **`deepseek/deepseek-v4-flash`** is the close alternative, equally accurate, several
times cheaper, and spending no reasoning tokens at all, for roughly 2.5× the latency.

Two results matter more than the ranking.

**The same weights, served two ways by the same provider, differ by 3.7× in latency** — 3.5s
against 12.8s. "How fast is this model" is not a well-formed question; only "how fast is this
endpoint" is. The same lesson had already arrived twice from the other direction: a model that
could not do constrained decoding on one host did it in 1.3s on another, and a model that
looked six times faster than its rivals turned out to have been measured on a stack we do not
use.

**The batch seat stays empty because the benchmark saturated.** Four candidates scored
perfectly, so the instrument cannot separate them at the top — and a tie broken by preference
would be exactly the argued-not-measured choice this table exists to avoid. Batch work is
judged on derived beliefs and overnight review, which do not exist yet; the seat is filled
when there is something real to score.

The quarantined extractor is a trust boundary rather than a performance tier — it is the
"never trusted" half described above, and it stays a separate model for that reason alone.
The structurer is judged on **abstention**: a categorizer that is right 85% of the time and
says so when unsure beats one that is right 95% of the time and confidently wrong for the
rest, because an unsorted item is easy to fix and a wrongly-sorted one hides.

## What it will not do

- **Commit anything without approval**, until you have explicitly granted that action type,
  and never for an irreversible one.
- **Fetch from the network** out of any model context.
- **Write as another device.** Its work is always attributable to it.
- **Act on an action type whose reversibility is undeclared.** An undeclared type is treated
  as irreversible.
- **Treat record content as instructions**, however the content is phrased.
