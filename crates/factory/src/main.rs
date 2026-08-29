//! `factory` — operator CLI. Parses, runs `cli::run`, prints the error. Nothing else lives here.
#![forbid(unsafe_code)]
#![allow(
    clippy::disallowed_methods,
    reason = "tokio::main expands to expect; the binary is the wiring site"
)]

mod cli;
mod doctor;

use clap::Parser as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    cli::run(cli::Cli::parse()).await
}
