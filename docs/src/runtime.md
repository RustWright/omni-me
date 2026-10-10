# The stack every process needs

Every omni-me process embeds the database rather than talking to one over a socket, and that
choice reaches further into the process than a dependency usually does: it sets how much stack
the threads running queries need. SurrealDB computes a query by recursing, bounded by its own
ceiling of 120 frames, and upstream documents the consequence in
`surrealdb::engine::local` — embed it on a multi-thread runtime with an enlarged worker stack.
It is written there as a requirement of using the engine, not as tuning advice.

omni-me did not do this until 2026-09-27, and what it cost was the whole assistant.

## The failure

The headless agent aborted within a second of logging `agent running; answering questions`, on
a release build against a real server, with no question pending. Not an error return and not a
panic: `thread 'tokio-rt-worker' has overflowed its stack`, then SIGABRT. A stack overflow is
delivered as a signal, so nothing higher up gets to log it, retry it, or degrade — the process
is simply gone, and a supervisor restarting it walks straight back into the same tick.

Three runs settled what it was. Two at tokio's default worker stack of 2 MiB aborted, two for
two. One with `RUST_MIN_STACK=67108864` did not, and went on to answer a question and author a
proposal. Nothing else differed between the runs, so the size of the stack was the whole
variable — which also means it is deep recursion with a bound, not runaway recursion, because a
bigger stack fixed it rather than postponing it.

`tokio-rt-worker` is tokio's default name for both its worker threads and its blocking pool, so
the frames belonged to a runtime thread rather than to the process's main thread.

## Why every binary was exposed

`#[tokio::main]` builds a runtime with tokio's defaults, and nothing in the repo asked for
anything else. The agent is where it surfaced, but four entry points embed the same engine the
same way: the agent, the public server, the private overlay's server, and the app — which gets
its runtime from Tauri, and Tauri builds one with `Runtime::new()` on first use.

So the app had been running its command handlers, including archive and ledger queries, on 2 MiB
stacks for as long as it had existed. It never aborted, which is worth stating plainly: a query
shallow enough today can stop being shallow enough after a schema change nobody connected to
stack depth.

`omni_me_core::async_runtime::build` is now the one place that answers this, and every entry
point goes through it.

## The size, and what is not known about it

The default is 64 MiB, which is the size there is evidence for: 2 MiB aborts and 64 MiB runs
clean. Upstream suggests 10 MiB. Nothing here has tested 10 MiB against a query that reaches
SurrealDB's 120-frame ceiling, so the smaller number would be a guess dressed as a default. A
64 MiB stack costs address space rather than memory — Linux commits stack pages as they are
touched — so the conservative choice is close to free on the server, and this is not a number to
shave without a measurement that names the deepest query the archive can produce.

`OMNI_WORKER_STACK_MB` overrides it. That knob exists because setting an explicit stack size is
what stops `RUST_MIN_STACK` applying to these threads, and removing the escape hatch while the
true watermark is unmeasured would be the wrong trade.

## What this does not cover

A process's main thread keeps whatever the OS gave it, normally 8 MiB, and `thread_stack_size`
cannot reach it. Each binary runs its top-level future there via `block_on`, so that future is on
8 MiB — four times the size that aborted, but not the 64 MiB its spawned work gets. If a
top-level future ever overflows, the fix is a thread hop, not this setting.

Tests are the other gap. `#[tokio::test]` runs on the harness's own thread at 2 MiB unless
`RUST_MIN_STACK` says otherwise, so a test that drives a deep query can abort where production
would be fine. A test binary dying with a stack overflow rather than a failure is the signature.
