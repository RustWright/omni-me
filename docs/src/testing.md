# One database for the whole test suite

Every unit test in `omni-me-core` that needs a database calls `crate::db::test_db()`, and gets
a fresh, empty one. It is not a fresh *instance*: the whole test binary shares one embedded
SurrealKV instance, and each test gets its own namespace inside it.

## Why not one instance per test

That was the original shape, and it wedged CI. Each `connect` leaves an engine thread behind
for the rest of the process, whether or not the handle and its directory are dropped: a probe
of eight connect-write-drop cycles took the process from 11 threads to 18, one per connect,
and never back. With a few hundred tests the binary eventually deadlocked, and which test it
stopped on depended on how many connects had come before it. Running tests one at a time does
not help, because the count grows with connects, not with parallelism. The same probe with one
instance and eight namespaces stayed flat at 20 threads.

## What keeps tests apart

Every table, index, analyzer and projection bookmark (`projection_versions`) is defined inside
the selected namespace, and the core keeps no database state in process-wide statics. Cloning a
handle gives the clone its own session, so `use_ns` on one test's handle does not move
another's. `db::tests::test_databases_do_not_see_each_others_rows` checks this.

## The runtime it lives on

The embedded engine spawns its router and its background tasks on whichever tokio runtime
calls `connect`. Each `#[tokio::test]` builds its own runtime and drops it when the test ends,
so a shared instance connected from inside a test would die with that test. The fixture
connects from a process-lived runtime built by `async_runtime::build`, the same builder the
binaries use, so tests also run queries on the worker stack the engine needs
([The stack every process needs](runtime.md)).

## What still connects on its own

Tests of `connect` itself, in `db/mod.rs`, open their own files. So do the `server`, `agent` and
`tauri-app` tests, which are separate binaries with a handful of connects each.
