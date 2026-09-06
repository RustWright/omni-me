# Turning features off

omni-me ships six features. Any of them can be switched off, globally or on one
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

Journal, Notes, Routines and Finances each own a tab. Auto-import and the LLM do not:
they appear as flows inside Finances, so switching either off removes those flows and
its settings section but no tab.

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
