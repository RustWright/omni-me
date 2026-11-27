//! Public `omni-me-server` binary, bank-free: [`config_sources`] builds only the generic
//! sources `sources.toml` declares, and none without the file. The overlay binary passes its
//! own builder, with the bank adapters, to the same [`omni_me_server::run`].

use omni_me_core::auto_import::config::{self, SourcesConfig};
use omni_me_server::{RunConfig, SourceCtx, SourceFuture, run};

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

/// Built by hand rather than with `#[tokio::main]`: the database engine needs a
/// worker stack larger than tokio's default. See `omni_me_core::async_runtime`.
fn main() {
    let runtime = match omni_me_core::async_runtime::build() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("could not start the async runtime: {e}");
            std::process::exit(1);
        }
    };
    runtime.block_on(run(RunConfig {
        source_builder: Box::new(config_sources),
        calendar: None,
    }));
}
