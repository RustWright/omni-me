# Telling a dev instance from the real one

> Status: **built 2026-09-14.** Describes shipped behaviour.

If you run omni-me for real, sooner or later you will want a second instance to test against
— a scratch server you can wipe, reseed and point a test device at, without touching the data
you actually depend on.

The obvious way to get one is cheap, because `DB_PATH` and `BLOB_DIR` are both relative: a
second working directory is a second database. That is genuinely all the isolation the
*server* needs.

The problem is everything that acts *on* a server. A tool that wipes a database is pointed at
its target by configuration — a working directory, a container name, a URL — and
configuration is exactly the thing that is wrong when an accident happens. Worse, the most
useful dev instance is seeded from a copy of the live database, which makes the two
indistinguishable from the inside. Nothing about the data says which one you are looking at.

So the server says it instead.

## The marker

At boot the server resolves a **deployment identity** — `production` or `dev` — and writes it
to `.omni-instance` in its data directory. The same value is reported on `/health`:

```json
{ "status": "ok", "instance": "dev" }
```

Two properties do the work:

- **It lives with the data, not with the config.** The thing a destructive tool destroys is a
  directory or a volume, and that is precisely what carries the marker. Copy the data
  somewhere else and the claim goes with it.
- **`/health` is outside the authentication gate.** A tool can ask what it is talking to
  before it has a token, which is the only way the check can run first.

You declare the identity with `OMNI_INSTANCE=production` or `OMNI_INSTANCE=dev`, normally in
the unit file or compose environment.

## What happens at boot

| `OMNI_INSTANCE` | `.omni-instance` | Result |
| --- | --- | --- |
| unset | absent | Boots. Reports `unknown`. Stamps nothing. |
| unset | present | Boots. Reports what the marker says. |
| set | absent | Stamps the marker. Reports it. |
| set | present, agrees | Boots. Reports it. |
| set | present, **disagrees** | **Refuses to start.** |

Three of these deserve an explanation.

**Unset is allowed,** because a zero-config server is a promise this project keeps: `mkdir hub
&& cd hub && cargo run -p omni-me-server` has to work. Such a server reports `unknown`, and
`unknown` is refused by every tool that would destroy or seed it. Not knowing is not
permission.

**An undeclared process reports the marker rather than erasing it.** If you run the plain
binary in a stamped directory, the data's own claim stands. Silence is not a counter-assertion.

**Disagreement is a refusal, not a warning.** This is the case the whole mechanism exists for.
A clone of the live database, dropped into a directory whose config says `dev`, arrives
stamped `production` — and if the server booted anyway, a test device would sync real writes
into production while you believed you were on the clone. Refusing costs you one deliberate
command; continuing costs you data you cannot untangle afterwards.

That command is `OMNI_INSTANCE_RESTAMP=1`, which lets the declared value overwrite a marker
that disagrees. Use it when you have just cloned live data on purpose and the new identity is
the one you mean.

An unrecognised value — `OMNI_INSTANCE=prod`, say — is also a refusal rather than an absence.
A typo that read as "unset" would leave the directory unstamped while looking declared.

The check runs **before the database is opened**, so a refused boot has not taken the storage
lock and has not run a migration.

## What the guards do

Reading the marker is only half of it; something has to act on it. In the deployment scripts,
`omni_require_dev_volume` reads `.omni-instance` through a read-only mount and
`omni_require_dev_url` reads `/health`. Both refuse anything that is not exactly `dev`, and
both run *before* the operation they guard — including before the safety snapshot, since a
snapshot of the wrong volume is already the wrong operation.

`scripts/seed-bench-hub.py` applies the same rule in the write direction. Its default `--url`
port is also the live server's, so a mistyped host would push fictional benchmark events into
real data.

There is one escape, because destroying production is sometimes the actual intent — a
pre-release reset, for instance. Setting `OMNI_CONFIRM_PRODUCTION` to a fixed phrase (defined
in `deploy/lib-guard.sh`) allows a single run. The phrase carries a random suffix so it cannot
be reached by autocomplete or by a reflexive `y`.

The deploy's last-resort rollback restores a snapshot over production, which is legitimate and
automated, so it passes the phrase itself on that one invocation. Guarding it without that
would have converted a recoverable failed deploy into an outage with no way back — worth
knowing if you add a guard to a new script: check who else calls it.

## What this does not protect

The guard is a check on *tools that ask*. It cannot stop `rm -rf` on a data directory, a
`docker volume rm`, or anything else that never consults the marker. It narrows the accident
from "a wrong working directory is enough" to "you have to bypass a refusal", and that is the
claim — not that production has become unreachable.

Note also that until a stamping server has actually been deployed, an existing production
volume has **no marker**, so it reports `unknown` and the guards refuse it. That is the
intended order: deploy first, then reset.
