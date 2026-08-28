//! `stewardd` — the factory's only daemon. Deterministic; no LLM.
#![forbid(unsafe_code)]
// Binaries are the one place startup wiring may give up; each expect must state its invariant.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_methods, clippy::unnecessary_wraps)]

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "stewardd", version, about = "Factory steward daemon")]
struct Cli {
    /// Seconds between sweep cycles.
    #[arg(long, default_value_t = 15)]
    interval: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::from_default_env(),
    ).init();
    let cli = Cli::parse();
    tracing::info!(interval = cli.interval, "stewardd starting (no-op until fac-ec6.4)");
    Ok(())
}
