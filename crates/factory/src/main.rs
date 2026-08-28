//! `factory` — operator CLI. The only wiring site besides `stewardd`.
#![forbid(unsafe_code)]
// Binaries are the one place startup wiring may give up; each expect must state its invariant.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_methods, clippy::unnecessary_wraps)]

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "factory", version, about = "Autonomous AI software factory")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print build/version information.
    Version,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::from_default_env(),
    ).init();
    let cli = Cli::parse();
    match cli.command {
        Command::Version => println!("factory {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}
