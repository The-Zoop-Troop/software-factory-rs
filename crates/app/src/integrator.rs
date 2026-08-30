//! The Integrator v0 (ARCHITECTURE.md §4.4): land verified branches on `main` one at a
//! time — rebase, run the project's own check suite, fast-forward, push. The only
//! component that pushes to the remote. Batch-then-bisect is Phase 1.

use domain::{BeadId, BeadKind, BranchName, Duration, Event, MergeMeta, TaskState, VerifyCommand};

use crate::events::{EventKind, FactoryEvent};
use crate::ports::{BeadStore, Clock, EventSink, Repo, RepoError, RunError, Runner, StoreError};
use crate::transition::{TransitionError, apply_event, load_task};

/// How to integrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrateConfig {
    pub main: BranchName,
    /// Remote to push `main` to after landing; `None` for local-only rigs.
    pub remote: Option<String>,
    /// Project-wide checks run on the rebased head before it lands. Empty = trust the verify bead.
    pub checks: Vec<VerifyCommand>,
    pub check_timeout: Duration,
    /// Branches the factory must never integrate into or push (`main`, `master` by default).
    /// A rig whose integration branch is protected refuses to run at all.
    pub protected: Vec<BranchName>,
}

/// What one integration pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IntegrateReport {
    pub landed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
}

/// Failure to list merge beads; per-bead failures are counted in the report.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IntegratorError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(
        "integration branch `{branch}` is protected; the factory never lands on it (set RIG_MAIN / --main to a feature branch, or change --protected deliberately)"
    )]
    ProtectedBranch { branch: BranchName },
}

/// Land every open merge bead whose task is `mergeable`, in listing order.
///
/// # Errors
/// Only when the ledger cannot be listed.
#[tracing::instrument(skip_all, err)]
pub async fn integrate_once(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    runner: &dyn Runner,
    clock: &dyn Clock,
    log: &dyn EventSink,
    cfg: &IntegrateConfig,
    actor: &str,
) -> Result<IntegrateReport, IntegratorError> {
    if cfg.protected.contains(&cfg.main) {
        return Err(IntegratorError::ProtectedBranch {
            branch: cfg.main.clone(),
        });
    }
    let mut report = IntegrateReport::default();
    for bead in store.list_active(BeadKind::Merge).await? {
        let Some(meta) = bead.merge.clone() else {
            report.skipped += 1;
            continue;
        };
        let outcome =
            integrate_one(store, repo, runner, clock, log, actor, cfg, &bead.id, &meta).await;
        let kind = match &outcome {
            Ok(Some(Landed::Yes(sha))) => {
                report.landed += 1;
                EventKind::Integrated {
                    merge_bead: bead.id.clone(),
                    landed: Some(sha.clone()),
                    rejection: None,
                }
            }
            Ok(Some(Landed::No(rejection))) => {
                report.failed += 1;
                EventKind::Integrated {
                    merge_bead: bead.id.clone(),
                    landed: None,
                    rejection: Some(rejection.clone()),
                }
            }
            Ok(None) => {
                report.skipped += 1;
                continue;
            }
            Err(e) => {
                report.errors += 1;
                EventKind::Error {
                    detail: e.to_string(),
                }
            }
        };
        log.record(&FactoryEvent {
            at: clock.now(),
            actor: actor.to_owned(),
            bead: Some(meta.task.clone()),
            kind,
        });
    }
    Ok(report)
}

/// Why a branch could not land. Reopens the task with this as the note.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum LandRejection {
    Conflict {
        onto: BranchName,
        paths: Vec<std::path::PathBuf>,
    },
    /// The local integration branch and the remote each have commits the other lacks; the
    /// operator reconciles (nothing is force-pushed or guessed).
    Diverged {
        branch: BranchName,
        local: domain::Sha,
        remote: domain::Sha,
    },
    CheckFailed {
        command: VerifyCommand,
        exit_code: Option<i32>,
        timed_out: bool,
        tail: String,
    },
}

impl std::fmt::Display for LandRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diverged {
                branch,
                local,
                remote,
            } => write!(
                f,
                "integration branch `{branch}` diverged from the remote (local {local}, remote {remote}); reconcile by hand — nothing was pushed"
            ),
            Self::Conflict { onto, paths } => {
                write!(f, "rebase onto {onto} conflicted in {paths:?}")
            }
            Self::CheckFailed {
                command,
                exit_code,
                timed_out,
                tail,
            } => {
                write!(
                    f,
                    "check `{command}` failed after rebase (exit {exit_code:?}{})",
                    if *timed_out { ", timed out" } else { "" }
                )?;
                if !tail.is_empty() {
                    write!(f, "\n{tail}")?;
                }
                Ok(())
            }
        }
    }
}

enum Landed {
    Yes(domain::Sha),
    No(LandRejection),
}

/// `None` if the task is no longer mergeable (stale merge bead — closed as such).
#[allow(
    clippy::too_many_arguments,
    reason = "ports plus the stage identity; nothing outlives the call"
)]
async fn integrate_one(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    runner: &dyn Runner,
    clock: &dyn Clock,
    log: &dyn EventSink,
    actor: &str,
    cfg: &IntegrateConfig,
    merge_bead: &BeadId,
    meta: &MergeMeta,
) -> Result<Option<Landed>, TransitionError> {
    let task = load_task(store, &meta.task).await?;
    let TaskState::Mergeable { branch, head } = &task.state else {
        store
            .close(merge_bead, "task no longer mergeable; stale merge bead")
            .await?;
        return Ok(None);
    };
    if branch != &meta.branch || head != &meta.head {
        store
            .close(
                merge_bead,
                "merge bead does not match task's current branch/head",
            )
            .await?;
        return Ok(None);
    }
    log.record(&FactoryEvent {
        at: clock.now(),
        actor: actor.to_owned(),
        bead: Some(meta.task.clone()),
        kind: EventKind::IntegrateStarted {
            merge_bead: merge_bead.clone(),
        },
    });

    let worktree = repo.worktree_add(branch, head).await?;
    let attempt = land(repo, runner, cfg, &worktree).await;
    repo.worktree_remove(worktree).await?;

    match attempt {
        Ok(new_head) => {
            apply_event(
                store,
                &meta.task,
                Event::Merged {
                    merged: new_head.clone(),
                },
            )
            .await?;
            store
                .close(merge_bead, &format!("landed on {} as {new_head}", cfg.main))
                .await?;
            Ok(Some(Landed::Yes(new_head)))
        }
        Err(LandError::Rejected(rejection)) => {
            let detail = rejection.to_string();
            apply_event(
                store,
                &meta.task,
                Event::MergeFailed {
                    detail: detail.clone(),
                },
            )
            .await?;
            store
                .close(merge_bead, "integration failed; task reopened")
                .await?;
            Ok(Some(Landed::No(rejection)))
        }
        // Infrastructure trouble (remote down, git broken): leave everything as it was and retry later.
        Err(LandError::Infra(e)) => Err(TransitionError::Repo(e)),
        Err(LandError::Runner(e)) => Err(TransitionError::Run(e)),
    }
}

enum LandError {
    /// The branch itself is at fault: conflict or failing checks. Reopens the task.
    Rejected(LandRejection),
    /// Our side is at fault. Nothing changes; try again next pass.
    Infra(RepoError),
    /// The check runner could not start.
    Runner(RunError),
}

/// Rebase → checks → fast-forward → push. Returns the sha now at the tip of `main`.
async fn land(
    repo: &dyn Repo,
    runner: &dyn Runner,
    cfg: &IntegrateConfig,
    worktree: &crate::ports::Worktree,
) -> Result<domain::Sha, LandError> {
    // Land onto what the remote has: a hand commit or another rig's push must be rebased over,
    // not rejected three times a poll. A diverged branch is the operator's call.
    if let Some(remote) = &cfg.remote {
        match repo.sync_branch(remote, &cfg.main).await {
            Ok(crate::ports::RemoteSync::UpToDate) => {}
            Ok(crate::ports::RemoteSync::FastForwarded { to }) => {
                tracing::info!(branch = %cfg.main, %to, "integration branch fast-forwarded from the remote");
            }
            Ok(crate::ports::RemoteSync::Diverged { local, remote }) => {
                return Err(LandError::Rejected(LandRejection::Diverged {
                    branch: cfg.main.clone(),
                    local,
                    remote,
                }));
            }
            Err(e) => return Err(LandError::Infra(e)),
        }
    }
    let new_head = match repo.rebase(worktree, &cfg.main).await {
        Ok(sha) => sha,
        Err(RepoError::Conflict { paths }) => {
            return Err(LandError::Rejected(LandRejection::Conflict {
                onto: cfg.main.clone(),
                paths,
            }));
        }
        Err(
            e @ (RepoError::RefNotFound { .. }
            | RepoError::NotFastForward { .. }
            | RepoError::Rejected { .. }
            | RepoError::Unavailable { .. }),
        ) => {
            return Err(LandError::Infra(e));
        }
    };

    for check in &cfg.checks {
        let out = runner
            .run(&worktree.path, check.as_ref(), cfg.check_timeout)
            .await
            .map_err(LandError::Runner)?;
        if !out.succeeded() {
            let tail = out
                .stderr
                .chars()
                .rev()
                .take(2000)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>();
            return Err(LandError::Rejected(LandRejection::CheckFailed {
                command: check.clone(),
                exit_code: out.exit_code,
                timed_out: out.timed_out,
                tail,
            }));
        }
    }

    // Saga: fast-forward is the first step with a side effect; push is the second. If push
    // fails, compensate by moving `main` back so a retry starts from a clean state.
    let before = repo.head_of(&cfg.main).await.map_err(LandError::Infra)?;
    match repo.fast_forward(&cfg.main, &new_head).await {
        Ok(()) => {}
        // Someone else moved main between rebase and ff: not the branch's fault, retry next pass.
        Err(e) => return Err(LandError::Infra(e)),
    }
    if let Some(remote) = &cfg.remote
        && let Err(push_err) = repo.push(remote, &cfg.main).await
    {
        match repo.rollback(&cfg.main, &new_head, &before).await {
            Ok(()) => {
                tracing::warn!(error = %push_err, "push failed; main rolled back");
            }
            Err(rb) => {
                tracing::error!(error = %push_err, rollback = %rb, "push failed and rollback failed; main is ahead of the remote");
            }
        }
        return Err(LandError::Infra(push_err));
    }
    Ok(new_head)
}

#[cfg(test)]
#[path = "integrator_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "integrator_protected_tests.rs"]
mod protected_tests;
