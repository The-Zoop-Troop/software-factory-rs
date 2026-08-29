//! `factory` — operator CLI. Parses, runs `cli::run`, prints the error. Nothing else lives here.
#![forbid(unsafe_code)]
#![allow(
    clippy::disallowed_methods,
    reason = "tokio::main expands to expect; the binary is the wiring site"
)]

mod cli;
mod doctor;
mod remote;
#[cfg(test)]
mod remote_tests;
mod telegram;

use clap::Parser as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::from_default_env();
    if std::env::var("FACTORY_LOG_FORMAT").is_ok_and(|v| v == "json") {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    }
    cli::run(cli::Cli::parse()).await
}
