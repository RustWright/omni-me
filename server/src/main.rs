//! Public `omni-me-server` binary — bank-free.
//!
//! The public engine has no compiled-in bank adapters. Instead [`config_sources`]
//! reads a server-side `sources.toml` (3.6) and builds whatever *generic*
//! sources it declares — CSV files, subprocess helpers — via
//! `core::auto_import::config::build_generic_sources`. With no file (or an empty
//! one) it returns zero sources, so the server still boots, serves, and syncs
//! with auto-import simply idle (graceful zero-config, 3.4). The private overlay
//! binary supplies its own builder that *also* wires real bank adapters and
//! calls the same [`omni_me_server::run`].

use omni_me_core::auto_import::config::{self, SourcesConfig};
use omni_me_server::{run, RunConfig, SourceCtx, SourceFuture};

/// Config-driven source builder. Named (not an inline closure) so its explicit
/// `SourceFuture` return type gives the `Box::pin(async {…})` a coercion site
/// into the boxed trait-object future. Any failure to read/parse `sources.toml`
/// degrades to zero sources with a warning rather than aborting boot.
fn config_sources(ctx: SourceCtx) -> SourceFuture {
    Box::pin(async move {
        let cfg = config::default_path()
            .map_err(|e| tracing::warn!(error = %e, "could not resolve sources.toml path"))
            .and_then(|p| {
                config::load(&p)
                    .map_err(|e| tracing::warn!(error = %e, "failed to load sources.toml"))
            })
            .unwrap_or_else(|()| SourcesConfig::default());
        config::build_generic_sources(&cfg, &ctx.store, &ctx.projections, &ctx.device_id)
    })
}

#[tokio::main]
async fn main() {
    run(RunConfig {
        source_builder: Box::new(config_sources),
    })
    .await;
}
