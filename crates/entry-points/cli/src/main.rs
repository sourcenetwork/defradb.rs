//! DefraDB CLI - Command-line interface for DefraDB
//!
//! This binary provides the `defra` command for interacting with DefraDB nodes.
//! It supports starting nodes, managing schemas, querying data, and more.

use std::process::ExitCode;

use clap::Parser;
use tracing::error;

use cli::cli::{Cli, Command};
use cli::config::Config;
use cli::error::Result;

/// The process allocator: a heap per thread, and a block freed on another
/// thread goes back to its own heap without a lock, which the node's workers
/// do whenever a value crosses cores.
#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// The process's one network runtime is started here and every command runs
/// on it. Once the command returns, this thread waits while it shuts down in
/// order, its owner thread dropping it outside any async context.
fn main() -> ExitCode {
    // CI's comparison run sets it to 0 to run every shard without multishot
    // receive buffers.
    if let Some(recv_buffers) = std::env::var("DEFRA_NUCLEI_RECV_BUFFERS")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        nuclei::Proactor::with_config(nuclei::config::NucleiConfig {
            iouring: nuclei::config::IoUringConfiguration {
                recv_buffers,
                ..Default::default()
            },
            ..Default::default()
        });
    }
    let cli = Cli::parse();
    // Load configuration (flags → env → config file → defaults)
    let config = match Config::load(&cli) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match edge::EdgeRuntime::start(edge::EdgeConfig {
        placement: edge_placement(&cli, &config),
        ..edge::EdgeConfig::default()
    }) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let edge = runtime.edge().clone();
    let outcome = edge
        .block_on(run(cli, config, edge.clone()))
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()));
    if let Err(error) = runtime.shutdown() {
        error!(%error, "network runtime did not shut down cleanly");
    }
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Install the process-wide rustls crypto provider.
///
/// We cannot control which crypto providers our upstream deps enable, and
/// rustls panics rather than choosing when the graph offers more than one, so
/// install one unconditionally instead of relying on feature unification.
///
/// We intentionally do this in the CLI wrapper instead of the library
/// so that embedded applications can choose their own provider.
///
/// Must run before anything builds a TLS client or server. An `Err` means a
/// provider is already installed, which is equally fine.
fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Where the network runtime's threads run: where `start` places its worker
/// pool (`--worker-placement`, `DEFRA_WORKER_PLACEMENT`, then
/// `workers.placement`), so the network threads and the workers follow one
/// decision. A value the pool cannot read leaves the default here, and
/// `start` refuses it.
fn edge_placement(cli: &Cli, config: &Config) -> workers::Placement {
    let flag = match &cli.command {
        Command::Start(start) => start.workers.worker_placement.as_deref(),
        _ => None,
    };
    flag.or(config.workers.placement.as_deref())
        .and_then(|placement| placement.parse().ok())
        .unwrap_or_default()
}

async fn run(cli: Cli, config: Config, edge: edge::Edge) -> Result<()> {
    install_crypto_provider();

    // Initialize logging based on config. Telemetry only initializes for the
    // `start` command (matching Go, which configures it only in start.go) so
    // ephemeral commands like `version` / `client` don't spin up the exporter.
    let logging = cli::logging::init(&config, &edge, should_profile(&cli), enable_telemetry(&cli))?;
    for warning in &config.ignored_keys {
        tracing::warn!("{warning}");
    }

    // Execute the command
    let result = cli.execute(config, edge).await;
    logging.finish();
    result
}

fn should_profile(cli: &Cli) -> bool {
    matches!(&cli.command, Command::Start(args) if args.profile)
}

fn enable_telemetry(cli: &Cli) -> bool {
    matches!(&cli.command, Command::Start(_))
}

#[cfg(test)]
#[path = "../tests/unit/main_tests.rs"]
mod tests;
