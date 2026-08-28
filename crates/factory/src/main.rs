//! `factory` — operator CLI. The only wiring site besides `stewardd`.
#![forbid(unsafe_code)]
// Binaries are the one place startup wiring may give up; each expect must state its invariant.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::unnecessary_wraps
)]

use std::fmt::Write as _;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use infra::BdCli;
use infra::app::domain::{BeadId, TaskState};
use infra::app::{Bead, BeadStore};

#[derive(Debug, Parser)]
#[command(name = "factory", version, about = "Autonomous AI software factory")]
struct Cli {
    /// Directory containing `.beads/` (defaults to the current directory).
    #[arg(long, global = true, default_value = ".")]
    workdir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print build/version information.
    Version,
    /// Inspect beads through the factory's typed view.
    Bead {
        #[command(subcommand)]
        command: BeadCommand,
    },
}

#[derive(Debug, Subcommand)]
enum BeadCommand {
    /// Show a bead with its factory kind, state, budget and lease decoded.
    Show { id: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let store = BdCli::new(&cli.workdir).with_actor("factory");
    match cli.command {
        Command::Version => println!("factory {}", env!("CARGO_PKG_VERSION")),
        Command::Bead {
            command: BeadCommand::Show { id },
        } => {
            let id = BeadId::try_new(id)?;
            let bead = store.show(&id).await?;
            print!("{}", render(&bead));
        }
    }
    Ok(())
}

fn render(b: &Bead) -> String {
    let mut out = String::new();
    // Writing to a String cannot fail; the Results are discarded deliberately.
    let _ = writeln!(out, "{}  {}", b.id, b.title);
    let _ = writeln!(out, "  bd status : {}", b.status.as_str());
    match b.kind {
        Some(k) => {
            let _ = writeln!(out, "  kind      : {k}");
        }
        None => {
            let _ = writeln!(out, "  kind      : (not a factory bead)");
        }
    }
    if let Some(m) = &b.meta {
        let _ = writeln!(out, "  state     : {}", m.state.name());
        let detail = match &m.state {
            TaskState::Leased { lease } => Some(format!(
                "  lease     : {} until {} (claimed {})",
                lease.holder, lease.expires, lease.claimed_at
            )),
            TaskState::InVerify { branch, head } | TaskState::Mergeable { branch, head } => {
                Some(format!("  branch    : {branch} @ {head}"))
            }
            TaskState::Closed { merged } => Some(format!("  merged    : {merged}")),
            TaskState::Incident { reason } => Some(format!("  incident  : {reason:?}")),
            TaskState::Open => None,
        };
        if let Some(d) = detail {
            let _ = writeln!(out, "{d}");
        }
        let _ = writeln!(out, "  verify    : {}", m.verify_bead);
        let _ = writeln!(out, "  base      : {}", m.base);
        let _ = writeln!(
            out,
            "  budget    : tokens {}/{}  wall {}s/{}s  attempts {}/{}  lease expiries {}",
            m.usage.tokens,
            m.budget.tokens,
            m.usage.wall_clock.seconds(),
            m.budget.wall_clock.seconds(),
            m.usage.attempts,
            m.budget.attempts,
            m.lease_expiries
        );
    }
    if let Some(a) = &b.acceptance {
        let _ = writeln!(out, "  accept    : {a}");
    }
    if let Some(n) = &b.notes {
        let _ = writeln!(out, "  notes     :");
        for line in n.lines() {
            let _ = writeln!(out, "    {line}");
        }
    }
    out
}
