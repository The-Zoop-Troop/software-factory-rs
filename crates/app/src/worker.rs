//! The Worker (ARCHITECTURE.md §4.2, §8): stateless. Claim one ready task, cut a branch,
//! hand a fresh harness a curated context packet, commit whatever it did, submit for
//! verification. It never decides it is done; the Verifier does.

use std::fmt::Write as _;

use domain::{
    AgentId, Attempts, BeadId, BeadKind, BranchName, Duration, Event, Task, TaskState, Tokens,
    Turns, VerifyCommand,
};
use futures::FutureExt as _;
use futures::future::{Either, select};

use crate::bead::Bead;
use crate::events::{EventKind, FactoryEvent};
use crate::ports::{
    BeadStore, Clock, EventSink, Harness, HarnessRequest, Repo, RepoError, StoreError, ToolPolicy,
};
use crate::transition::{TransitionError, apply_event};

/// How this worker behaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub agent: AgentId,
    pub main: BranchName,
    pub lease_ttl: Duration,
    /// Harness turn cap per session.
    pub max_turns: Turns,
    /// Thinking effort for worker sessions (`None` = harness default).
    pub effort: Option<domain::Effort>,
}

/// The agent's `FACTORY_BLOCKED.md`, if it wrote one: its text, and the file is gone.
fn take_blocked_note(worktree: &std::path::Path) -> Option<String> {
    let path = worktree.join("FACTORY_BLOCKED.md");
    let text = std::fs::read_to_string(&path).ok()?;
    if let Err(e) = std::fs::remove_file(&path) {
        tracing::warn!(error = %e, "could not remove FACTORY_BLOCKED.md");
    }
    Some(text.trim().to_owned())
}

/// The last `resume-from: <branch>` marker in the notes, if any (see `remote::service`).
#[must_use]
pub fn resume_branch(notes: Option<&str>) -> Option<BranchName> {
    notes?
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("resume-from: "))
        .and_then(|b| BranchName::try_new(b.trim()).ok())
}

/// What one session did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkReport {
    pub task: BeadId,
    pub branch: BranchName,
    pub head: domain::Sha,
    pub tokens: Tokens,
    pub turns: Turns,
}

/// Worker failures. A claimed task is released (lease left to expire) on any error after claim.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkerError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Transition(#[from] TransitionError),
    #[error("repo: {0}")]
    Repo(#[from] RepoError),
    #[error("harness: {0}")]
    Harness(#[from] crate::ports::HarnessError),
}

/// Claim and work one task. `Ok(None)` when nothing is ready.
///
/// # Errors
/// Ledger, repo or harness infrastructure failures. Model-level failure is not an error:
/// whatever the session produced is submitted and the Verifier judges it.
#[tracing::instrument(skip_all, fields(agent = %cfg.agent), err)]
#[allow(
    clippy::too_many_lines,
    reason = "one linear session; splitting would hide the order of effects"
)]
pub async fn work_once(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    harness: &dyn Harness,
    clock: &dyn Clock,
    log: &dyn EventSink,
    cfg: &WorkerConfig,
) -> Result<Option<WorkReport>, WorkerError> {
    let Some(bead) = pick(store).await? else {
        return Ok(None);
    };
    let id = bead.id.clone();
    let now = clock.now();
    let tr = apply_event(
        store,
        &id,
        Event::Claim {
            holder: cfg.agent.clone(),
            now,
            ttl: cfg.lease_ttl,
        },
    )
    .await?;
    log.record(&event(
        clock,
        &cfg.agent,
        &id,
        EventKind::Claimed {
            holder: cfg.agent.to_string(),
        },
    ));

    let packet = ContextPacket::build(store, &bead, &tr.task).await?;
    let branch = BranchName::for_task(&id).map_err(|e| RepoError::Rejected {
        op: crate::ports::GitOp::WorktreeAdd,
        detail: e.to_string(),
    })?;
    // An operator may ask to continue from the task's existing branch (environment incidents).
    let base = resume_branch(bead.notes.as_deref()).unwrap_or_else(|| cfg.main.clone());
    let from = repo.head_of(&base).await?;
    let worktree = repo.branch_worktree(&branch, &from).await?;

    let remaining = remaining_wall_clock(&tr.task);
    let mcp = match crate::mcp::McpConfig::load(&worktree.path) {
        Ok(m) => m,
        Err(e) => {
            store
                .note(&id, &format!("ignoring .factory/mcp.json: {e}"))
                .await?;
            crate::mcp::McpConfig::default()
        }
    };
    let session = harness.run(HarnessRequest {
        cwd: worktree.path.clone(),
        system_prompt: WORKER_SYSTEM_PROMPT.to_owned(),
        prompt: packet.render(),
        schema: None,
        tools: ToolPolicy::Full,
        mcp,
        max_turns: cfg.max_turns,
        effort: cfg.effort,
        timeout: remaining,
    });
    let outcome = run_with_heartbeats(store, log, repo, &worktree, clock, cfg, &id, session).await;

    // The agent's blocked note is for the ledger, never for the repository: read it, then remove
    // it before anything is committed.
    let blocked = take_blocked_note(&worktree.path);

    // Whatever happened, tidy the worktree; the branch (and any commits) survive.
    let commit = repo
        .commit_all(&worktree, &format!("{}: {}", id, bead.title))
        .await;
    if let Err(e) = repo.worktree_remove(worktree).await {
        tracing::warn!(error = %e, "worktree left behind; the next claim recreates it");
    }
    let head = commit?;
    let outcome = outcome?;

    let now = clock.now();
    // A session that changed nothing has nothing to verify — whether it errored or merely talked —
    // and a session that declared itself blocked must not be verified either. Give the task back
    // (an attempt) and keep the reason so the incident is legible.
    if head == from || blocked.is_some() {
        let why = match (&blocked, outcome.is_error) {
            (Some(_), _) => "blocked",
            (None, true) => "harness error",
            (None, false) => "no changes made",
        };
        let reason = blocked.clone().unwrap_or_else(|| outcome.text.clone());
        let note = format!(
            "released: {why}: {}",
            reason.chars().take(600).collect::<String>()
        );
        apply_event(
            store,
            &id,
            Event::Release {
                holder: cfg.agent.clone(),
                now,
                note,
            },
        )
        .await?;
        log.record(&event(
            clock,
            &cfg.agent,
            &id,
            EventKind::Released {
                holder: cfg.agent.to_string(),
                detail: reason.chars().take(200).collect(),
            },
        ));
        return Ok(None);
    }
    apply_event(
        store,
        &id,
        Event::Submit {
            holder: cfg.agent.clone(),
            branch: branch.clone(),
            head: head.clone(),
            now,
            tokens: outcome.tokens,
        },
    )
    .await?;
    if outcome.is_error {
        store
            .note(&id, &format!("harness reported an error: {}", outcome.text))
            .await?;
    }
    log.record(&event(
        clock,
        &cfg.agent,
        &id,
        EventKind::Submitted {
            holder: cfg.agent.to_string(),
            tokens: outcome.tokens,
            turns: outcome.turns,
            wall_clock: clock.now().since(now),
            head: head.clone(),
        },
    ));
    Ok(Some(WorkReport {
        task: id,
        branch,
        head,
        tokens: outcome.tokens,
        turns: outcome.turns,
    }))
}

/// First ready task bead in `Open` state.
async fn pick(store: &dyn BeadStore) -> Result<Option<Bead>, StoreError> {
    Ok(store
        .ready(BeadKind::Task)
        .await?
        .into_iter()
        .find(|b| matches!(b.meta.as_ref().map(|m| &m.state), Some(TaskState::Open))))
}

fn remaining_wall_clock(task: &Task) -> Duration {
    Duration::from_seconds(
        task.budget
            .wall_clock
            .seconds()
            .saturating_sub(task.usage.wall_clock.seconds())
            .max(60),
    )
}

/// Drive the harness while renewing the lease every `ttl / 3`, reporting worktree drift each time.
#[allow(
    clippy::too_many_arguments,
    reason = "the ports the session needs; no struct outlives it"
)]
async fn run_with_heartbeats<F>(
    store: &dyn BeadStore,
    log: &dyn EventSink,
    repo: &dyn Repo,
    worktree: &crate::ports::Worktree,
    clock: &dyn Clock,
    cfg: &WorkerConfig,
    id: &BeadId,
    session: F,
) -> Result<crate::ports::HarnessOutcome, crate::ports::HarnessError>
where
    F: Future<Output = Result<crate::ports::HarnessOutcome, crate::ports::HarnessError>> + Send,
{
    let interval =
        Duration::from_seconds(cfg.lease_ttl.seconds() / 3).max(Duration::from_seconds(1));
    let mut session = Box::pin(session);
    loop {
        let tick = Box::pin(clock.sleep(interval).fuse());
        match select(session, tick).await {
            Either::Left((outcome, _)) => return outcome,
            Either::Right(((), s)) => {
                session = s;
                let hb = Event::Heartbeat {
                    holder: cfg.agent.clone(),
                    now: clock.now(),
                };
                if let Err(e) = apply_event(store, id, hb).await {
                    // A lost lease means the Steward reassigned us; stop feeding a zombie session.
                    tracing::warn!(error = %e, "heartbeat failed; abandoning session");
                    return Err(crate::ports::HarnessError::LeaseLost {
                        detail: e.to_string(),
                    });
                }
                // Progress is a courtesy to the operator; a failed sample is not a failed session.
                match repo.diff_stat(worktree).await {
                    Ok(stat) => log.record(&event(
                        clock,
                        &cfg.agent,
                        id,
                        EventKind::Progress {
                            files: stat.files,
                            insertions: stat.insertions,
                            deletions: stat.deletions,
                        },
                    )),
                    Err(e) => tracing::debug!(error = %e, "progress sample skipped"),
                }
            }
        }
    }
}

/// Everything the harness gets to see (ARCHITECTURE.md §8). Small, deterministic.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ContextPacket {
    task_id: BeadId,
    title: String,
    description: String,
    acceptance: Option<String>,
    verify_commands: Vec<VerifyCommand>,
    prior_notes: Option<String>,
    references: Vec<String>,
    attempt: Attempts,
    attempts_allowed: Attempts,
}

impl ContextPacket {
    async fn build(store: &dyn BeadStore, bead: &Bead, task: &Task) -> Result<Self, StoreError> {
        let verify_commands = store
            .show(&task.verify_bead)
            .await?
            .verify
            .map(|v| Vec::from(v.commands))
            .unwrap_or_default();
        let references = match &bead.parent {
            Some(epic) => store
                .children(epic)
                .await?
                .into_iter()
                .filter(|c| c.kind == Some(BeadKind::Reference))
                .map(|c| c.description)
                .collect(),
            None => vec![],
        };
        Ok(Self {
            task_id: bead.id.clone(),
            title: bead.title.clone(),
            description: bead.description.clone(),
            acceptance: bead.acceptance.clone(),
            verify_commands,
            prior_notes: bead.notes.clone(),
            references,
            attempt: task.usage.attempts.incr(),
            attempts_allowed: task.budget.attempts,
        })
    }

    fn render(&self) -> String {
        let mut p = String::new();
        let _ = writeln!(p, "# Task {}: {}\n", self.task_id, self.title);
        let _ = writeln!(p, "{}\n", self.description);
        if let Some(a) = &self.acceptance {
            let _ = writeln!(p, "## Acceptance criteria\n{a}\n");
        }
        if !self.verify_commands.is_empty() {
            let _ = writeln!(
                p,
                "## Verification (run from the repo root; all must exit 0)"
            );
            for c in &self.verify_commands {
                let _ = writeln!(p, "    {c}");
            }
            p.push('\n');
        }
        if !self.references.is_empty() {
            let _ = writeln!(p, "## Project reference");
            for r in &self.references {
                let _ = writeln!(p, "{r}\n");
            }
        }
        if let Some(n) = &self.prior_notes {
            let _ = writeln!(
                p,
                "## Previous attempts (this is attempt {} of {})\nRead this before touching anything:\n{n}\n",
                self.attempt, self.attempts_allowed
            );
        }
        p
    }
}

const WORKER_SYSTEM_PROMPT: &str = "\
You are a worker in an autonomous software factory. You are in a fresh git worktree on your \
own branch; nothing outside this directory matters and no one else is editing it. Complete the \
task described in the message, then stop.

Rules:
- Do exactly the task. Do not refactor unrelated code, add features, or change tooling.
- Before you stop, run the listed verification commands yourself from the repo root and make them pass.
- Do not commit, push, or create branches; the factory commits your working tree when you exit.
- If the task is impossible as written or you are missing information, write a short file named \
FACTORY_BLOCKED.md at the repo root explaining exactly what is needed, then stop; the factory \
reads it into the task's notes and removes it — it is never committed.
- Do not read or modify anything under .factory/ or .beads/.\
Environment: you are inside a sandbox with the repository, its toolchain and package \
registries only — no docker, no cloud CLIs, no credentials, no external services; /tmp is not \
executable. Do not add steps that need them; make tests hermetic.\
";

fn event(clock: &dyn Clock, actor: &AgentId, bead: &BeadId, kind: EventKind) -> FactoryEvent {
    FactoryEvent {
        at: clock.now(),
        actor: actor.to_string(),
        bead: Some(bead.clone()),
        kind,
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
