//! The `factory plan` command: run the Planner inline or serve the plan queue.
//! Split from `cli.rs` so the command stays under the file-size taste cap as it grows.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::unnecessary_wraps,
    clippy::missing_errors_doc,
    clippy::too_many_lines
)]

use std::path::{Path, PathBuf};

use infra::app::domain::{self, BranchName, PlanDefaults};
use infra::app::{self, plan};
use infra::{BdCli, GitCli, JsonlSink, SystemClock};

use crate::cli::{HarnessKind, build_harness};

/// Arguments for `factory plan`; the subcommand doc lives on [`crate::cli::Command`].
#[derive(Debug, clap::Args)]
pub(crate) struct PlanArgs {
    /// Path to the project clone (the Planner reads it for context in later phases).
    #[arg(long, default_value = "repo")]
    pub(crate) repo: PathBuf,
    /// Integration branch; tasks are cut from its current tip.
    #[arg(long, default_value = "main")]
    pub(crate) main: String,
    /// Read the plan from this file instead of --text.
    #[arg(long, conflicts_with = "text")]
    pub(crate) file: Option<PathBuf>,
    /// The plan, inline.
    #[arg(long)]
    pub(crate) text: Option<String>,
    /// `rig:epic` this plan waits for (with --rig or --queued).
    #[arg(long = "after")]
    pub(crate) after: Vec<String>,
    /// LLM harness behind the Planner.
    #[arg(long, value_enum, default_value_t = HarnessKind::Claude)]
    pub(crate) harness: HarnessKind,
    /// Model: Claude model name, or `provider/model` for opencode.
    #[arg(long)]
    pub(crate) model: Option<String>,
    /// Thinking effort: low | medium | high | max (harness default when omitted).
    #[arg(long, env = "RIG_EFFORT")]
    pub(crate) effort: Option<String>,
    /// Token budget per task the Planner writes onto new tasks (default 400000).
    #[arg(long, env = "RIG_TASK_TOKENS")]
    pub(crate) task_tokens: Option<u64>,
    /// Spend cap for the planner run, USD (claude only).
    #[arg(long, default_value_t = 2.0)]
    pub(crate) max_budget_usd: f64,
    /// Serve the plan queue instead: plan each open `plan_request` bead (from the console).
    #[arg(long, conflicts_with_all = ["text", "file"])]
    pub(crate) queue: bool,
    /// Queue the plan as a `plan_request` bead on this rig's ledger instead of planning
    /// inline: the planner service (`--queue`) picks it up, and the console shows it as a
    /// request card. With `--after`, the request waits for those epics first.
    #[arg(long, conflicts_with = "queue")]
    pub(crate) queued: bool,
    /// With --queue: keep polling every N seconds (one sweep when omitted).
    #[arg(long)]
    pub(crate) interval: Option<u64>,
    /// With --queue: event log path (JSONL, appended) for planner progress.
    #[arg(long, default_value = ".factory/events.jsonl")]
    pub(crate) events: PathBuf,
}

pub(crate) async fn run(workdir: &Path, args: PlanArgs) -> anyhow::Result<()> {
    let PlanArgs {
        repo,
        main,
        file,
        text,
        after,
        harness,
        model,
        effort,
        task_tokens,
        max_budget_usd,
        queue,
        queued,
        interval,
        events,
    } = args;
    if queued {
        let plan_text = read_plan_text(file, text)?;
        let needs = after
            .iter()
            .map(|s| crate::remote::parse_need(s))
            .collect::<Result<Vec<_>, _>>()?;
        let store = BdCli::new(workdir).with_actor("operator");
        let waiting = !needs.is_empty();
        let id = app::submit_plan_request(&store, &plan_text, "cli", needs).await?;
        if waiting {
            println!("request {id} waiting on {}", after.join(", "));
        } else {
            println!("request {id} queued; the rig's planner service will take it");
        }
        return Ok(());
    }
    if !after.is_empty() {
        anyhow::bail!("--after needs --queued (queue on this rig) or --rig (queue on a console)");
    }
    let effort = effort.map(|e| e.parse::<domain::Effort>()).transpose()?;
    let mut defaults = PlanDefaults {
        effort,
        ..PlanDefaults::default()
    };
    if let Some(t) = task_tokens {
        defaults.budget.tokens = domain::Tokens::new(t);
    }
    if queue {
        let harness = build_harness(harness, model, max_budget_usd)?;
        let git = GitCli::new(&repo, repo.join(".factory-worktrees"));
        let store = BdCli::new(workdir).with_actor("planner");
        let main = BranchName::try_new(main)?;
        if let Some(dir) = events.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let log = JsonlSink::open(&events)?;
        loop {
            let out = app::plan_queued_once(
                &store,
                harness.as_ref(),
                &git,
                &repo,
                &main,
                defaults,
                app::Progress {
                    sink: &log,
                    clock: &SystemClock,
                },
            )
            .await?;
            if let Some(o) = out {
                let line = match o.result {
                    Ok(r) => format!("epic {} ({} tasks)", r.epic, r.tasks.len()),
                    Err(e) => format!("failed: {e}"),
                };
                println!("{}: {line}", o.request);
            }
            let Some(secs) = interval else { break };
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
        }
        return Ok(());
    }
    let plan_text = read_plan_text(file, text)?;
    let harness = build_harness(harness, model, max_budget_usd)?;
    let git = GitCli::new(&repo, repo.join(".factory-worktrees"));
    let store = BdCli::new(workdir).with_actor("planner");
    let report = plan(
        &store,
        harness.as_ref(),
        &git,
        &repo,
        &BranchName::try_new(main)?,
        &plan_text,
        defaults,
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
    Ok(())
}

fn read_plan_text(file: Option<PathBuf>, text: Option<String>) -> anyhow::Result<String> {
    match (file, text) {
        (Some(f), _) => Ok(std::fs::read_to_string(f)?),
        (None, Some(t)) => Ok(t),
        (None, None) => anyhow::bail!("give the plan with --text or --file"),
    }
}
