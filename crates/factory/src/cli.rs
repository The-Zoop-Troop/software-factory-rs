//! `factory` command line: argument types, harness wiring, rendering, and dispatch.
//! Kept out of `main.rs` so all of it is unit-testable; `main` only parses and prints.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::unnecessary_wraps,
    clippy::missing_errors_doc
)]
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
use infra::app::Harness;
use infra::app::domain::{AgentId, BeadId, BranchName, Duration, PlanDefaults, TaskState};
use infra::app::{
    Bead, BeadStore, IntegrateConfig, WorkerConfig, integrate_once, plan, verify_once, work_once,
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
            let mut h = OpencodeServer::from_model_spec(&spec).map_err(|e| anyhow::anyhow!(e))?;
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
                max_turns,
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
#[allow(clippy::unwrap_used)]
mod tests {
    use clap::Parser as _;
    use infra::app::domain::{
        AgentId, Budget, Duration, FactoryMeta, Lease, Sha, TaskState, Timestamp, Usage,
    };
    use infra::app::{Bead, BeadStatus};

    use super::*;

    #[test]
    fn parses_every_command() {
        let c = Cli::parse_from(["factory", "version"]);
        assert!(matches!(c.command, Command::Version));
        let c = Cli::parse_from(["factory", "--workdir", "/w", "bead", "show", "fac-1"]);
        assert_eq!(c.workdir, PathBuf::from("/w"));
        assert!(
            matches!(c.command, Command::Bead { command: BeadCommand::Show { ref id } } if id == "fac-1")
        );
        let c = Cli::parse_from([
            "factory",
            "plan",
            "--harness",
            "opencode",
            "--model",
            "p/m",
            "--text",
            "hi",
        ]);
        assert!(matches!(
            c.command,
            Command::Plan {
                harness: HarnessKind::Opencode,
                ..
            }
        ));
        let c = Cli::parse_from([
            "factory",
            "work",
            "--harness",
            "codex",
            "--agent",
            "w9",
            "--lease-ttl",
            "7",
            "--interval",
            "3",
        ]);
        assert!(
            matches!(c.command, Command::Work { harness: HarnessKind::Codex, ref agent, lease_ttl: 7, interval: Some(3), .. } if agent == "w9")
        );
        let c = Cli::parse_from(["factory", "verify", "--repo", "r"]);
        assert!(matches!(c.command, Command::Verify { interval: None, .. }));
        let c = Cli::parse_from([
            "factory",
            "integrate",
            "--check",
            "a",
            "--check",
            "b",
            "--remote",
            "origin",
        ]);
        assert!(
            matches!(c.command, Command::Integrate { ref checks, remote: Some(ref r), .. } if checks.len() == 2 && r == "origin")
        );
        assert!(Cli::try_parse_from(["factory", "bogus"]).is_err());
    }

    #[test]
    fn build_harness_variants() {
        assert!(build_harness(HarnessKind::Claude, Some("m".into()), 1.0).is_ok());
        assert!(build_harness(HarnessKind::Codex, None, 1.0).is_ok());
        assert!(build_harness(HarnessKind::Opencode, Some("p/m".into()), 1.0).is_ok());
        assert!(
            build_harness(HarnessKind::Opencode, None, 1.0).is_err(),
            "opencode needs a model"
        );
        assert!(build_harness(HarnessKind::Opencode, Some("nope".into()), 1.0).is_err());
    }

    fn bead(meta: Option<FactoryMeta>) -> Bead {
        Bead {
            id: infra::app::domain::BeadId::try_new("fac-1").unwrap(),
            title: "t".into(),
            description: String::new(),
            acceptance: Some("acc".into()),
            notes: Some("n1\nn2".into()),
            status: BeadStatus::Open,
            labels: vec![],
            parent: None,
            kind: meta.is_some().then_some(infra::app::domain::BeadKind::Task),
            meta,
            verify: None,
            merge: None,
        }
    }

    fn meta(state: TaskState) -> FactoryMeta {
        FactoryMeta {
            verify_bead: infra::app::domain::BeadId::try_new("fac-2").unwrap(),
            base: Sha::try_new("a".repeat(40)).unwrap(),
            budget: Budget::default(),
            usage: Usage::default(),
            lease_expiries: 0,
            state,
        }
    }

    #[test]
    fn render_covers_every_state() {
        let plain = render(&bead(None));
        assert!(
            plain.contains("(not a factory bead)")
                && plain.contains("accept")
                && plain.contains("    n2")
        );
        let sha = Sha::try_new("b".repeat(40)).unwrap();
        let branch = infra::app::domain::BranchName::try_new("task/fac-1").unwrap();
        let lease = Lease::grant(
            AgentId::try_new("w").unwrap(),
            Timestamp::from_unix_seconds(1),
            Duration::from_seconds(9),
        );
        for (state, needle) in [
            (TaskState::Open, "state     : open"),
            (TaskState::Leased { lease }, "lease     : w until 10"),
            (
                TaskState::InVerify {
                    branch: branch.clone(),
                    head: sha.clone(),
                },
                "branch    : task/fac-1 @",
            ),
            (
                TaskState::Mergeable {
                    branch,
                    head: sha.clone(),
                },
                "branch    : task/fac-1 @",
            ),
            (TaskState::Closed { merged: sha }, "merged    :"),
            (
                TaskState::Incident {
                    reason: infra::app::domain::task::IncidentReason::Manual { detail: "x".into() },
                },
                "incident  :",
            ),
        ] {
            let out = render(&bead(Some(meta(state))));
            assert!(out.contains(needle), "{out}");
            assert!(out.contains("budget    : tokens 0/400000"));
        }
    }

    #[tokio::test]
    async fn run_version_and_missing_plan_text() {
        assert!(run(Cli::parse_from(["factory", "version"])).await.is_ok());
        let err = run(Cli::parse_from(["factory", "plan"])).await.unwrap_err();
        assert!(err.to_string().contains("--text or --file"));
    }
}
