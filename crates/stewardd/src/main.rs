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
use infra::app::domain::BranchName;
use infra::app::domain::Duration;
use infra::app::steward_contract::ContractSource;
use infra::{BdCli, GitCli, JsonlSink, SystemClock};
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
    /// The rig's repository: when present, a closing epic gets a contract bead (what landed).
    #[arg(long)]
    repo: Option<PathBuf>,
    /// Integration branch the contract's head is read from.
    #[arg(long, default_value = "main")]
    main: String,
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

    let main = BranchName::try_new(&cli.main)?;
    let git = cli
        .repo
        .as_ref()
        .filter(|p| p.join(".git").exists())
        .map(|p| GitCli::new(p, p.join(".factory/worktrees")));
    let contracts = git
        .as_ref()
        .map(|repo| ContractSource { repo, main: &main });

    let token = CancellationToken::new();
    tokio::spawn(shutdown_signal(token.clone()));
    let stop = async move { token.cancelled().await };
    run::steward_loop(
        &store,
        &clock,
        &log,
        contracts,
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
