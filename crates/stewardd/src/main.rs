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

use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use infra::app::{Clock, sweep};
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
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();

    if let Some(dir) = cli.events.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let store = BdCli::new(&cli.workdir).with_actor("stewardd");
    let clock = SystemClock;
    let log = JsonlSink::open(&cli.events)?;

    let token = CancellationToken::new();
    tokio::spawn(shutdown_signal(token.clone()));

    loop {
        match sweep(&store, &clock, &log, "stewardd").await {
            Ok(report) => tracing::info!(?report, "sweep"),
            Err(e) => tracing::error!(error = %e, "sweep failed"),
        }
        if cli.once || token.is_cancelled() {
            break;
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(cli.interval)) => {}
            () = token.cancelled() => break,
        }
    }
    tracing::info!(at = clock.now().unix_seconds(), "stewardd stopped");
    drop(log);
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
