# Two roots on disk, and why

omni-me keeps two separate directories on disk, and the separation is load-bearing rather than
tidiness. `XDG_CONFIG_HOME` holds what an operator supplies and the app only ever reads, which
today means `credentials.toml`. `XDG_STATE_HOME` holds what the app writes itself: `sources.toml`
and `paused_sources.toml`. In the container those resolve to `/config/omni-me` and
`/data/state/omni-me`.

They used to be the same directory, and every write the app attempted failed.

## The failure

The container runs unprivileged as uid 10001. The credentials secret is mounted read-only at
`/config/omni-me/credentials.toml`, a single-file bind mount. Docker creates the parent
directories of a bind mount that do not already exist, and it creates them owned by **root**. So
`/config/omni-me` existed, held exactly one file, and was not writable by the user the app runs
as. Every attempt to save a source definition or a pause failed with `Permission denied
(os error 13)`.

The shape is worth recognising because nothing about it looks like a permissions problem. The
compose file mounts one read-only file and says nothing about directories. The Dockerfile sets
`XDG_CONFIG_HOME` and says nothing about the mount. The two are only related through a Docker
behaviour that neither file mentions, and the directory that ends up root-owned is one no author
ever wrote down.

## Why it stayed hidden

The off-switch that persists a pause is the one that mattered, and it half-applied. The handler
aborts the source's scheduler task first, then writes the pause to disk. When the write failed it
returned a 500, but the source was already stopped in memory, and `/auto_import/status` reports
the **live registry rather than disk**. So the status endpoint confirmed a pause that had not been
saved, and a restart brought the source back.

Two things follow, and both outlive the bug:

- **Never cite `/auto_import/status` as evidence that a configuration change was saved.** It
  answers a different question, and it answers it correctly. Read the file.
- **A 500 from pause or resume now says the two stores diverged**, in those words, rather than
  offering a generic configuration error. The live change stands and a restart will undo it,
  which is a specific thing an operator needs to be told.

The source add and remove paths never had this problem. They write the file first and only touch
the registry once the write succeeded, so a failure there changed nothing anywhere.

## Why the in-memory change is not rolled back

Reversing the order looks like the obvious fix and is the wrong one. Pausing a source is what you
do when it is misbehaving, and the motivating incident was a bank source making repeated real
login attempts. Stopping it immediately matters more than the two stores agreeing. If the durable
write fails, the right outcome is a stopped source plus a loud error, not a source left running
because a file could not be written.

Consistency is bought at the other end instead: the writable directory is now one the app owns,
so the write does not fail.

## The rule

The directory the app writes to must not contain a bind mount, and must be under a path the
container user owns. `/data` already satisfies both because it is the app user's home and the
named volume inherits its ownership from the image.

A reset of the data volume now also clears `sources.toml` and `paused_sources.toml`, since they
live on it. That is correct for a dev instance being reseeded, and it is worth knowing before
resetting anything else.
