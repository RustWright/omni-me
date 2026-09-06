# What omni-me insists on, and what is yours

> Status: **decided 2026-09-05**, ahead of the work it describes. Sections marked
> **(today)** describe shipped behaviour. Sections marked **(planned)** are committed
> decisions that are not built yet, written down first so the implementation can be
> checked against them rather than the other way round.

omni-me is a personal life-operating-system app built for one person's daily use. It is
open-core so that other people can run their own, and that raises a fair question: how much
of what you find here is a considered design, and how much is just one person's preference
that you are now stuck with?

This document answers that. It lists the things the app genuinely insists on and why, so you
meet a documented decision instead of an undocumented wall. Everything not listed here is
meant to be yours.

## The rule that decides where a customization lives

**Client customization is data. Server customization may be code.**

That is not an aesthetic preference. It follows from what a rebuild costs on each side:

- Changing the **server** means swapping one binary's composition root. It builds in CI, needs
  no mobile toolchain and no signing key. The private overlay pattern this repo is designed
  around does exactly that, and it is a supported thing for you to do too.
- Changing the **client** means an Android NDK, a JDK, the wasm toolchain, a signing key, and
  publishing your own updates, because app updates are served from a static directory on your
  own server. A client fork and a painless update path are mutually exclusive.

So anything you should reasonably be able to change about the app itself is reachable without
a rebuild, and the things that do require a rebuild are on the side where a rebuild is cheap.

## Three ways to extend, in increasing cost

1. **Config.** Anything expressible as a value. No rebuild, and updates keep working.
2. **A subprocess plugin.** Any new data source. Write a program in any language that speaks
   one line of JSON on stdin and stdout, and point a config entry at it. The engine never sees
   your credentials, so this boundary is structural rather than a promise. The contract is
   frozen and documented in `SUBPROCESS_SOURCE_CONTRACT.md`. No fork, no rebuild of the app.
3. **An overlay crate.** New in-process behaviour: your own event types, projections or
   compiled-in sources. A sibling crate that depends on these crates and supplies its own
   `main`. This is a fork of the server only, and the server is the cheap side.

## What omni-me insists on

### Every change is an event **(today)**

State is a fold over an append-only log. Nothing is updated in place, and read models are
rebuilt by replaying.

**Why:** it is what makes the data durable through schema churn, and it is why turning a
feature off is safe rather than destructive. Disable something and its read models stop being
maintained; enable it again and they rebuild from the log. In a conventional app that
round trip is data loss.

**What it costs you:** the log only grows, and a change to how data is interpreted applies
retroactively on replay.

### Notes are markdown with YAML frontmatter **(today)**

**Why:** the notes stay readable and portable without this app. You can point Obsidian or any
editor at an export and lose nothing.

**What it costs you:** the note body is text, so features that want rich structured content
have to encode it in frontmatter or in the body's own conventions.

### One instance, one person **(today)**

There is no login, no user model and no multi-tenancy. There is a single shared token between
your devices and your server.

**Why:** it was built for one person, and adding accounts would change nearly every read path
for a benefit nobody using it has asked for.

**What it costs you:** if two people want to use omni-me, they run two servers. This is not
expected to change, so please do not plan around it changing.

### Finances are double-entry bookkeeping **(today)**

A transaction is a set of postings that balance. The ledger projects to a plaintext
hledger-format file, and balances are computed from it.

**Why:** it is the model that makes the books checkable. A single-entry list of amounts cannot
be verified against anything, whereas double-entry lets an import be checked against a
statement's own declared totals, which is how this app catches a bulk import that quietly
dropped rows.

**What it costs you:** you cannot swap in a different transaction model without new events and
new projections. What you *can* do is enter transactions single-entry style and let the app
fill the other side from a default account, which is what most personal finance apps are doing
underneath anyway.

**What is not forced:** your account names and your tagging conventions. An account is a free
string path throughout, the set of accounts is configuration, and display names can be
overridden. The app ships a small number of reserved account names, and they are the exception
rather than the pattern.

### The screens are the app's; your data shape is yours **(partly today)**

You can [hide features you do not use](features.md) and retheme the app today. You will be
able to declare what your records look like. You will not be able to write your own screens.

**Why:** the interface compiles to WebAssembly, so there is no mechanism to load someone
else's interface code at runtime. The only alternative would be inventing a layout language to
describe screens in config, and maintaining a language forever is a much larger promise than
this project can honestly make.

**What it costs you:** a genuinely new screen means an overlay and your own build.

### Journal is one record type, not a special case **(planned)**

A record type declares its identity rule, its template, its properties, what counts as
complete, and whether completed entries close themselves. The daily journal is one instance of
that, and the reflection prompts that ship with it are a preset you can replace, not the app's
opinion about how you should journal.

**Why:** the alternative is what this codebase did before, where one person's journaling
framework was compiled into the completeness check, the template and the properties panel.

**What it costs you today:** journal entries are keyed by date, one per day. Multiple entries
per day has no representation yet.

### Habits are not tasks **(today)**

Routines repeat between every day and every 31 days. Anything less frequent is rejected.

**Why:** routines exist for habit formation, and something that fires four times a year is a
calendar entry wearing a habit's clothes.

**What it costs you:** if you want a quarterly recurring item, this is not the feature for it.

## What is yours

Configuration today: the server address and token, timezone, base currency, your account list
and display-name overrides, your LLM provider, your data sources, the theme and accent, and
[which features exist at all](features.md) — per device as well as globally.

Planned: your record types, including their templates, properties and completeness rules.

Always yours, with no rebuild and no permission: a new data source, via a subprocess plugin.

## Where this boundary will move

Invariants can be revisited, but each one here is load-bearing for something else in the
design, so the honest expectation is that they hold. If you hit one of these and it is
genuinely in your way, that is worth raising as an issue, because a wall someone hits
repeatedly is evidence the decision was wrong.
