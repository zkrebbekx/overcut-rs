//! The `overcut` binary: an F1 Fantasy analysis toolkit.
//!
//! The binary is a thin shell. It parses the command line, starts the
//! tracing subscriber, and hands the command to [`cli`]. Every command
//! calls a use case of the application layer. The `serve` command starts
//! the JSON API in [`http`].
//!
//! The exit code is 1 on any error. The error text goes to stderr with
//! the prefix `overcut: `.

mod cli;
mod http;
mod text;

use clap::Parser;
use tracing_subscriber::EnvFilter;

/// Starts the tracing subscriber.
///
/// The filter comes from `OVERCUT_LOG`, then from `RUST_LOG`, then
/// defaults to `warn`.
fn init_tracing() {
    let filter = EnvFilter::try_from_env("OVERCUT_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = cli::Cli::parse();
    if let Err(err) = cli.run().await {
        eprintln!("overcut: {err:#}");
        std::process::exit(1);
    }
}
