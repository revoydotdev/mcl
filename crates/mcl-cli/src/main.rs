//! CLI entry point: thin wrapper that parses args and delegates to [`mcl_cli::run`].

use clap::Parser;
use mcl_cli::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.debug {
        env_logger::Builder::from_env("RUST_LOG")
            .filter_level(log::LevelFilter::Trace)
            .init();
    } else {
        env_logger::init();
    }

    mcl_cli::run(cli).await;
}
