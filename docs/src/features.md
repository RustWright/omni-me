# Turning features off

omni-me ships seven features. Any of them can be switched off, globally or on one
device, and "off" means the feature stops existing rather than being hidden: its tab
disappears, its background work never starts, and it can no longer write anything or
reach your server.

Nothing is deleted. Every change you have ever made is still in the event log, so
turning a feature back on rebuilds its data by replaying that log. That round trip is
the reason [event sourcing](invariants.md) is an invariant here — in a conventional
app, switching a feature off and on again would mean losing what it held.

## The features

| Feature | Key | What it is |
|---|---|---|
| Journal | `feature.journal` | The date-keyed daily entry, one per day |
| Notes | `feature.notes` | Free-form titled notes |
| Routines | `feature.routines` | Repeating habits and their completions |
| Finances | `feature.finances` | Double-entry transactions, accounts, budgets, reconciliation |
| Auto-import | `feature.auto_import` | Pulling transactions from your banks and brokerages |
| LLM | `feature.llm` | Extracting structure from documents, photos and email |
| Documents | `feature.documents` | The document archive — files you scan or upload, and what was read out of them |

Journal, Notes, Routines, Finances and Documents each own a tab. Auto-import and the
LLM do not: they appear as flows inside Finances, so switching either off removes those
flows and its settings section but no tab.

Documents is deliberately its own feature rather than part of Finances. The archive
holds tax notices, paystubs and leases as readily as bank statements, so filing it
under Finances would mean switching Finances off took your tax records with it.

## What "off" actually stops

Switching a feature off stops five things, and it is worth knowing they are not the
same list for every feature:

- **Its tab**, if it has one. Settings is never hidden — it holds the switches.
- **Its projections**: the read models that answer queries. These stop being
  maintained, so the tables freeze where they are.
- **Its background work**: the schedulers that scan and close things on a timer.
- **Its commands**: anything that would write one of its events, or reach the server on
  its behalf, refuses. Reads are deliberately *not* blocked — they would only return the
  frozen tables described below, and nothing in the app can reach them anyway once the
  tab is gone.
- **Its settings sections**, so you are not configuring something inert.

Features do not map one-to-one onto projections, and two cases are worth stating
because they look surprising from the outside:

- The projection behind journal entries and notes also handles LLM results, so it keeps
  running while **any** of Journal, Notes or LLM is on.
- Auto-import produces transactions, and transactions are Finances' read model. So
  running auto-import with Finances off keeps that read model alive — your imported
  transactions are recorded and will be there when Finances comes back, but there is no
  screen showing them in the meantime. The review-and-commit flow lives in the Finances
  tab, so it is unreachable rather than half-working.

## Where a setting lives

Every feature switch exists at two layers:

- **Shared**, synced across all your devices. This is the answer unless a device
  overrides it.
- **This device**, which is never synced. Setting it here overrides the shared value on
  this device only, and clearing it goes back to following the shared one.

The Settings screen shows both and says which one is winning.

## Changes apply at the next launch

A feature switch takes effect when the app next starts, and the Settings row says so.

This is not a limitation waiting to be fixed. What a feature registers — its
projections, its schedulers — is decided during startup, before anything is on screen.
Applying a switch immediately would leave the half state: a tab gone while its
background work still runs, or a tab present with nothing maintaining its data. The
launch boundary is the only point where the whole set changes together.

Theme and accent are different: they are pure presentation, so they repaint the moment
you change them.

## Wiping one feature's data

Switching a feature off freezes its data. Wiping is the other operation, and it is
deliberately harder to reach: it deletes the events one feature owns, then rebuilds what
was derived from them. The use for it is a clean re-import — a ledger whose rows came
from several versions of an importer is neither empty nor complete, and the gap left by
an old one is invisible once it is in the data.

**What a feature owns is read off the same map that decides what it may write.** There is
one classification of event types to features, used both to refuse a write while a
feature is off and to decide what a wipe of that feature takes. A second list would
eventually disagree with the first, and the way it would show up is a new kind of finance
event that is correctly gated and then survives a finances wipe.

Three consequences of that, and none of them is obvious from the outside:

**Some events belong to two features.** A recorded transaction is authored under either
Finances or auto-import, because a committed import batch is the second author of a
transaction. So a wipe of *either* feature claims it. That is right for a ledger wipe — a
transaction is a transaction, whoever proposed it — but it means a finances wipe removes
transactions while leaving the batch that proposed them.

**Which matters because the batch carries the dedup key.** Import batches are identified
by a content hash so that re-fetching an overlapping window collapses onto the row
already proposed rather than adding a second one. That record is derived from the batch
events, so it survives a finances-only wipe — and the same statements, imported again,
collapse onto a batch already marked committed. The ledger stays empty and nothing says
why. A re-import therefore has to name both features, which is why the operation takes a
list rather than a single feature and never widens one silently.

**Four event types belong to no feature at all** and so survive every wipe: the config
that records which features are on, a filed problem report, a declared record type, and
the audit record of a wipe itself. A wipe that could take the config would be a wipe that
turns off the feature it just emptied.

**A wipe is per node, not global.** Every device keeps a complete log, and each one pushes
what it authored using its own watermark. A wiped event is behind every peer's cursor, so
it does not come back — but the other nodes still hold their copies and still project
them. Wiping the server alone leaves the phone showing the old ledger: not a stale cache,
a genuinely different answer. Clearing a feature everywhere means clearing it on every
node, and the count each one reports is how you check that it happened.

That count is the other half of the design. The operation reports how many events it
removed rather than simply succeeding, for the same reason a backup is verified by size
and not by exit code: "it worked" cannot be checked against what was there.

### Two steps, and three gates

A wipe is reached over the server's own API, in the shape the document purge already
uses: **preview, then confirm.**

The preview reports how many events of each type would go — per type, because a total
cannot be compared against anything a person can count — and writes nothing. Looking and
walking away leaves the log untouched. It also hands back a token, and reports which
deployment answered.

The confirm has to present three things, and each closes a different way of destroying
the wrong data:

- **The token**, which is spent by the confirm it authorises. A second confirm with the
  same token is refused rather than being a second wipe, so a retry has to preview again
  and see current state.
- **A subset of the previewed features.** The confirm may narrow what the preview
  described and may never widen it. Previewing one feature and confirming two would
  destroy events nobody counted.
- **The name of the deployment it believes it is talking to**, as `/health` reports it. A
  server that declares itself `dev` refuses a wipe addressed to production, and a server
  that declares nothing at all refuses every wipe — an undeclared deployment is treated
  as a refusal wherever a destructive tool asks, so a half-provisioned box can never be
  mistaken for the dev one.

Listing what can be wiped is its own read-only endpoint, and documents appear on it
carrying their refusal rather than being left off. Omitted, the refusal would read as an
oversight to route around.

## Two things this does not cover

**The server does not read these switches.** Auto-import runs on your server, and
whether a source is polled is governed by that server's own source configuration and
per-source pause controls — not by `feature.auto_import` on a client. Switching the
feature off on a device stops that device from showing and reviewing batches; it does
not stop the server fetching them. To stop the fetching, pause the source.

**A feature's existing tables are left in place** while it is off. They stop being
updated, and nothing reads them, but they are not cleared. Clearing them would buy
tidiness at the cost of making re-enabling slow — and the replay is what makes the
round trip safe in the first place.
