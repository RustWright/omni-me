//! Public `omni-me-server` binary — bank-free.
//!
//! The public engine ships with zero auto-import sources: [`no_sources`]
//! returns an empty vec, so the server boots, serves, and syncs with
//! auto-import simply idle. The private overlay binary supplies a builder that
//! wires in the real bank adapters and calls the same [`omni_me_server::run`].

use omni_me_server::{run, RunConfig, SourceCtx, SourceFuture};

/// Zero-source builder — the public engine has no bank adapters of its own.
/// Named (not an inline closure) so its explicit `SourceFuture` return type
/// gives the `Box::pin(async {…})` a coercion site into the boxed trait-object
/// future, which also lets `Vec::new()` infer its element type.
fn no_sources(_ctx: SourceCtx) -> SourceFuture {
    Box::pin(async { Vec::new() })
}

#[tokio::main]
async fn main() {
    run(RunConfig {
        source_builder: Box::new(no_sources),
    })
    .await;
}
