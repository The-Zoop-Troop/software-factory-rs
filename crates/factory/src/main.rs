//! `factory` — operator CLI. The only wiring site besides `stewardd`.
#![forbid(unsafe_code)]
// Binaries are the one place startup wiring may give up; each expect must state its invariant.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::unnecessary_wraps,
    clippy::too_many_lines
)]

use std::fmt::Write as _;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use infra::app::domain::{AgentId, BeadId, BranchName, Duration, PlanDefaults, TaskState};
use infra::app::{
    Bead, BeadStore, IntegrateConfig, WorkerConfig, integrate_once, plan, verify_once, work_once,
};
use infra::{BdCli, ClaudeCli, GitCli, JsonlSink, ShellRunner, SystemClock};

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
    /// Run the Planner: turn a plan (text or file) into an epic of task + verify beads.
    Plan {
        /// Path to the project clone (the Planner reads it for context in later phases).
        #[arg(long, default_value = "repo")]
        repo: PathBuf,
        /// Integration branch; tasks are cut from its current tip.
        #[arg(long, default_value = "main")]
        main: String,
        /// Read the plan from this file instead of --text.
        #[arg(long, conflicts_with = "text")]
        file: Option<PathBuf>,
        /// The plan, inline.
        #[arg(long)]
        text: Option<String>,
        /// Model override for the planner run.
        #[arg(long)]
        model: Option<String>,
        /// Spend cap for the planner run, USD.
        #[arg(long, default_value_t = 2.0)]
        max_budget_usd: f64,
    },
    /// Run a Worker: claim ready tasks and hand each to a fresh Claude Code session.
    Work {
        /// Path to the project clone.
        #[arg(long, default_value = "repo")]
        repo: PathBuf,
        /// Directory for task worktrees.
        #[arg(long, default_value = ".factory/worktrees")]
        worktrees: PathBuf,
        /// Event log path (JSONL, appended).
        #[arg(long, default_value = ".factory/events.jsonl")]
        events: PathBuf,
        /// Integration branch tasks are cut from.
        #[arg(long, default_value = "main")]
        main: String,
        /// This worker's identity (lease holder).
        #[arg(long, default_value = "worker-1")]
        agent: String,
        /// Lease TTL, seconds; heartbeats renew at a third of this.
        #[arg(long, default_value_t = 300)]
        lease_ttl: u64,
        /// Harness turn cap per task session.
        #[arg(long, default_value_t = 200)]
        max_turns: u32,
        /// Spend cap per task session, USD.
        #[arg(long, default_value_t = 5.0)]
        max_budget_usd: f64,
        /// Model override.
        #[arg(long)]
        model: Option<String>,
        /// Seconds to wait when nothing is ready; omit to run one task (or none) and exit.
        #[arg(long)]
        interval: Option<u64>,
    },
    /// Run the Verifier: check every task awaiting verification.
    Verify {
        /// Path to the project clone.
        #[arg(long, default_value = "repo")]
        repo: PathBuf,
        /// Directory for throwaway worktrees.
        #[arg(long, default_value = ".factory/worktrees")]
        worktrees: PathBuf,
        /// Event log path (JSONL, appended).
        #[arg(long, default_value = ".factory/events.jsonl")]
        events: PathBuf,
        /// Seconds between passes; omit to run once and exit.
        #[arg(long)]
        interval: Option<u64>,
    },
    /// Run the Integrator: land verified branches on main.
    Integrate {
        /// Path to the project clone.
        #[arg(long, default_value = "repo")]
        repo: PathBuf,
        /// Directory for throwaway worktrees.
        #[arg(long, default_value = ".factory/worktrees")]
        worktrees: PathBuf,
        /// Event log path (JSONL, appended).
        #[arg(long, default_value = ".factory/events.jsonl")]
        events: PathBuf,
        /// Integration branch.
        #[arg(long, default_value = "main")]
        main: String,
        /// Remote to push main to after landing (omit for local-only).
        #[arg(long)]
        remote: Option<String>,
        /// Project-wide check to run on the rebased head before landing (repeatable).
        #[arg(long = "check")]
        checks: Vec<String>,
        /// Timeout per check, seconds.
        #[arg(long, default_value_t = 1200)]
        check_timeout: u64,
        /// Seconds between passes; omit to run once and exit.
        #[arg(long)]
        interval: Option<u64>,
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
        Command::Plan {
            repo,
            main,
            file,
            text,
            model,
            max_budget_usd,
        } => {
            let plan_text = match (file, text) {
                (Some(f), _) => std::fs::read_to_string(f)?,
                (None, Some(t)) => t,
                (None, None) => anyhow::bail!("give the plan with --text or --file"),
            };
            let mut harness = ClaudeCli::default().with_max_budget_usd(max_budget_usd);
            if let Some(m) = model {
                harness = harness.with_model(m);
            }
            let git = GitCli::new(&repo, repo.join(".factory-worktrees"));
            let store = BdCli::new(&cli.workdir).with_actor("planner");
            let report = plan(
                &store,
                &harness,
                &git,
                &repo,
                &BranchName::try_new(main)?,
                &plan_text,
                PlanDefaults::default(),
            )
            .await?;
            println!(
                "epic {}  ({} tasks, {} tokens)",
                report.epic,
                report.tasks.len(),
                report.tokens
            );
            for (key, id) in &report.tasks {
                println!("  {id}  {key}");
            }
        }
        Command::Work {
            repo,
            worktrees,
            events,
            main,
            agent,
            lease_ttl,
            max_turns,
            max_budget_usd,
            model,
            interval,
        } => {
            if let Some(dir) = events.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let git = GitCli::new(&repo, &worktrees);
            let log = JsonlSink::open(&events)?;
            let store = BdCli::new(&cli.workdir).with_actor(&agent);
            let mut harness = ClaudeCli::default().with_max_budget_usd(max_budget_usd);
            if let Some(m) = model {
                harness = harness.with_model(m);
            }
            let cfg = WorkerConfig {
                agent: AgentId::try_new(agent)?,
                main: BranchName::try_new(main)?,
                lease_ttl: Duration::from_seconds(lease_ttl),
                max_turns,
            };
            loop {
                match work_once(&store, &git, &harness, &SystemClock, &log, &cfg).await {
                    Ok(Some(report)) => tracing::info!(?report, "task submitted"),
                    Ok(None) => tracing::info!("nothing ready"),
                    // Infrastructure trouble: log and keep polling; the lease will expire if needed.
                    Err(e) => tracing::error!(error = %e, "work failed"),
                }
                let Some(secs) = interval else { break };
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            }
        }
        Command::Verify {
            repo,
            worktrees,
            events,
            interval,
        } => {
            if let Some(dir) = events.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let git = GitCli::new(&repo, &worktrees);
            let log = JsonlSink::open(&events)?;
            let store = BdCli::new(&cli.workdir).with_actor("verifier");
            loop {
                let report =
                    verify_once(&store, &git, &ShellRunner, &SystemClock, &log, "verifier").await?;
                tracing::info!(?report, "verify pass");
                let Some(secs) = interval else { break };
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            }
        }
        Command::Integrate {
            repo,
            worktrees,
            events,
            main,
            remote,
            checks,
            check_timeout,
            interval,
        } => {
            if let Some(dir) = events.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let git = GitCli::new(&repo, &worktrees);
            let log = JsonlSink::open(&events)?;
            let store = BdCli::new(&cli.workdir).with_actor("integrator");
            let cfg = IntegrateConfig {
                main: BranchName::try_new(main)?,
                remote,
                checks,
                check_timeout: Duration::from_seconds(check_timeout),
            };
            loop {
                let report = integrate_once(
                    &store,
                    &git,
                    &ShellRunner,
                    &SystemClock,
                    &log,
                    &cfg,
                    "integrator",
                )
                .await?;
                tracing::info!(?report, "integrate pass");
                let Some(secs) = interval else { break };
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            }
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
