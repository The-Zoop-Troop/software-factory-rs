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

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use infra::app::Harness;
use infra::app::domain::{self, AgentId, BeadId, BranchName, Duration};
use infra::app::{
    BeadStore, IntegrateConfig, WorkerConfig, inbox, integrate_once, ledger_summary, resolve,
    verify_once, work_once,
};
use infra::{
    BdCli, CLAUDE_CREDENTIALS, CODEX_CREDENTIALS, ChildEnv, ClaudeCli, CodexCli, Confine, GitCli,
    JsonlSink, OPENCODE_CREDENTIALS, OpencodeServer, Sandbox, ShellRunner, SystemClock,
};

#[derive(Debug, Parser)]
#[command(name = "factory", version, about = "Autonomous AI software factory")]
pub(crate) struct Cli {
    /// Directory containing `.beads/` (defaults to the current directory).
    #[arg(long, global = true, default_value = ".")]
    pub(crate) workdir: PathBuf,
    /// Operate a remote rig through its console instead of a local ledger
    /// (`https://host/rigs/<name>`); applies to watch, inbox, plan, stop, doctor.
    #[arg(long, global = true, env = "FACTORY_RIG")]
    pub(crate) rig: Option<String>,
    /// Bearer token for --rig.
    #[arg(long, global = true, env = "FACTORY_TOKEN", hide_env_values = true)]
    pub(crate) token: Option<String>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Print build/version information.
    Version,
    /// Throughput report: stage timings, critical path, concurrency (log file, or --rig).
    Metrics {
        #[arg(long)]
        epic: Option<String>,
        #[arg(long, default_value = ".factory/events.jsonl")]
        events: PathBuf,
        #[arg(long, conflicts_with = "csv")]
        json: bool,
        /// One CSV row per stage per epic (--json: pretty JSON).
        #[arg(long)]
        csv: bool,
    },
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
        /// Event log path (JSONL, appended) for the reopened task's transition.
        #[arg(long, default_value = ".factory/events.jsonl")]
        events: PathBuf,
    },
    /// Manage rigs on this host: one compose project per rig, one console over all of them.
    Rig {
        /// Where rig files live (registry, per-rig env and secrets, console config); `~` expands.
        #[arg(long, env = "FACTORY_ROOT", default_value = "~/.factory")]
        root: PathBuf,
        /// The shared rig compose file.
        #[arg(long, env = "FACTORY_COMPOSE", default_value = "compose.yaml")]
        compose: PathBuf,
        #[command(subcommand)]
        command: crate::rig::RigCommand,
    },
    /// Cancel an epic through the console: its open tasks are closed (needs --rig).
    Stop {
        /// The epic id.
        epic: String,
    },
    /// Run a Telegram bot over a remote rig (long polling; needs --rig and --token).
    Telegram {
        /// Bot token from `@BotFather`.
        #[arg(long, env = "TELEGRAM_BOT_TOKEN", hide_env_values = true)]
        bot_token: String,
        /// Chat ids allowed to talk to the bot (repeatable); others are ignored.
        #[arg(long = "chat", required = true)]
        chats: Vec<i64>,
        /// Seconds between task polls for push notifications.
        #[arg(long, default_value_t = 30)]
        poll: u64,
        /// Telegram API base (tests point this at a local server).
        #[arg(long, default_value = "https://api.telegram.org", hide = true)]
        api_base: String,
    },
    /// Run the Planner: turn a plan (text or file) into an epic of task + verify beads.
    Plan(crate::plan_cmd::PlanArgs),
    /// Internal: run a program that may write only beneath the given directories (Landlock).
    /// Roles wrap sessions and verify commands with this; the role itself stays unconfined.
    #[command(hide = true)]
    Confine {
        /// A directory the program may write beneath; repeat for each.
        #[arg(long = "write")]
        write: Vec<PathBuf>,
        /// The program and its arguments, after `--`.
        #[arg(last = true, required = true)]
        program: Vec<std::ffi::OsString>,
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
        /// Seconds to wait after the provider refuses a session (a usage or spend limit, or an
        /// outage). A refused request costs nothing, so trying again is safe.
        #[arg(long, env = "RIG_LIMIT_WAIT", default_value_t = 900)]
        limit_wait: u64,
        /// LLM harness behind the Worker.
        #[arg(long, value_enum, default_value_t = HarnessKind::Claude)]
        harness: HarnessKind,
        /// Model: Claude model name, or `provider/model` for opencode.
        #[arg(long)]
        model: Option<String>,
        /// Thinking effort: low | medium | high | max (harness default when omitted).
        #[arg(long, env = "RIG_EFFORT")]
        effort: Option<String>,
        /// Seconds to wait when nothing is ready; omit to run one task (or none) and exit.
        #[arg(long)]
        interval: Option<u64>,
    },
    /// Run the Verifier: check every task awaiting verification.
    Verify {
        /// Path to the project clone.
        #[arg(long, default_value = "repo")]
        repo: PathBuf,
        /// Integration branch; the Verifier's own configuration is read from it, never from the
        /// branch under test.
        #[arg(long, default_value = "main")]
        main: String,
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
        /// Branches the factory must never integrate into or push (comma-separated).
        #[arg(
            long,
            env = "RIG_PROTECTED_BRANCHES",
            default_value = "main,master",
            value_delimiter = ','
        )]
        protected: Vec<String>,
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

/// `~/x` → `$HOME/x`; anything else unchanged.
pub(crate) fn expand_home(p: PathBuf) -> PathBuf {
    match (p.strip_prefix("~"), std::env::var_os("HOME")) {
        (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
        (Ok(_), None) | (Err(_), _) => p,
    }
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

/// The single place a harness is chosen and configured. Sessions start with this process's
/// environment minus its secrets (keeping the harness's own credential), confined if `confine`.
pub(crate) fn build_harness(
    kind: HarnessKind,
    model: Option<String>,
    max_budget_usd: f64,
    confine: Option<&Confine>,
) -> anyhow::Result<Box<dyn Harness>> {
    let sandbox = |keep: &[&str]| Sandbox {
        env: ChildEnv::from_process(keep),
        confine: confine.cloned(),
    };
    Ok(match kind {
        HarnessKind::Claude => {
            let mut h = ClaudeCli::default()
                .with_max_budget_usd(max_budget_usd)
                .with_sandbox(sandbox(&CLAUDE_CREDENTIALS));
            if let Some(m) = model {
                h = h.with_model(m);
            }
            Box::new(h)
        }
        HarnessKind::Codex => {
            let mut h = CodexCli::default().with_sandbox(sandbox(&CODEX_CREDENTIALS));
            if let Some(m) = model {
                h = h.with_model(m);
            }
            Box::new(h)
        }
        HarnessKind::Opencode => {
            let spec = model.ok_or_else(|| {
                anyhow::anyhow!("--harness opencode needs --model provider/model")
            })?;
            let mut h = OpencodeServer::from_model_spec(&spec)?
                .with_sandbox(sandbox(&OPENCODE_CREDENTIALS));
            if let Ok(cfg) = std::env::var("OPENCODE_CONFIG_CONTENT") {
                h = h.with_config_content(cfg);
            }
            Box::new(h)
        }
    })
}

/// How this role confines its untrusted children, proven to work here before the role starts:
/// `None` only when `RIG_CONFINE=off`.
pub(crate) async fn role_confine() -> anyhow::Result<Option<Confine>> {
    let env = ChildEnv::from_process(&[]);
    let Some(confine) = Confine::from_env(std::env::current_exe()?, &env) else {
        tracing::warn!("RIG_CONFINE=off: sessions and verify commands run unconfined");
        return Ok(None);
    };
    infra::confine::probe(&confine).await?;
    Ok(Some(confine))
}

/// The candidate's commands, as the Verifier and Integrator run them.
fn untrusted_runner(confine: Option<Confine>) -> ShellRunner {
    ShellRunner::with_sandbox(Sandbox {
        env: ChildEnv::from_process(&[]),
        confine,
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
pub(crate) use crate::render::{render, render_summary};

/// Seconds until a polling role looks again, or `None` for a one-shot run. A provider refusal gets
/// the longer wait, because each try before it clears would only be refused again.
pub(crate) fn next_poll(stopped: bool, interval: Option<u64>, limit_wait: u64) -> Option<u64> {
    interval.map(|secs| if stopped { secs.max(limit_wait) } else { secs })
}

pub(crate) async fn run(cli: Cli) -> anyhow::Result<()> {
    if let Some(rig) = cli.rig.as_deref() {
        let token = cli
            .token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--rig needs --token (or FACTORY_TOKEN)"))?;
        let api = infra::A2aHttp::new(rig, token)?;
        return crate::remote::run_remote(&api, cli.command).await;
    }
    let store = BdCli::new(&cli.workdir).with_actor("factory");
    match cli.command {
        Command::Stop { .. } | Command::Telegram { .. } => {
            anyhow::bail!("this command operates a remote rig: add --rig <url> --token <token>")
        }
        Command::Rig {
            root,
            compose,
            command,
        } => {
            let layout = crate::rig::Layout {
                root: expand_home(root),
                compose_file: compose,
            };
            print!(
                "{}",
                crate::rig::run(&infra::DockerCli::default(), &layout, command).await?
            );
        }
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
            checks.push(crate::doctor::confine_check().await);
            if probe {
                checks.extend(crate::doctor::probe_harnesses(&cli.workdir).await);
            }
            let (text, ok) = crate::doctor::render(&checks);
            print!("{text}");
            anyhow::ensure!(ok, "doctor found problems (see fixes above)");
        }
        Command::Metrics {
            epic,
            events,
            json,
            csv,
        } => print!(
            "{}",
            crate::metrics::from_file(&cli.workdir.join(&events), epic.as_deref(), json, csv)?
        ),
        Command::Watch { interval } => loop {
            let s = ledger_summary(&store).await?;
            print!("{}", render_summary(&s));
            let Some(secs) = interval else { break };
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
        },
        Command::Inbox {
            resolve: target,
            note,
            events,
        } => {
            if let Some(id) = target {
                let id = BeadId::try_new(id)?;
                let events = cli.workdir.join(events);
                if let Some(dir) = events.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let log = JsonlSink::open(&events)?;
                match resolve(&store, &log, &SystemClock, "operator", &id, &note).await? {
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
        Command::Plan(args) => crate::plan_cmd::run(&cli.workdir, args).await?,
        Command::Confine { write, program } => {
            let (bin, args) = program
                .split_first()
                .ok_or_else(|| anyhow::anyhow!("confine needs a program after --"))?;
            infra::confine_self(&write)?;
            // exec returns only on failure.
            let err = std::os::unix::process::CommandExt::exec(
                std::process::Command::new(bin).args(args),
            );
            return Err(std::io::Error::new(
                err.kind(),
                format!("exec {}: {err}", bin.to_string_lossy()),
            )
            .into());
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
            limit_wait,
            harness,
            model,
            effort,
            interval,
        } => {
            let effort = effort.map(|e| e.parse::<domain::Effort>()).transpose()?;
            if let Some(dir) = events.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let git = GitCli::new(&repo, &worktrees);
            let log = JsonlSink::open(&events)?;
            let store = BdCli::new(&cli.workdir).with_actor(&agent);
            let harness = build_harness(
                harness,
                model,
                max_budget_usd,
                role_confine().await?.as_ref(),
            )?;
            let cfg = WorkerConfig {
                agent: AgentId::try_new(agent)?,
                main: BranchName::try_new(main)?,
                lease_ttl: Duration::from_seconds(lease_ttl),
                max_turns: infra::app::domain::Turns::new(max_turns),
                effort,
            };
            loop {
                let mut stopped = false;
                match work_once(&store, &git, harness.as_ref(), &SystemClock, &log, &cfg).await {
                    Ok(Some(report)) => tracing::info!(?report, "task submitted"),
                    Ok(None) => tracing::info!("nothing ready"),
                    Err(e @ app::WorkerError::ProviderStopped { .. }) => {
                        stopped = true;
                        tracing::warn!(error = %e, limit_wait, "provider refused the session; waiting");
                    }
                    // Infrastructure trouble: log and keep polling; the lease will expire if needed.
                    Err(e) => tracing::error!(error = %e, "work failed"),
                }
                let Some(secs) = next_poll(stopped, interval, limit_wait) else {
                    break;
                };
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            }
        }
        Command::Verify {
            repo,
            main,
            worktrees,
            events,
            interval,
        } => {
            let main = BranchName::try_new(main)?;
            if let Some(dir) = events.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let git = GitCli::new(&repo, &worktrees);
            let log = JsonlSink::open(&events)?;
            let store = BdCli::new(&cli.workdir).with_actor("verifier");
            let runner = untrusted_runner(role_confine().await?);
            loop {
                let report =
                    verify_once(&store, &git, &runner, &SystemClock, &log, "verifier", &main)
                        .await?;
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
            protected,
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
                protected: protected
                    .iter()
                    .filter(|p| !p.trim().is_empty())
                    .map(|p| BranchName::try_new(p.trim()))
                    .collect::<Result<Vec<_>, _>>()?,
                remote,
                checks: checks
                    .into_iter()
                    .map(infra::app::domain::VerifyCommand::try_new)
                    .collect::<Result<Vec<_>, _>>()?,
                check_timeout: Duration::from_seconds(check_timeout),
            };
            let runner = untrusted_runner(role_confine().await?);
            loop {
                let report = integrate_once(
                    &store,
                    &git,
                    &runner,
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

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
