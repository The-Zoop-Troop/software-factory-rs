//! `factory` — operator CLI. Parses, runs `cli::run`, prints the error. Nothing else lives here.
#![forbid(unsafe_code)]
#![allow(
    clippy::disallowed_methods,
    reason = "tokio::main expands to expect; the binary is the wiring site"
)]

mod cli;
mod doctor;
mod metrics;
mod plan_cmd;
mod remote;
#[cfg(test)]
mod remote_tests;
mod render;
mod rig;
#[cfg(test)]
mod rig_tests;
mod telegram;

use clap::Parser as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Before anything holds a secret: children must not read this process's memory or environ.
    infra::harden_process()?;
    let _telemetry = infra::telemetry::init("factory", &infra::TelemetryConfig::from_env())?;
    cli::run(cli::Cli::parse()).await
}
