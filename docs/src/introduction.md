# omni-me

A personal notes, journal, routines and finances app. Offline-first, event-sourced, running on
Android and Linux desktop from one Rust codebase, syncing through a server you run yourself.

It was built as a single-user system for one person's actual daily use. The design reflects
that, and this documentation tries not to pretend otherwise: it optimises for durability of the
data and for honesty about what has actually been verified, rather than for onboarding.

## Who this documentation is for

Two readers, and they want different things.

**Someone deciding whether to run their own.** Start with
[What omni-me insists on](invariants.md). It lists the things the app genuinely will not bend
on and why, so you can find out in ten minutes whether this is the wrong shape for you rather
than after a weekend of setup.

**Someone extending it.** The short version is that there are three ways in, in increasing
cost: configuration, a subprocess plugin for a new data source, and an overlay crate for new
in-process behaviour. Which one applies to your change, and why the line falls where it does,
is covered in [What omni-me insists on](invariants.md).

## The state of these docs

This site is new and incomplete, and it is going to say so rather than paper over the gaps.
Currently here: the invariants, the feature switches, and the assistant contract, that last
one written before its implementation rather than after. Not yet written: a setup guide, the
customization guide, and the architecture notes that presently live as long comment headers
inside the source files.

Until those land, the honest pointers are the repository's own `README.md` for how the system
is put together, and `SUBPROCESS_SOURCE_CONTRACT.md` for the data-source plugin contract, which
is frozen and complete.
