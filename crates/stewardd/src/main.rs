//! `stewardd` — the factory's only daemon. Deterministic; no LLM.
//!
//! Loop: sweep the ledger (reap expired leases, escalate budget overruns, close finished
//! epics), append events to the JSONL log, sleep, repeat. SIGTERM/ctrl-c finishes the
//! in-flight sweep and exits.
#![forbid(unsafe_code)]
// Binaries are the one place startup wiring may give up; each expect must state its invariant.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::unnecessary_wraps
)]

mod run;

use std::path::PathBuf;

use clap::Parser;
use infra::app::domain::Duration;
use infra::{BdCli, JsonlSink, SystemClock};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
#[command(name = "stewardd", version, about = "Factory steward daemon")]
struct Cli {
    /// Directory containing `.beads/`.
    #[arg(long, default_value = ".")]
    workdir: PathBuf,
    /// Event log path (JSONL, appended).
    #[arg(long, default_value = ".factory/events.jsonl")]
    events: PathBuf,
    /// Seconds between sweep cycles.
    #[arg(long, default_value_t = 15)]
    interval: u64,
    /// Run a single sweep and exit.
    #[arg(long)]
    once: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = infra::telemetry::init("stewardd", &infra::TelemetryConfig::from_env())?;
    let cli = Cli::parse();

    if let Some(dir) = cli.events.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let store = BdCli::new(&cli.workdir).with_actor("stewardd");
    let clock = SystemClock;
    let log = JsonlSink::open(&cli.events)?;

    let token = CancellationToken::new();
    tokio::spawn(shutdown_signal(token.clone()));
    let stop = async move { token.cancelled().await };
    run::steward_loop(
        &store,
        &clock,
        &log,
        Duration::from_seconds(cli.interval),
        cli.once,
        stop,
    )
    .await;
    Ok(())
}

async fn shutdown_signal(token: CancellationToken) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("invariant: ctrl-c handler installs once at startup");
    };
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("invariant: SIGTERM handler installs once at startup")
            .recv()
            .await;
    };
    tokio::select! { () = ctrl_c => {}, () = terminate => {} }
    tracing::info!("shutdown signal received");
    token.cancel();
}
