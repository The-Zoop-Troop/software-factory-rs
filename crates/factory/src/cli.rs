//! `factory` command line: argument types, harness wiring, rendering, and dispatch.
//! Kept out of `main.rs` so all of it is unit-testable; `main` only parses and prints.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::unnecessary_wraps,
    clippy::missing_errors_doc,
    clippy::too_many_lines
)]

use std::fmt::Write as _;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use infra::app::Harness;
use infra::app::domain::{AgentId, BeadId, BranchName, Duration, PlanDefaults, TaskState};
use infra::app::{
    Bead, BeadStore, IntegrateConfig, WorkerConfig, inbox, integrate_once, ledger_summary, plan,
    resolve, verify_once, work_once,
};
use infra::{
    BdCli, ClaudeCli, CodexCli, GitCli, JsonlSink, OpencodeServer, ShellRunner, SystemClock,
};

#[derive(Debug, Parser)]
#[command(name = "factory", version, about = "Autonomous AI software factory")]
pub(crate) struct Cli {
    /// Directory containing `.beads/` (defaults to the current directory).
    #[arg(long, global = true, default_value = ".")]
    pub(crate) workdir: PathBuf,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Print build/version information.
    Version,
    /// Inspect beads through the factory's typed view.
    Bead {
        #[command(subcommand)]
        command: BeadCommand,
    },
    /// Check that this host or rig can run the factory (tools, ledger, repo, credentials).
    Doctor {
        /// Path to the project clone.
        #[arg(long, default_value = "repo")]
        repo: PathBuf,
        /// Also send a one-token request through every configured harness (costs a fraction of a cent).
        #[arg(long)]
        probe: bool,
    },
    /// Summarize the ledger: tasks per epic by state, incidents, questions.
    Watch {
        /// Seconds between refreshes; omit to print once.
        #[arg(long)]
        interval: Option<u64>,
    },
    /// Items that need a human: open incidents and questions.
    Inbox {
        /// Resolve this bead (closes it; reopens its task if it was an incident).
        #[arg(long)]
        resolve: Option<String>,
        /// Note recorded with the resolution.
        #[arg(long, default_value = "resolved by operator")]
        note: String,
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
        /// LLM harness behind the Planner.
        #[arg(long, value_enum, default_value_t = HarnessKind::Claude)]
        harness: HarnessKind,
        /// Model: Claude model name, or `provider/model` for opencode.
        #[arg(long)]
        model: Option<String>,
        /// Spend cap for the planner run, USD (claude only).
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
        /// Spend cap per task session, USD (claude only).
        #[arg(long, default_value_t = 5.0)]
        max_budget_usd: f64,
        /// LLM harness behind the Worker.
        #[arg(long, value_enum, default_value_t = HarnessKind::Claude)]
        harness: HarnessKind,
        /// Model: Claude model name, or `provider/model` for opencode.
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

/// Which agent runner executes LLM sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum HarnessKind {
    /// Claude Code headless (`claude -p`).
    Claude,
    /// `OpenCode` headless server (`opencode serve`), any configured provider.
    Opencode,
    /// Codex CLI headless (`codex exec --json`); needs `OPENAI_API_KEY` or a Codex login.
    Codex,
}

/// The single place a harness is chosen and configured.
pub(crate) fn build_harness(
    kind: HarnessKind,
    model: Option<String>,
    max_budget_usd: f64,
) -> anyhow::Result<Box<dyn Harness>> {
    Ok(match kind {
        HarnessKind::Claude => {
            let mut h = ClaudeCli::default().with_max_budget_usd(max_budget_usd);
            if let Some(m) = model {
                h = h.with_model(m);
            }
            Box::new(h)
        }
        HarnessKind::Codex => {
            let mut h = CodexCli::default();
            if let Some(m) = model {
                h = h.with_model(m);
            }
            Box::new(h)
        }
        HarnessKind::Opencode => {
            let spec = model.ok_or_else(|| {
                anyhow::anyhow!("--harness opencode needs --model provider/model")
            })?;
            let mut h = OpencodeServer::from_model_spec(&spec)?;
            if let Ok(cfg) = std::env::var("OPENCODE_CONFIG_CONTENT") {
                h = h.with_config_content(cfg);
            }
            Box::new(h)
        }
    })
}

#[derive(Debug, Subcommand)]
pub(crate) enum BeadCommand {
    /// Show a bead with its factory kind, state, budget and lease decoded.
    Show { id: String },
}

/// Execute a parsed command line. Every adapter is constructed here and nowhere else.
///
/// # Errors
/// Any adapter or workflow failure, already typed; `main` prints it.
#[allow(
    clippy::too_many_lines,
    reason = "one linear dispatch; the single wiring site"
)]
pub(crate) async fn run(cli: Cli) -> anyhow::Result<()> {
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
        Command::Doctor { repo, probe } => {
            let mut checks = crate::doctor::run_checks(&cli.workdir, &repo);
            if probe {
                checks.extend(crate::doctor::probe_harnesses(&cli.workdir).await);
            }
            let (text, ok) = crate::doctor::render(&checks);
            print!("{text}");
            anyhow::ensure!(ok, "doctor found problems (see fixes above)");
        }
        Command::Watch { interval } => loop {
            let s = ledger_summary(&store).await?;
            print!("{}", render_summary(&s));
            let Some(secs) = interval else { break };
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
        },
        Command::Inbox {
            resolve: target,
            note,
        } => {
            if let Some(id) = target {
                let id = BeadId::try_new(id)?;
                match resolve(&store, &id, &note).await? {
                    Some(task) => println!("resolved {id}; reopened {task}"),
                    None => println!("resolved {id}"),
                }
            }
            let items = inbox(&store).await?;
            if items.is_empty() {
                println!("inbox empty");
            }
            for b in items {
                println!(
                    "{}  [{}] {}\n    {}",
                    b.id,
                    b.kind.map_or("?", |k| k.as_str()),
                    b.title,
                    b.description.lines().next().unwrap_or_default()
                );
            }
        }
        Command::Plan {
            repo,
            main,
            file,
            text,
            harness,
            model,
            max_budget_usd,
        } => {
            let plan_text = match (file, text) {
                (Some(f), _) => std::fs::read_to_string(f)?,
                (None, Some(t)) => t,
                (None, None) => anyhow::bail!("give the plan with --text or --file"),
            };
            let harness = build_harness(harness, model, max_budget_usd)?;
            let git = GitCli::new(&repo, repo.join(".factory-worktrees"));
            let store = BdCli::new(&cli.workdir).with_actor("planner");
            let report = plan(
                &store,
                harness.as_ref(),
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
            harness,
            model,
            interval,
        } => {
            if let Some(dir) = events.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let git = GitCli::new(&repo, &worktrees);
            let log = JsonlSink::open(&events)?;
            let store = BdCli::new(&cli.workdir).with_actor(&agent);
            let harness = build_harness(harness, model, max_budget_usd)?;
            let cfg = WorkerConfig {
                agent: AgentId::try_new(agent)?,
                main: BranchName::try_new(main)?,
                lease_ttl: Duration::from_seconds(lease_ttl),
                max_turns: infra::app::domain::Turns::new(max_turns),
            };
            loop {
                match work_once(&store, &git, harness.as_ref(), &SystemClock, &log, &cfg).await {
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
                checks: checks
                    .into_iter()
                    .map(infra::app::domain::VerifyCommand::try_new)
                    .collect::<Result<Vec<_>, _>>()?,
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

/// `factory watch` output.
pub(crate) fn render_summary(s: &infra::app::LedgerSummary) -> String {
    let mut out = String::new();
    for (id, e) in &s.epics {
        let states = e
            .by_state
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            out,
            "{id}  {}  [{}/{}] {states}",
            e.title,
            e.by_state.get("closed").copied().unwrap_or(0),
            e.total
        );
    }
    let _ = writeln!(
        out,
        "tasks outside epics: {}  incidents: {}  questions: {}",
        s.tasks_without_epic, s.open_incidents, s.open_questions
    );
    out
}

pub(crate) fn render(b: &Bead) -> String {
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

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
